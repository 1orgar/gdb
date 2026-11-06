use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use rayon::prelude::*;
use std::collections::{HashMap, VecDeque};

/// Parallel Harmonic Closeness Centrality: C(u) = (1 / (n - 1)) * sum_{v != u} (1 / dist(u, v)).
/// Robust for both connected and disconnected graphs. Uses pivot reverse-BFS sampling when `|V| > 2000`.
pub fn closeness_centrality(csr: &ChunkedCsr) -> HashMap<VertexId, f64> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return HashMap::new();
    }

    let closeness: Vec<f64> = if num_v <= 2000 {
        (0..num_v)
            .into_par_iter()
            .map(|s| {
                if num_v <= 1 {
                    return 0.0;
                }
                let mut dist = vec![-1i32; num_v];
                dist[s] = 0;
                let mut queue = VecDeque::with_capacity(num_v);
                queue.push_back(s);

                let mut sum_reciprocal = 0.0f64;

                while let Some(u) = queue.pop_front() {
                    let start = csr.offsets[u] as usize;
                    let end = (csr.offsets[u + 1] as usize).min(csr.targets.len());

                    for off in start..end {
                        if let Some(v) = csr.dense_target_at(off) {
                            if dist[v] < 0 {
                                dist[v] = dist[u] + 1;
                                sum_reciprocal += 1.0 / (dist[v] as f64);
                                queue.push_back(v);
                            }
                        }
                    }
                }

                sum_reciprocal / ((num_v - 1) as f64)
            })
            .collect()
    } else {
        // Build reverse adjacency list once in O(V + E) and run k = 32 parallel reverse-BFS passes
        let mut rev_adj: Vec<Vec<u32>> = vec![Vec::new(); num_v];
        for u in 0..num_v {
            let start = csr.offsets[u] as usize;
            let end = (csr.offsets[u + 1] as usize).min(csr.targets.len());
            for off in start..end {
                if let Some(v) = csr.dense_target_at(off) {
                    rev_adj[v].push(u as u32);
                }
            }
        }
        let sample_k = 32usize;
        let pivots: Vec<usize> = (0..sample_k).map(|i| (i * num_v) / sample_k).collect();
        let mut sum_rec = pivots
            .into_par_iter()
            .fold(
                || vec![0.0f64; num_v],
                |mut local_sum, t| {
                    let mut dist = vec![-1i32; num_v];
                    dist[t] = 0;
                    let mut queue = VecDeque::with_capacity(4096);
                    queue.push_back(t);
                    while let Some(curr) = queue.pop_front() {
                        let d_next = dist[curr] + 1;
                        for &prev_u32 in &rev_adj[curr] {
                            let prev = prev_u32 as usize;
                            if dist[prev] < 0 {
                                dist[prev] = d_next;
                                local_sum[prev] += 1.0 / (d_next as f64);
                                queue.push_back(prev);
                            }
                        }
                    }
                    local_sum
                },
            )
            .reduce(
                || vec![0.0f64; num_v],
                |mut acc, local_sum| {
                    for i in 0..num_v {
                        acc[i] += local_sum[i];
                    }
                    acc
                },
            );
        let inv_k = 1.0 / (sample_k as f64);
        for v in sum_rec.iter_mut() {
            *v *= inv_k;
        }
        sum_rec
    };

    let mut result = HashMap::with_capacity(num_v);
    for i in 0..num_v {
        result.insert(VertexId(csr.reverse_map[i]), closeness[i]);
    }
    result
}
