use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::HashSet;

/// Computes Node2Vec feature representations using biased second-order random walks.
/// Returns a vector of (VertexId, embedding vector of length `dimensions`).
pub fn node2vec(
    csr: &ChunkedCsr,
    dimensions: usize,
    walk_length: usize,
    num_walks: usize,
    p: f64,
    q: f64,
) -> Vec<(VertexId, Vec<f32>)> {
    let num_v = csr.vertex_count();
    if num_v == 0 || dimensions == 0 {
        return Vec::new();
    }

    let inv_p = if p > 0.0 { 1.0 / p } else { 1.0 };
    let inv_q = if q > 0.0 { 1.0 / q } else { 1.0 };

    // Initialize embeddings with reproducible pseudo-random unit vectors
    let mut embeddings: Vec<Vec<f32>> = (0..num_v)
        .map(|idx| {
            let mut vec = Vec::with_capacity(dimensions);
            let mut seed = (idx as u64).wrapping_mul(6364136223846793005).wrapping_add(1);
            for _ in 0..dimensions {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let val = ((seed >> 33) as f32 / (1u64 << 31) as f32) - 1.0;
                vec.push(val);
            }
            normalize(&mut vec);
            vec
        })
        .collect();

    // Generate biased random walks and train via Skip-Gram SGD updates
    let learning_rate = 0.025f32;
    let window_size = 3usize;

    for _walk_iter in 0..num_walks {
        for start_idx in 0..num_v as u32 {
            let mut walk = Vec::with_capacity(walk_length);
            walk.push(start_idx);

            let mut curr = start_idx;
            let mut prev: Option<u32> = None;

            for _step in 1..walk_length {
                let start_off = csr.offsets[curr as usize] as usize;
                let end_off = csr.offsets[(curr + 1) as usize] as usize;
                let actual_end = end_off.min(csr.targets.len());

                if start_off >= actual_end {
                    break;
                }

                let num_neighbors = actual_end - start_off;
                let next_node = if let Some(p_node) = prev {
                    // Biased 2nd-order transition probability
                    let p_start = csr.offsets[p_node as usize] as usize;
                    let p_end = (csr.offsets[(p_node + 1) as usize] as usize).min(csr.targets.len());
                    let p_neighbors: HashSet<u64> = csr.targets[p_start..p_end].iter().copied().collect();

                    let mut weights = Vec::with_capacity(num_neighbors);
                    for off in start_off..actual_end {
                        let target = csr.targets[off];
                        let w = if target == p_node as u64 {
                            inv_p
                        } else if p_neighbors.contains(&target) {
                            1.0
                        } else {
                            inv_q
                        };
                        weights.push(w);
                    }

                    let sum_w: f64 = weights.iter().sum();
                    if sum_w <= 0.0 {
                        let pick = start_off + ((curr as usize + _step) % num_neighbors);
                        csr.targets[pick] as u32
                    } else {
                        let choice = (weights[0] / sum_w * (num_neighbors as f64)) as usize % num_neighbors;
                        csr.targets[start_off + choice] as u32
                    }
                } else {
                    let pick = start_off + ((curr as usize + _step) % num_neighbors);
                    csr.targets[pick] as u32
                };

                prev = Some(curr);
                curr = next_node;
                walk.push(curr);
            }

            // Skip-Gram update along the generated random walk
            for (i, &center) in walk.iter().enumerate() {
                let center_idx = center as usize;
                if center_idx >= num_v {
                    continue;
                }

                let start_w = i.saturating_sub(window_size);
                let end_w = (i + window_size + 1).min(walk.len());

                for &context in &walk[start_w..end_w] {
                    let context_idx = context as usize;
                    if context_idx == center_idx || context_idx >= num_v {
                        continue;
                    }

                    // Positive sample gradient: attract center and context embeddings
                    for d in 0..dimensions {
                        let diff = embeddings[context_idx][d] - embeddings[center_idx][d];
                        embeddings[center_idx][d] += learning_rate * diff;
                    }
                }
                normalize(&mut embeddings[center_idx]);
            }
        }
    }

    // Map back to original VertexIds
    let mut results = Vec::with_capacity(num_v);
    let mut reverse_map: Vec<(u32, u64)> = csr.vertex_map.iter().map(|(&k, &v)| (v, k)).collect();
    reverse_map.sort_by_key(|&(idx, _)| idx);

    for (idx, raw_vid) in reverse_map {
        let emb = embeddings[idx as usize].clone();
        results.push((VertexId(raw_vid), emb));
    }

    results
}

fn normalize(vec: &mut [f32]) {
    let norm_sq: f32 = vec.iter().map(|x| x * x).sum();
    let norm = norm_sq.sqrt();
    if norm > 1e-6 {
        for x in vec.iter_mut() {
            *x /= norm;
        }
    }
}
