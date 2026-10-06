use serde::{Deserialize, Serialize};
use std::fmt;
use std::hash::{Hash, Hasher};

/// 64-bit Vertex identifier.
/// Can represent a direct u64 ID or a 64-bit hash of a string ID.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct VertexId(pub u64);

impl VertexId {
    pub const MIN: VertexId = VertexId(u64::MIN);
    pub const MAX: VertexId = VertexId(u64::MAX);

    #[inline(always)]
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    /// Creates a VertexId by hashing a string key using a fast hash function.
    pub fn from_str_key(key: &str) -> Self {
        let mut hasher = ahash::AHasher::default();
        key.hash(&mut hasher);
        VertexId(hasher.finish())
    }

    /// Computes the partition ID for this vertex given total partitions.
    #[inline(always)]
    pub fn partition(&self, total_partitions: u32) -> u32 {
        debug_assert!(total_partitions > 0);
        (self.0 % (total_partitions as u64)) as u32
    }
}

impl fmt::Debug for VertexId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v#{}", self.0)
    }
}

impl fmt::Display for VertexId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for VertexId {
    #[inline(always)]
    fn from(id: u64) -> Self {
        VertexId(id)
    }
}

impl From<i64> for VertexId {
    #[inline(always)]
    fn from(id: i64) -> Self {
        VertexId(id as u64)
    }
}

/// 32-bit Identifier for Vertex Labels (e.g. Person, Company).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct LabelId(pub u32);

impl fmt::Debug for LabelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Label#{}", self.0)
    }
}

/// 32-bit Identifier for Edge Types (e.g. KNOWS, FOLLOWS).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct EdgeType(pub u32);

impl fmt::Debug for EdgeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EdgeType#{}", self.0)
    }
}

/// Multi-edge differentiator (Rank). Default is 0.
pub type EdgeRank = i64;

/// Complete Edge Identifier: (src, edge_type, rank, dst).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct EdgeId {
    pub src: VertexId,
    pub edge_type: EdgeType,
    pub rank: EdgeRank,
    pub dst: VertexId,
}

impl EdgeId {
    #[inline(always)]
    pub fn new(src: VertexId, edge_type: EdgeType, rank: EdgeRank, dst: VertexId) -> Self {
        Self { src, edge_type, rank, dst }
    }

    #[inline(always)]
    pub fn simple(src: VertexId, edge_type: EdgeType, dst: VertexId) -> Self {
        Self { src, edge_type, rank: 0, dst }
    }
}

impl fmt::Debug for EdgeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({:?})-[{:?}:#{}]->({:?})", self.src, self.edge_type, self.rank, self.dst)
    }
}

/// Traversal direction for edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Out,
    In,
    Both,
}
