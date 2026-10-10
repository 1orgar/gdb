use crate::csr::ChunkedCsr;
use crate::delta::DeltaMemTable;
use crate::properties::VertexPropertyTable;
use gdb_core::schema::GraphSchema;
use gdb_core::{DataValue, EdgeId, EdgeType, GdbError, GdbResult, LabelId, VertexId};
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Storage engine for a single graph partition.
/// Manages high-throughput concurrent OLTP mutations and fast OLAP queries
/// via a unified Dual-Store (DeltaMemTable + ChunkedCSR + Arrow Properties).
pub struct PartitionStorageEngine {
    pub partition_id: u32,
    schema: Arc<RwLock<GraphSchema>>,
    /// Compacted, immutable, cache-aligned CSR topology
    csr: RwLock<Arc<ChunkedCsr>>,
    /// Concurrent lock-free Delta MemTable for mutations
    delta: Arc<DeltaMemTable>,
    /// Property tables indexed by vertex LabelId
    vertex_properties: RwLock<HashMap<LabelId, Arc<VertexPropertyTable>>>,
}

impl PartitionStorageEngine {
    pub fn new(partition_id: u32, schema: Arc<RwLock<GraphSchema>>) -> Self {
        Self {
            partition_id,
            schema,
            csr: RwLock::new(Arc::new(ChunkedCsr::new())),
            delta: Arc::new(DeltaMemTable::new()),
            vertex_properties: RwLock::new(HashMap::new()),
        }
    }

    /// Allocates next commit version.
    pub fn next_commit_version(&self) -> u64 {
        self.delta.next_commit_version()
    }

    /// Latest snapshot version.
    pub fn latest_version(&self) -> u64 {
        self.delta.latest_version()
    }

    /// Inserts or updates an edge in the partition.
    pub fn insert_edge(&self, edge: EdgeId, version: u64) {
        self.delta.insert_edge(edge, version);
    }

    /// Deletes an edge (tombstone) at given commit version.
    pub fn delete_edge(&self, edge: EdgeId, version: u64) {
        self.delta.delete_edge(edge, version);
    }

    /// Sets properties for a vertex under a given label.
    pub fn set_vertex_properties(
        &self,
        vid: VertexId,
        label_id: LabelId,
        props: HashMap<String, DataValue>,
    ) -> GdbResult<()> {
        let table = self.get_or_create_property_table(label_id)?;
        table.set_properties(vid, props);
        Ok(())
    }

    /// Reads a vertex property by name.
    pub fn get_vertex_property(
        &self,
        vid: VertexId,
        label_id: LabelId,
        prop_name: &str,
    ) -> Option<DataValue> {
        let guard = self.vertex_properties.read();
        guard.get(&label_id).and_then(|t| t.get_property(vid, prop_name))
    }

    /// Creates a secondary index for vertices under a given label.
    pub fn create_vertex_index(&self, label_id: LabelId, prop_name: &str) -> GdbResult<()> {
        let table = self.get_or_create_property_table(label_id)?;
        table.create_index(prop_name);
        Ok(())
    }

    /// Drops a secondary index for vertices under a given label.
    pub fn drop_vertex_index(&self, label_id: LabelId, prop_name: &str) -> GdbResult<bool> {
        let guard = self.vertex_properties.read();
        if let Some(table) = guard.get(&label_id) {
            Ok(table.drop_index(prop_name))
        } else {
            Ok(false)
        }
    }

    /// Checks if a secondary index exists on a property for a given label.
    pub fn has_vertex_index(&self, label_id: LabelId, prop_name: &str) -> bool {
        let guard = self.vertex_properties.read();
        guard.get(&label_id).map(|t| t.has_index(prop_name)).unwrap_or(false)
    }

    /// Point lookup of vertices by property value via secondary index.
    pub fn lookup_vertex_by_index(
        &self,
        label_id: LabelId,
        prop_name: &str,
        value: &DataValue,
    ) -> Option<Vec<VertexId>> {
        let guard = self.vertex_properties.read();
        guard.get(&label_id).and_then(|t| t.lookup_by_index(prop_name, value))
    }

    /// Updates a single property on a vertex under a label.
    pub fn update_vertex_property(
        &self,
        vid: VertexId,
        label_id: LabelId,
        prop_name: &str,
        value: DataValue,
    ) -> GdbResult<()> {
        let table = self.get_or_create_property_table(label_id)?;
        table.update_property(vid, prop_name, value);
        Ok(())
    }

