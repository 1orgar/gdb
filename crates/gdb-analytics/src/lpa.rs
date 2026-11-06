use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::HashMap;

#[inline]
fn sorted_intersection_len(a: &[u32], b: &[u32]) -> u32 {
    let mut i = 0;
    let mut j = 0;
    let mut count = 0u32;
    while i < a.len() && j < b.len() {
        if a[i] < b[j] {
            i += 1;
        } else if a[i] > b[j] {
            j += 1;
        } else {
            count += 1;
            i += 1;
            j += 1;
        }
    }
    count
}

/// Label Propagation Algorithm (LPA) for fast community detection over undirected topology.
/// Uses triangle-structural support weighting (`1 + |N(u) ∩ N(v)|`) to prevent bridge-edge
/// label leakage across communities on initial iterations.
pub fn label_propagation(csr: &ChunkedCsr, max_iter: usize) -> HashMap<VertexId, u64> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return HashMap::new();
    }

    let adj = csr.build_undirected_adj();
    let mut labels: Vec<u64> = (0..num_v).map(|i| csr.reverse_map[i]).collect();

    let mut label_counts: Vec<(u64, u32)> = Vec::with_capacity(32);

    for _ in 0..max_iter {
        let mut changed = false;

        for u in 0..num_v {
            let neighbors = &adj[u];
            if neighbors.is_empty() {
                continue;
            }

            label_counts.clear();
            for &v_u32 in neighbors {
                let v = v_u32 as usize;
                let v_label = labels[v];
                let support = 1 + sorted_intersection_len(neighbors, &adj[v]);
                if let Some(entry) = label_counts.iter_mut().find(|(lbl, _)| *lbl == v_label) {
                    entry.1 += support;
                } else {
                    label_counts.push((v_label, support));
                }
            }

            if let Some(&(best_label, _)) = label_counts
                .iter()
                .max_by(|(l1, c1), (l2, c2)| c1.cmp(c2).then_with(|| l2.cmp(l1)))
            {
                if best_label != labels[u] {
                    labels[u] = best_label;
                    changed = true;
                }
            }
        }

        if !changed {
            break;
        }
    }

    let mut result = HashMap::with_capacity(num_v);
    for i in 0..num_v {
        result.insert(VertexId(csr.reverse_map[i]), labels[i]);
    }
    result
}
