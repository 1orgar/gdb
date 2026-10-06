use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::{HashMap, VecDeque};

/// Harmonic Closeness Centrality: C(u) = sum_{v != u} (1 / dist(u, v)).
/// Robust for both connected and disconnected graphs.
pub fn closeness_centrality(csr: &ChunkedCsr) -> HashMap<VertexId, f64> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return HashMap::new();
    }

    let mut closeness = vec![0.0f64; num_v];

    for s in 0..num_v {
        let mut dist = vec![-1i32; num_v];
        dist[s] = 0;
        let mut queue = VecDeque::new();
        queue.push_back(s);

        let mut sum_reciprocal = 0.0f64;

        while let Some(u) = queue.pop_front() {
            let start = csr.offsets[u] as usize;
            let end = csr.offsets[u + 1] as usize;
            let actual_end = end.min(csr.targets.len());

            for off in start..actual_end {
                let target_raw = csr.targets[off];
                if let Some(&v_idx) = csr.vertex_map.get(&target_raw) {
                    let v = v_idx as usize;
                    if dist[v] < 0 {
                        dist[v] = dist[u] + 1;
                        sum_reciprocal += 1.0 / (dist[v] as f64);
                        queue.push_back(v);
                    }
                }
            }
        }

        // Normalize by (n - 1)
        if num_v > 1 {
            closeness[s] = sum_reciprocal / ((num_v - 1) as f64);
        }
    }

    let mut result = HashMap::with_capacity(num_v);
    for i in 0..num_v {
        result.insert(VertexId(csr.reverse_map[i]), closeness[i]);
    }
    result
}