    /// Deletes a vertex from property storage, and optionally deletes all incident edges.
    pub fn delete_vertex(&self, vid: VertexId, label_id: LabelId, detach: bool) -> GdbResult<bool> {
        if detach {
            let snapshot = self.latest_version();
            let ver = self.next_commit_version();
            let out_edges = self.get_out_edges(vid, None, snapshot);
            for e in out_edges {
                self.delete_edge(e, ver);
            }
        }
        let guard = self.vertex_properties.read();
        if let Some(table) = guard.get(&label_id) {
            Ok(table.delete_vertex(vid))
        } else {
            Ok(false)
        }
    }

    /// Drops all stored properties and indexes for an entire vertex label.
    pub fn drop_vertex_label(&self, label_id: LabelId) {
        let mut guard = self.vertex_properties.write();
        guard.remove(&label_id);
    }

    /// Drops a single property column from a vertex label.
    pub fn drop_vertex_property(&self, label_id: LabelId, prop_name: &str) {
        let guard = self.vertex_properties.read();
        if let Some(table) = guard.get(&label_id) {
            table.drop_property(prop_name);
        }
    }

    /// Traverses outgoing edges of a vertex combining CSR + Delta MemTable with MVCC visibility.
    pub fn get_out_edges(
        &self,
        src: VertexId,
        filter_type: Option<EdgeType>,
        snapshot: u64,
    ) -> Vec<EdgeId> {
        // 1. Fetch edges from immutable CSR
        let csr = self.csr.read().clone();
        let base_edges = csr.get_out_edges(src, filter_type);

        // 2. Fetch delta active edges and tombstones
        let (delta_active, tombstones) = self.delta.get_out_edges(src, filter_type, snapshot);

        if delta_active.is_empty() && tombstones.is_empty() {
            return base_edges;
        }

        let tombstone_set: HashSet<EdgeId> = tombstones.into_iter().collect();

        // 3. Merge: base_edges (excluding tombstones) + delta_active
        let mut results = Vec::with_capacity(base_edges.len() + delta_active.len());

        for e in base_edges {
            if !tombstone_set.contains(&e) {
                results.push(e);
            }
        }

        for e in delta_active {
            results.push(e);
        }

        results
    }

    /// Performs background compaction: merges DeltaMemTable into a new contiguous CSR.
    pub fn compact(&self) {
        let (delta_edges, tombstones) = self.delta.drain_for_compaction();

        let mut all_edges = Vec::new();

        // Collect all edges from existing CSR, excluding tombstones
        let current_csr = self.csr.read().clone();
        for &src_raw in &current_csr.reverse_map {
            let src = VertexId(src_raw);
            let edges = current_csr.get_out_edges(src, None);
            for e in edges {
                if !tombstones.contains(&e) {
                    all_edges.push(e);
                }
            }
        }

        // Add active delta edges
        all_edges.extend(delta_edges);

        // Deduplicate edges
        all_edges.sort_unstable_by(|a, b| {
            a.src.cmp(&b.src)
                .then(a.edge_type.0.cmp(&b.edge_type.0))
                .then(a.rank.cmp(&b.rank))
                .then(a.dst.cmp(&b.dst))
        });
        all_edges.dedup();

        // Build new dense CSR
        let new_csr = Arc::new(ChunkedCsr::from_edges(all_edges));
        *self.csr.write() = new_csr;

        // Clear delta table
        self.delta.clear();
    }

    /// Rebuilds Arrow columnar batches for all registered property tables.
    pub fn rebuild_arrow_batches(&self) -> GdbResult<()> {
        let guard = self.vertex_properties.read();
        for table in guard.values() {
            table.rebuild_arrow_batches()?;
        }
        Ok(())
    }

    /// Gets property table by label_id.
    pub fn get_property_table(&self, label_id: LabelId) -> Option<Arc<VertexPropertyTable>> {
        self.vertex_properties.read().get(&label_id).cloned()
    }

    /// Returns all known vertex IDs combining CSR and in-memory property tables.
    pub fn get_all_vertex_ids(&self, label_id: Option<LabelId>) -> Vec<VertexId> {
        let mut ids = HashSet::new();
        let csr = self.csr.read();
        for &vid_raw in &csr.reverse_map {
            ids.insert(VertexId(vid_raw));
        }
        let guard = self.vertex_properties.read();
        if let Some(lid) = label_id {
            if let Some(table) = guard.get(&lid) {
                for vid in table.vertex_ids() {
                    ids.insert(vid);
                }
            }
        } else {
            for table in guard.values() {
                for vid in table.vertex_ids() {
                    ids.insert(vid);
                }
            }
        }
        ids.into_iter().collect()
    }

