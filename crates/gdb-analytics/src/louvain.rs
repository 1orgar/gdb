use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::HashMap;

/// Louvain Community Detection algorithm (Modularity optimization).
pub fn louvain(csr: &ChunkedCsr, max_iter: usize) -> HashMap<VertexId, u64> {
    let num_v = csr.vertex_count();
    let num_edges = csr.edge_count();

    if num_v == 0 || num_edges == 0 {
        let mut res = HashMap::new();
        for &vid_raw in &csr.reverse_map {
            res.insert(VertexId(vid_raw), 0);
        }
        return res;
    }

    let m2 = (num_edges * 2) as f64; // Sum of all degrees

    // Degrees of each vertex
    let mut degrees = vec![0.0f64; num_v];
    for u in 0..num_v {
        let start = csr.offsets[u] as usize;
        let end = csr.offsets[u + 1] as usize;
        degrees[u] = (end.saturating_sub(start)) as f64;
    }

    // Community assignment: community[u] = c
    let mut community: Vec<usize> = (0..num_v).collect();

    // Total degree of all nodes in community c: tot[c]
    let mut tot = degrees.clone();

    for _ in 0..max_iter {
        let mut improved = false;

        for u in 0..num_v {
            let k_u = degrees[u];
            let current_c = community[u];

            // Calculate connections from u to neighboring communities
            let start = csr.offsets[u] as usize;
            let end = csr.offsets[u + 1] as usize;
            let actual_end = end.min(csr.targets.len());

            if start >= actual_end {
                continue;
            }

            let mut comm_weights: HashMap<usize, f64> = HashMap::new();
            for off in start..actual_end {
                let target_raw = csr.targets[off];
                if let Some(&v_idx) = csr.vertex_map.get(&target_raw) {
                    let c = community[v_idx as usize];
                    *comm_weights.entry(c).or_insert(0.0) += 1.0;
                }
            }

            // Remove u from its current community
            let k_u_in_curr = *comm_weights.get(&current_c).unwrap_or(&0.0);
            tot[current_c] -= k_u;

            // Find community with best modularity gain Delta Q
            // Delta Q = k_{u, in} / 2m - (tot * k_u) / (2m^2)
            let mut best_c = current_c;
            let mut best_gain = k_u_in_curr - (tot[current_c] * k_u) / m2;

            for (&target_c, &k_u_in_target) in &comm_weights {
                if target_c == current_c {
                    continue;
                }
                let gain = k_u_in_target - (tot[target_c] * k_u) / m2;
                if gain > best_gain {
                    best_gain = gain;
                    best_c = target_c;
                }
            }

            // Assign u to best community
            community[u] = best_c;
            tot[best_c] += k_u;

            if best_c != current_c {
                improved = true;
            }
        }

        if !improved {
            break;
        }
    }

    let mut result = HashMap::with_capacity(num_v);
    for i in 0..num_v {
        result.insert(VertexId(csr.reverse_map[i]), community[i] as u64);
    }
    result
}
