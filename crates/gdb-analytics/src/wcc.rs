use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::HashMap;

/// Disjoint-Set / Union-Find with path halving and rank heuristic.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    #[inline]
    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    #[inline]
    fn union(&mut self, x: usize, y: usize) {
        let root_x = self.find(x);
        let root_y = self.find(y);
        if root_x == root_y {
            return;
        }

        match self.rank[root_x].cmp(&self.rank[root_y]) {
            std::cmp::Ordering::Less => self.parent[root_x] = root_y,
            std::cmp::Ordering::Greater => self.parent[root_y] = root_x,
            std::cmp::Ordering::Equal => {
                // Deterministic root tie-breaking by smaller dense index
                if root_x < root_y {
                    self.parent[root_y] = root_x;
                    self.rank[root_x] += 1;
                } else {
                    self.parent[root_x] = root_y;
                    self.rank[root_y] += 1;
                }
            }
        }
    }
}

/// Computes Weakly Connected Components (WCC) for all vertices in the graph.
/// Returns a map: VertexId -> ComponentId (represented by canonical root VertexId).
pub fn weakly_connected_components(csr: &ChunkedCsr) -> HashMap<VertexId, VertexId> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return HashMap::new();
    }

    let mut uf = UnionFind::new(num_v);

    for u_idx in 0..num_v {
        let start = csr.offsets[u_idx] as usize;
        let end = (csr.offsets[u_idx + 1] as usize).min(csr.targets.len());

        for off in start..end {
            if let Some(v_idx) = csr.dense_target_at(off) {
                uf.union(u_idx, v_idx);
            }
        }
    }

    let mut result = HashMap::with_capacity(num_v);
    for u_idx in 0..num_v {
        let root_idx = uf.find(u_idx);
        let vid = VertexId(csr.reverse_map[u_idx]);
        let root_vid = VertexId(csr.reverse_map[root_idx]);
        result.insert(vid, root_vid);
    }

    result
}
