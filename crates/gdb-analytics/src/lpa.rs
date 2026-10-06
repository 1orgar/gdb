use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::HashMap;

/// Label Propagation Algorithm (LPA) for fast community detection.
pub fn label_propagation(csr: &ChunkedCsr, max_iter: usize) -> HashMap<VertexId, u64> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return HashMap::new();
    }

    // Initialize: each vertex has its own label (its dense index as u64)
    let mut labels: Vec<u64> = (0..num_v as u64).collect();
    let mut new_labels = labels.clone();

    for _ in 0..max_iter {
        let mut changed = false;

        for u in 0..num_v {
            let start = csr.offsets[u] as usize;
            let end = csr.offsets[u + 1] as usize;
            let actual_end = end.min(csr.targets.len());

            if start >= actual_end {
                continue;
            }

            // Count frequency of labels among outgoing neighbors
            let mut label_counts: HashMap<u64, u32> = HashMap::new();
            for off in start..actual_end {
                let target_raw = csr.targets[off];
                if let Some(&v_idx) = csr.vertex_map.get(&target_raw) {
                    let v_label = labels[v_idx as usize];
                    *label_counts.entry(v_label).or_insert(0) += 1;
                }
            }

            // Pick the most frequent label (with smallest label as deterministic tie-breaker)
            if let Some((&best_label, _)) = label_counts
                .iter()
                .max_by(|(l1, c1), (l2, c2)| c1.cmp(c2).then_with(|| l2.cmp(l1)))
            {
                if best_label != labels[u] {
                    new_labels[u] = best_label;
                    changed = true;
                }
            }
        }

        labels.copy_from_slice(&new_labels);
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
