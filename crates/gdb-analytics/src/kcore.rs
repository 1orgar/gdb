use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// K-Core Decomposition: computes the coreness number for every vertex in the undirected simple graph.
/// A k-core is a maximal connected subgraph in which all vertices have degree at least k.
pub fn k_core_decomposition(csr: &ChunkedCsr) -> HashMap<VertexId, u32> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return HashMap::new();
    }

    // Build undirected simple adjacency list (no self-loops, deduplicated)
    let adj = csr.build_undirected_adj();
    let mut degrees: Vec<u32> = adj.iter().map(|n| n.len() as u32).collect();

    let mut coreness = vec![0u32; num_v];
    let mut removed = vec![false; num_v];

    // Priority queue of (degree, vertex_idx)
    let mut pq = BinaryHeap::with_capacity(num_v);
    for u in 0..num_v {
        pq.push(Reverse((degrees[u], u)));
    }

    let mut current_k = 0u32;

    while let Some(Reverse((deg, u))) = pq.pop() {
        if removed[u] || deg > degrees[u] {
            continue;
        }

        current_k = current_k.max(degrees[u]);
        coreness[u] = current_k;
        removed[u] = true;

        // Symmetrically reduce degrees of remaining undirected neighbors
        for &v_u32 in &adj[u] {
            let v = v_u32 as usize;
            if !removed[v] && degrees[v] > 0 {
                degrees[v] -= 1;
                pq.push(Reverse((degrees[v], v)));
            }
        }
    }

    let mut result = HashMap::with_capacity(num_v);
    for i in 0..num_v {
        result.insert(VertexId(csr.reverse_map[i]), coreness[i]);
    }
    result
}
