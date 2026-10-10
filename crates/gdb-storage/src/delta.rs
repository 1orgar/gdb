use dashmap::DashMap;
use gdb_core::{EdgeId, EdgeRank, EdgeType, VertexId};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MvccEdge {
    pub dst: VertexId,
    pub edge_type: EdgeType,
    pub rank: EdgeRank,
    pub version: u64,
    pub is_deleted: bool,
}

/// In-Memory Lock-Free Delta MemTable for high-throughput OLTP mutations.
pub struct DeltaMemTable {
    /// Outgoing edges mapped by source VertexId.
    /// SmallVec optimizes the common case where a vertex has few new edges in delta.
    out_edges: DashMap<u64, SmallVec<[MvccEdge; 4]>, ahash::RandomState>,
    /// Global or partition-level monotonic commit version counter.
    current_version: AtomicU64,
}

impl Default for DeltaMemTable {
    fn default() -> Self {
        Self::new()
    }
}

impl DeltaMemTable {
    pub fn new() -> Self {
        Self {
            out_edges: DashMap::with_hasher(ahash::RandomState::new()),
            current_version: AtomicU64::new(1),
        }
    }

    /// Allocates next commit sequence number.
    #[inline(always)]
    pub fn next_commit_version(&self) -> u64 {
        self.current_version.fetch_add(1, Ordering::SeqCst)
    }

    /// Current snapshot version.
    #[inline(always)]
    pub fn latest_version(&self) -> u64 {
        self.current_version.load(Ordering::SeqCst)
    }

    /// Inserts an edge into the delta buffer with a commit version.
    pub fn insert_edge(&self, edge: EdgeId, version: u64) {
        let src_raw = edge.src.as_u64();
        let mvcc_edge = MvccEdge {
            dst: edge.dst,
            edge_type: edge.edge_type,
            rank: edge.rank,
            version,
            is_deleted: false,
        };

        let mut entry = self.out_edges.entry(src_raw).or_default();
        if let Some(existing) = entry.iter_mut().find(|e| {
            e.edge_type == edge.edge_type && e.rank == edge.rank && e.dst == edge.dst
        }) {
            if version >= existing.version {
                *existing = mvcc_edge;
            }
        } else {
            entry.push(mvcc_edge);
        }
    }

    /// Marks an edge as deleted (tombstone) at given commit version.
    pub fn delete_edge(&self, edge: EdgeId, version: u64) {
        let src_raw = edge.src.as_u64();
        let mvcc_edge = MvccEdge {
            dst: edge.dst,
            edge_type: edge.edge_type,
            rank: edge.rank,
            version,
            is_deleted: true,
        };

        let mut entry = self.out_edges.entry(src_raw).or_default();
        if let Some(existing) = entry.iter_mut().find(|e| {
            e.edge_type == edge.edge_type && e.rank == edge.rank && e.dst == edge.dst
        }) {
            if version >= existing.version {
                *existing = mvcc_edge;
            }
        } else {
            entry.push(mvcc_edge);
        }
    }

    /// Retrieves active delta edges visible at a specific snapshot version.
    pub fn get_out_edges(
        &self,
        src: VertexId,
        filter_type: Option<EdgeType>,
        snapshot: u64,
    ) -> (Vec<EdgeId>, Vec<EdgeId>) {
        let src_raw = src.as_u64();
        let Some(entry) = self.out_edges.get(&src_raw) else {
            return (Vec::new(), Vec::new());
        };

        let mut active = Vec::new();
        let mut tombstones = Vec::new();

        for e in entry.iter() {
            if e.version <= snapshot {
                if let Some(et) = filter_type {
                    if e.edge_type != et {
                        continue;
                    }
                }
                let edge_id = EdgeId::new(src, e.edge_type, e.rank, e.dst);
                if e.is_deleted {
                    tombstones.push(edge_id);
                } else {
                    active.push(edge_id);
                }
            }
        }

        (active, tombstones)
    }

    /// Drains all active edges and tombstones for compaction.
    pub fn drain_for_compaction(&self) -> (Vec<EdgeId>, HashSet<EdgeId>) {
        let mut active = Vec::new();
        let mut tombstones = HashSet::new();

        for item in self.out_edges.iter() {
            let src = VertexId(*item.key());
            for e in item.value().iter() {
                let edge_id = EdgeId::new(src, e.edge_type, e.rank, e.dst);
                if e.is_deleted {
                    tombstones.insert(edge_id);
                } else {
                    active.push(edge_id);
                }
            }
        }

        (active, tombstones)
    }

    /// Clears the delta table after successful compaction.
    pub fn clear(&self) {
        self.out_edges.clear();
    }

    /// Returns the total number of edges currently in the Delta MemTable.
    pub fn edge_count(&self) -> usize {
        self.out_edges.iter().map(|entry| entry.value().len()).sum()
    }

    /// Checks whether a vertex has outgoing or incoming delta mutations.
    pub fn contains_vertex(&self, vid: VertexId) -> bool {
        let vid_raw = vid.as_u64();
        if self.out_edges.contains_key(&vid_raw) {
            return true;
        }
        for entry in self.out_edges.iter() {
            if entry.value().iter().any(|e| e.dst == vid) {
                return true;
            }
        }
        false
    }
}

