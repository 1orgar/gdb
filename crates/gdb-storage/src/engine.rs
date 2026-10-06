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

    /// Total distinct vertices in CSR.
    pub fn total_vertices(&self) -> usize {
        self.current_csr().vertex_count()
    }
}

