use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::{BinaryHeap, HashMap};
use std::cmp::Reverse;

/// K-Core Decomposition: computes the coreness number for every vertex in the graph.
/// A k-core is a maximal connected subgraph in which all vertices have degree at least k.
pub fn k_core_decomposition(csr: &ChunkedCsr) -> HashMap<VertexId, u32> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return HashMap::new();
    }

    // 1. Compute initial degrees
    let mut degrees = vec![0u32; num_v];
    for u in 0..num_v {
        let start = csr.offsets[u] as usize;
        let end = csr.offsets[u + 1] as usize;
        degrees[u] = end.saturating_sub(start) as u32;
    }

    let mut coreness = vec![0u32; num_v];
    let mut removed = vec![false; num_v];

    // Priority queue of (degree, vertex_idx)
    let mut pq = BinaryHeap::with_capacity(num_v);
    for u in 0..num_v {
        pq.push(Reverse((degrees[u], u)));
    }

    let mut current_k = 0u32;

    while let Some(Reverse((deg, u))) = pq.pop() {
        if removed[u] {
            continue;
        }

        // Lazy update check
        if deg > degrees[u] {
            continue;
        }

        current_k = current_k.max(degrees[u]);
        coreness[u] = current_k;
        removed[u] = true;

        // Reduce degrees of neighbors
        let start = csr.offsets[u] as usize;
        let end = csr.offsets[u + 1] as usize;
        let actual_end = end.min(csr.targets.len());

        for off in start..actual_end {
            let target_raw = csr.targets[off];
            if let Some(&v_idx) = csr.vertex_map.get(&target_raw) {
                let v = v_idx as usize;
                if !removed[v] && degrees[v] > 0 {
                    degrees[v] -= 1;
                    pq.push(Reverse((degrees[v], v)));
                }
            }
        }
    }

    let mut result = HashMap::with_capacity(num_v);
    for i in 0..num_v {
        result.insert(VertexId(csr.reverse_map[i]), coreness[i]);
    }
    result
}
