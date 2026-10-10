use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriangleMetric {
    pub triangles: u64,
    pub clustering_coefficient: f64,
}

/// Computes Triangle Count and Local Clustering Coefficient (LCC) for all vertices.
pub fn triangle_count(csr: &ChunkedCsr) -> HashMap<VertexId, TriangleMetric> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return HashMap::new();
    }

    // Pre-extract sorted neighbor index lists for each vertex (undirected projection)
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); num_v];
    for u in 0..num_v {
        let start = csr.offsets[u] as usize;
        let end = csr.offsets[u + 1] as usize;
        let actual_end = end.min(csr.targets.len());

        for off in start..actual_end {
            let target_raw = csr.targets[off];
            if let Some(&v_idx) = csr.vertex_map.get(&target_raw) {
                let v = v_idx as usize;
                if u != v {
                    adj[u].push(v);
                    adj[v].push(u);
                }
            }
        }
    }
    for u in 0..num_v {
        adj[u].sort_unstable();
        adj[u].dedup();
    }

    let mut triangles = vec![0u64; num_v];

    // For each vertex u, count triangles by intersecting adjacency lists
    for u in 0..num_v {
        let u_neighbors = &adj[u];
        let deg = u_neighbors.len();

        for i in 0..deg {
            let v = u_neighbors[i];
            let v_neighbors = &adj[v];

            // Intersect u_neighbors and v_neighbors using two-pointer sweep
            let mut p1 = i + 1;
            let mut p2 = 0;

            while p1 < deg && p2 < v_neighbors.len() {
                let w1 = u_neighbors[p1];
                let w2 = v_neighbors[p2];

                if w1 == w2 {
                    triangles[u] += 1;
                    p1 += 1;
                    p2 += 1;
                } else if w1 < w2 {
                    p1 += 1;
                } else {
                    p2 += 1;
                }
            }
        }
    }

    let mut result = HashMap::with_capacity(num_v);
    for u in 0..num_v {
        let deg = adj[u].len() as f64;
        let t = triangles[u];
        let lcc = if deg > 1.0 {
            (2.0 * t as f64) / (deg * (deg - 1.0))
        } else {
            0.0
        };

        result.insert(
            VertexId(csr.reverse_map[u]),
            TriangleMetric {
                triangles: t,
                clustering_coefficient: lcc,
            },
        );
    }

    result
}
