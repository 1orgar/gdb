use gdb_core::{EdgeId, EdgeType, VertexId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// High-performance Compressed Sparse Row (CSR) graph topology storage.
/// Edges are stored in dense, contiguous arrays aligned for CPU cache lines
/// and direct Zero-Copy transfer to GPU compute buffers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChunkedCsr {
    /// Mapping from dense local vertex index to offset in `targets`.
    /// Length is `num_vertices + 1`.
    pub offsets: Vec<u32>,
    /// Contiguous array of destination vertex IDs.
    pub targets: Vec<u64>,
    /// Contiguous array of edge types.
    pub edge_types: Vec<u32>,
    /// Contiguous array of edge ranks.
    pub ranks: Vec<i64>,
    /// Mapping from original 64-bit VertexId to dense local vertex index.
    pub vertex_map: HashMap<u64, u32>,
    /// Mapping from dense local vertex index back to original VertexId.
    pub reverse_map: Vec<u64>,
}

impl ChunkedCsr {
    pub fn new() -> Self {
        Self {
            offsets: vec![0],
            targets: Vec::new(),
            edge_types: Vec::new(),
            ranks: Vec::new(),
            vertex_map: HashMap::new(),
            reverse_map: Vec::new(),
        }
    }

    /// Total number of vertices in this CSR chunk.
    #[inline(always)]
    pub fn vertex_count(&self) -> usize {
        self.reverse_map.len()
    }

    /// Total number of edges in this CSR chunk.
    #[inline(always)]
    pub fn edge_count(&self) -> usize {
        self.targets.len()
    }

    /// Gets or assigns a dense index for a VertexId during CSR construction.
    fn get_or_create_index(
        vertex_map: &mut HashMap<u64, u32>,
        reverse_map: &mut Vec<u64>,
        vid: VertexId,
    ) -> u32 {
        let raw = vid.as_u64();
        if let Some(&idx) = vertex_map.get(&raw) {
            idx
        } else {
            let idx = reverse_map.len() as u32;
            vertex_map.insert(raw, idx);
            reverse_map.push(raw);
            idx
        }
    }

    /// Builds a static, cache-aligned CSR from a sorted list of edges.
    pub fn from_edges(mut edges: Vec<EdgeId>) -> Self {
        if edges.is_empty() {
            return Self::new();
        }

        // Sort edges by source vertex, then edge_type, then rank, then dst
        edges.sort_unstable_by(|a, b| {
            a.src
                .cmp(&b.src)
                .then(a.edge_type.0.cmp(&b.edge_type.0))
                .then(a.rank.cmp(&b.rank))
                .then(a.dst.cmp(&b.dst))
        });

        let mut vertex_map = HashMap::new();
        let mut reverse_map = Vec::new();

        // 1. Assign dense indices
        for edge in &edges {
            Self::get_or_create_index(&mut vertex_map, &mut reverse_map, edge.src);
            Self::get_or_create_index(&mut vertex_map, &mut reverse_map, edge.dst);
        }

        let num_vertices = reverse_map.len();
        let num_edges = edges.len();

        let mut offsets = vec![0u32; num_vertices + 1];
        let mut targets = Vec::with_capacity(num_edges);
        let mut edge_types = Vec::with_capacity(num_edges);
        let mut ranks = Vec::with_capacity(num_edges);

        // Count degree per vertex
        for edge in &edges {
            let src_idx = vertex_map[&edge.src.as_u64()] as usize;
            offsets[src_idx + 1] += 1;
        }

        // Prefix sum for offsets
        for i in 0..num_vertices {
            offsets[i + 1] += offsets[i];
        }

        // Fill CSR targets, edge_types, ranks
        let mut current_offset = offsets.clone();
        for edge in edges {
            let src_idx = vertex_map[&edge.src.as_u64()] as usize;
            let pos = current_offset[src_idx] as usize;
            current_offset[src_idx] += 1;

            if pos >= targets.len() {
                targets.resize(pos + 1, 0);
                edge_types.resize(pos + 1, 0);
                ranks.resize(pos + 1, 0);
            }

            targets[pos] = edge.dst.as_u64();
            edge_types[pos] = edge.edge_type.0;
            ranks[pos] = edge.rank;
        }

        Self {
            offsets,
            targets,
            edge_types,
            ranks,
            vertex_map,
            reverse_map,
        }
    }

    /// Fast contiguous slice lookup of outgoing edges for a given source vertex.
    /// This is cache-friendly and vectorizable.
    #[inline]
    pub fn get_out_edges(&self, src: VertexId, filter_type: Option<EdgeType>) -> Vec<EdgeId> {
        let src_raw = src.as_u64();
        let Some(&src_idx) = self.vertex_map.get(&src_raw) else {
            return Vec::new();
        };

        let start = self.offsets[src_idx as usize] as usize;
        let end = self.offsets[(src_idx + 1) as usize] as usize;

        if start >= end || start >= self.targets.len() {
            return Vec::new();
        }

        let actual_end = end.min(self.targets.len());
        let count = actual_end - start;
        let mut results = Vec::with_capacity(count);

        let target_slice = &self.targets[start..actual_end];
        let type_slice = &self.edge_types[start..actual_end];
        let rank_slice = &self.ranks[start..actual_end];

        for i in 0..count {
            let et = EdgeType(type_slice[i]);
            if let Some(expected_type) = filter_type {
                if et != expected_type {
                    continue;
                }
            }
            results.push(EdgeId::new(
                src,
                et,
                rank_slice[i],
                VertexId(target_slice[i]),
            ));
        }

        results
    }
}
