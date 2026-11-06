use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use rayon::prelude::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriangleMetric {
    pub triangles: u64,
    pub clustering_coefficient: f64,
}

/// Computes Triangle Count and Local Clustering Coefficient (LCC) in parallel across all vertices.
pub fn triangle_count(csr: &ChunkedCsr) -> HashMap<VertexId, TriangleMetric> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return HashMap::new();
    }

    let adj = csr.build_undirected_adj();

    let triangles: Vec<u64> = (0..num_v)
        .into_par_iter()
        .map(|u| {
            let u_neighbors = &adj[u];
            let deg = u_neighbors.len();
            let mut count = 0u64;

            for i in 0..deg {
                let v = u_neighbors[i] as usize;
                let v_neighbors = &adj[v];

                let mut p1 = i + 1;
                let mut p2 = 0;

                while p1 < deg && p2 < v_neighbors.len() {
                    let w1 = u_neighbors[p1];
                    let w2 = v_neighbors[p2];

                    if w1 == w2 {
                        count += 1;
                        p1 += 1;
                        p2 += 1;
                    } else if w1 < w2 {
                        p1 += 1;
                    } else {
                        p2 += 1;
                    }
                }
            }
            count
        })
        .collect();

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