    /// Checks whether a specific vertex ID exists in the graph.
    pub fn has_vertex(&self, vid: VertexId, label_id: Option<LabelId>) -> bool {
        let csr = self.csr.read();
        if csr.vertex_map.contains_key(&vid.as_u64()) {
            return true;
        }
        let guard = self.vertex_properties.read();
        if let Some(lid) = label_id {
            if let Some(table) = guard.get(&lid) {
                if table.has_vertex(vid) {
                    return true;
                }
            }
        } else {
            for table in guard.values() {
                if table.has_vertex(vid) {
                    return true;
                }
            }
        }
        if self.delta.contains_vertex(vid) {
            return true;
        }
        false
    }

    fn get_or_create_property_table(&self, label_id: LabelId) -> GdbResult<Arc<VertexPropertyTable>> {
        {
            let guard = self.vertex_properties.read();
            if let Some(t) = guard.get(&label_id) {
                return Ok(t.clone());
            }
        }

        let schema_guard = self.schema.read();
        let v_schema = schema_guard
            .get_vertex_schema_by_id(label_id)
            .ok_or_else(|| GdbError::Schema(format!("Schema not found for {:?}", label_id)))?;

        let table = Arc::new(VertexPropertyTable::new(v_schema));
        let mut guard = self.vertex_properties.write();
        guard.insert(label_id, table.clone());
        Ok(table)
    }

    /// Exposes read-only reference to current CSR (e.g. for GPU zero-copy buffer mapping).
    pub fn current_csr(&self) -> Arc<ChunkedCsr> {
        self.csr.read().clone()
    }

    /// Total active edges across CSR and Delta MemTable.
    pub fn total_edges(&self) -> usize {
        self.current_csr().edge_count() + self.delta.edge_count()
    }

    /// Edges stored in immutable Chunked-CSR.
    pub fn csr_edges(&self) -> usize {
        self.current_csr().edge_count()
    }

    /// Edges currently in Delta MemTable waiting for compaction.
    pub fn delta_edges(&self) -> usize {
        self.delta.edge_count()
    }

    /// Total distinct vertices in CSR or property tables.
    pub fn total_vertices(&self) -> usize {
        let csr_cnt = self.current_csr().vertex_count();
        let prop_cnt: usize = self.vertex_properties.read().values().map(|t| t.len()).sum();
        csr_cnt.max(prop_cnt)
    }

    /// Performs vector similarity search over property tables for a given vertex label.
    pub fn vector_similarity_search(
        &self,
        label_id: LabelId,
        property_name: &str,
        query: &[f32],
        k: usize,
        metric: &str,
    ) -> Vec<(VertexId, f32)> {
        if let Some(table) = self.vertex_properties.read().get(&label_id) {
            table.vector_similarity_search(property_name, query, k, metric)
        } else {
            Vec::new()
        }
    }

    /// Computes full graph statistics (vertex counts, degree distribution, edges) for CBO optimization.
    pub fn analyze_graph(&self) -> GraphStatistics {
        let total_v = self.total_vertices();
        let total_e = self.total_edges();
        let mut v_per_label = HashMap::new();
        let mut e_per_type = HashMap::new();

        {
            let schema = self.schema.read();
            for s in schema.list_vertex_schemas().into_iter() {
                let cnt = self.vertex_properties.read().get(&s.label_id).map(|t| t.len()).unwrap_or(0);
                v_per_label.insert(s.label.clone(), cnt);
            }
            for s in schema.list_edge_schemas().into_iter() {
                e_per_type.insert(s.edge_type_name.clone(), total_e);
            }
        }

        let csr = self.csr.read();
        let mut max_deg = 0;
        let csr_v = csr.vertex_count();
        if csr_v > 0 {
            for i in 0..csr_v {
                let deg = (csr.offsets[i + 1] - csr.offsets[i]) as usize;
                if deg > max_deg {
                    max_deg = deg;
                }
            }
        }

        let avg_deg = if total_v > 0 {
            total_e as f64 / total_v as f64
        } else {
            0.0
        };

        GraphStatistics {
            total_vertices: total_v,
            total_edges: total_e,
            vertices_per_label: v_per_label,
            edges_per_type: e_per_type,
            avg_degree: avg_deg,
            max_degree: max_deg,
        }
    }
}

/// Graph cardinality and distribution statistics for Cost-Based Optimization (CBO).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphStatistics {
    pub total_vertices: usize,
    pub total_edges: usize,
    pub vertices_per_label: HashMap<String, usize>,
    pub edges_per_type: HashMap<String, usize>,
    pub avg_degree: f64,
    pub max_degree: usize,
}

