use ahash::AHashSet;
use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;

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
    if num_v == 0 || dimensions == 0 || walk_length == 0 {
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

    // Reusable buffers to eliminate per-step heap allocations
    let mut candidate_denses = Vec::with_capacity(64);
    let mut weights = Vec::with_capacity(64);

    let learning_rate = 0.025f32;
    let window_size = 3usize;

    for _walk_iter in 0..num_walks {
        for start_idx in 0..num_v as u32 {
            let mut walk = Vec::with_capacity(walk_length);
            walk.push(start_idx);

            let mut curr = start_idx;
            let mut prev: Option<u32> = None;

            for _step in 1..walk_length {
                if (curr as usize) + 1 >= csr.offsets.len() {
                    break;
                }
                let start_off = csr.offsets[curr as usize] as usize;
                let end_off = csr.offsets[(curr + 1) as usize] as usize;
                let actual_end = end_off.min(csr.targets.len());

                if start_off >= actual_end {
                    break;
                }

                candidate_denses.clear();
                weights.clear();

                let next_node: Option<u32> = if let Some(p_node) = prev {
                    // Biased 2nd-order transition probability
                    let p_raw = if (p_node as usize) < csr.reverse_map.len() {
                        csr.reverse_map[p_node as usize]
                    } else {
                        u64::MAX
                    };

                    let (p_small, p_set) = if (p_node as usize) + 1 < csr.offsets.len() {
                        let p_start = csr.offsets[p_node as usize] as usize;
                        let p_end = (csr.offsets[(p_node + 1) as usize] as usize).min(csr.targets.len());
                        if p_start < p_end {
                            let slice = &csr.targets[p_start..p_end];
                            if slice.len() <= 32 {
                                (Some(slice), None)
                            } else {
                                let set: AHashSet<u64> = slice.iter().copied().collect();
                                (None, Some(set))
                            }
                        } else {
                            (Some(&[][..]), None)
                        }
                    } else {
                        (Some(&[][..]), None)
                    };

                    let is_p_neighbor = |target: u64| -> bool {
                        if let Some(slice) = p_small {
                            slice.contains(&target)
                        } else if let Some(ref set) = p_set {
                            set.contains(&target)
                        } else {
                            false
                        }
                    };

                    for off in start_off..actual_end {
                        let target_raw = csr.targets[off];
                        if let Some(&v_dense) = csr.vertex_map.get(&target_raw) {
                            candidate_denses.push(v_dense);
                            let w = if target_raw == p_raw {
                                inv_p
                            } else if is_p_neighbor(target_raw) {
                                1.0
                            } else {
                                inv_q
                            };
                            weights.push(w);
                        }
                    }

                    if candidate_denses.is_empty() {
                        None
                    } else {
                        let sum_w: f64 = weights.iter().sum();
                        let pick = if sum_w <= 0.0 {
                            (curr as usize + _step) % candidate_denses.len()
                        } else {
                            let mut step_seed = (curr as u64)
                                .wrapping_mul(6364136223846793005)
                                .wrapping_add((_step as u64).wrapping_mul(1442695040888963407))
                                .wrapping_add(1);
                            step_seed = step_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                            let r = ((step_seed >> 33) as f64 / (1u64 << 31) as f64) * sum_w;
                            let mut acc = 0.0;
                            let mut sel = 0;
                            for (i, &w) in weights.iter().enumerate() {
                                acc += w;
                                if r <= acc {
                                    sel = i;
                                    break;
                                }
                            }
                            sel.min(candidate_denses.len() - 1)
                        };
                        Some(candidate_denses[pick])
                    }
                } else {
                    // 1st-order step: pick among valid outgoing neighbors
                    for off in start_off..actual_end {
                        let target_raw = csr.targets[off];
                        if let Some(&v_dense) = csr.vertex_map.get(&target_raw) {
                            candidate_denses.push(v_dense);
                        }
                    }

                    if candidate_denses.is_empty() {
                        None
                    } else {
                        let pick = (curr as usize + _step) % candidate_denses.len();
                        Some(candidate_denses[pick])
                    }
                };

                let Some(next_dense) = next_node else {
                    break;
                };

                prev = Some(curr);
                curr = next_dense;
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

    // Map back to original VertexIds in dense order
    let mut results = Vec::with_capacity(num_v);
    for idx in 0..num_v {
        let raw_vid = csr.reverse_map[idx];
        let emb = embeddings[idx].clone();
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
