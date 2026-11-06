use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::HashMap;

/// Louvain Community Detection algorithm (Modularity optimization over undirected topology).
pub fn louvain(csr: &ChunkedCsr, max_iter: usize) -> HashMap<VertexId, u64> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return HashMap::new();
    }

    let adj = csr.build_undirected_adj();
    let mut degrees = vec![0.0f64; num_v];
    let mut m2 = 0.0f64; // Sum of all undirected degrees = 2 * |E_undir|
    for u in 0..num_v {
        let d = adj[u].len() as f64;
        degrees[u] = d;
        m2 += d;
    }

    if m2 == 0.0 {
        let mut res = HashMap::with_capacity(num_v);
        for (i, &vid_raw) in csr.reverse_map.iter().enumerate() {
            res.insert(VertexId(vid_raw), i as u64);
        }
        return res;
    }

    // Community assignment: community[u] = c
    let mut community: Vec<usize> = (0..num_v).collect();
    // Total degree of all nodes in community c: tot[c]
    let mut tot = degrees.clone();

    let mut comm_weights: Vec<(usize, f64)> = Vec::with_capacity(32);

    for _ in 0..max_iter {
        let mut improved = false;

        for u in 0..num_v {
            let k_u = degrees[u];
            if k_u == 0.0 {
                continue;
            }
            let current_c = community[u];

            comm_weights.clear();
            let mut k_u_in_curr = 0.0f64;
            for &v_u32 in &adj[u] {
                let c = community[v_u32 as usize];
                if c == current_c {
                    k_u_in_curr += 1.0;
                }
                if let Some(entry) = comm_weights.iter_mut().find(|(cid, _)| *cid == c) {
                    entry.1 += 1.0;
                } else {
                    comm_weights.push((c, 1.0));
                }
            }

            // Remove u from its current community
            tot[current_c] -= k_u;

            // Find community with best modularity gain Delta Q
            let mut best_c = current_c;
            let mut best_gain = k_u_in_curr - (tot[current_c] * k_u) / m2;

            for &(target_c, k_u_in_target) in &comm_weights {
                if target_c == current_c {
                    continue;
                }
                let gain = k_u_in_target - (tot[target_c] * k_u) / m2;
                if gain > best_gain + 1e-12
                    || ((gain - best_gain).abs() <= 1e-12 && target_c < best_c)
                {
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
