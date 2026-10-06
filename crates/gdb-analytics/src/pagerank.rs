use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::HashMap;

/// High-precision PageRank algorithm with convergence check.
pub fn pagerank(
    csr: &ChunkedCsr,
    damping: f64,
    max_iter: usize,
    tolerance: f64,
) -> HashMap<VertexId, f64> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return HashMap::new();
    }

    let n = num_v as f64;
    let mut rank = vec![1.0 / n; num_v];
    let mut new_rank = vec![0.0; num_v];

    // Precompute out-degrees
    let mut out_degrees = vec![0u32; num_v];
    for u in 0..num_v {
        let start = csr.offsets[u] as usize;
        let end = csr.offsets[u + 1] as usize;
        out_degrees[u] = end.saturating_sub(start) as u32;
    }

    for _ in 0..max_iter {
        let base = (1.0 - damping) / n;
        new_rank.fill(base);

        let mut dangling_sum = 0.0f64;
        for u in 0..num_v {
            if out_degrees[u] == 0 {
                dangling_sum += rank[u];
            }
        }
        let dangling_contrib = (damping * dangling_sum) / n;
        for val in new_rank.iter_mut() {
            *val += dangling_contrib;
        }

        for u in 0..num_v {
            let deg = out_degrees[u];
            if deg > 0 {
                let contrib = damping * (rank[u] / deg as f64);
                let start = csr.offsets[u] as usize;
                let end = csr.offsets[u + 1] as usize;
                for off in start..end {
                    let target_raw = csr.targets[off];
                    if let Some(&target_idx) = csr.vertex_map.get(&target_raw) {
                        new_rank[target_idx as usize] += contrib;
                    }
                }
            }
        }

        // Check L1 convergence norm: sum(|r_new - r_old|) < tolerance
        let diff: f64 = rank.iter().zip(new_rank.iter()).map(|(a, b)| (a - b).abs()).sum();
        std::mem::swap(&mut rank, &mut new_rank);

        if diff < tolerance {
            break;
        }
    }

    let mut result = HashMap::with_capacity(num_v);
    for i in 0..num_v {
        result.insert(VertexId(csr.reverse_map[i]), rank[i]);
    }
    result
}
