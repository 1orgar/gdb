use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use rayon::prelude::*;
use std::collections::{HashMap, VecDeque};

/// Parallel Brandes' algorithm for Betweenness Centrality on unweighted graphs.
/// Automatically uses deterministic pivot sampling (`k = 32`) when `|V| > 2000`.
pub fn betweenness_centrality(csr: &ChunkedCsr, normalized: bool) -> HashMap<VertexId, f64> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return HashMap::new();
    }

    let sample_k = if num_v <= 2000 { num_v } else { 32 };
    let pivots: Vec<usize> = if sample_k == num_v {
        (0..num_v).collect()
    } else {
        (0..sample_k).map(|i| (i * num_v) / sample_k).collect()
    };

    let mut cb = pivots
        .into_par_iter()
        .fold(
            || vec![0.0f64; num_v],
            |mut local_cb, s| {
                let mut stack = Vec::with_capacity(num_v.min(4096));
                let mut p: Vec<Vec<usize>> = vec![Vec::new(); num_v];
                let mut sigma = vec![0.0f64; num_v];
                sigma[s] = 1.0;
                let mut d = vec![-1i32; num_v];
                d[s] = 0;

                let mut queue = VecDeque::with_capacity(num_v.min(4096));
                queue.push_back(s);

                while let Some(v) = queue.pop_front() {
                    stack.push(v);

                    let start = csr.offsets[v] as usize;
                    let end = (csr.offsets[v + 1] as usize).min(csr.targets.len());

                    for off in start..end {
                        if let Some(w) = csr.dense_target_at(off) {
                            if d[w] < 0 {
                                d[w] = d[v] + 1;
                                queue.push_back(w);
                            }

                            if d[w] == d[v] + 1 {
                                sigma[w] += sigma[v];
                                p[w].push(v);
                            }
                        }
                    }
                }

                let mut delta = vec![0.0f64; num_v];
                while let Some(w) = stack.pop() {
                    for &v in &p[w] {
                        delta[v] += (sigma[v] / sigma[w]) * (1.0 + delta[w]);
                    }
                    if w != s {
                        local_cb[w] += delta[w];
                    }
                }

                local_cb
            },
        )
        .reduce(
            || vec![0.0f64; num_v],
            |mut acc, local_cb| {
                for i in 0..num_v {
                    acc[i] += local_cb[i];
                }
                acc
            },
        );

    let scale = (num_v as f64) / (sample_k as f64);
    let norm_factor = if normalized && num_v > 2 {
        scale / (((num_v - 1) as f64) * ((num_v - 2) as f64))
    } else {
        scale
    };
    if (norm_factor - 1.0).abs() > 1e-12 {
        for val in cb.iter_mut() {
            *val *= norm_factor;
        }
    }

    let mut result = HashMap::with_capacity(num_v);
    for i in 0..num_v {
        result.insert(VertexId(csr.reverse_map[i]), cb[i]);
    }
    result
}
