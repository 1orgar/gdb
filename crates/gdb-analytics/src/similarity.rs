use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::HashSet;

/// Calculates Jaccard Similarity between two vertices based on their outgoing neighbors.
/// J(A, B) = |N(A) ∩ N(B)| / |N(A) ∪ N(B)|
pub fn jaccard_similarity(csr: &ChunkedCsr, node1: VertexId, node2: VertexId) -> f64 {
    let n1 = get_neighbors_set(csr, node1);
    let n2 = get_neighbors_set(csr, node2);

    if n1.is_empty() && n2.is_empty() {
        return 1.0;
    }

    let intersection = n1.intersection(&n2).count();
    let union = n1.union(&n2).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Calculates Cosine Similarity between the adjacency vectors of two vertices.
/// Cos(A, B) = |N(A) ∩ N(B)| / (sqrt(|N(A)|) * sqrt(|N(B)|))
pub fn cosine_similarity(csr: &ChunkedCsr, node1: VertexId, node2: VertexId) -> f64 {
    let n1 = get_neighbors_set(csr, node1);
    let n2 = get_neighbors_set(csr, node2);

    if n1.is_empty() || n2.is_empty() {
        return 0.0;
    }

    let intersection = n1.intersection(&n2).count();
    let denom = ((n1.len() as f64).sqrt()) * ((n2.len() as f64).sqrt());

    if denom == 0.0 {
        0.0
    } else {
        intersection as f64 / denom
    }
}

/// Counts the number of common neighbors between two vertices: |N(A) ∩ N(B)|.
pub fn common_neighbors(csr: &ChunkedCsr, node1: VertexId, node2: VertexId) -> usize {
    let n1 = get_neighbors_set(csr, node1);
    let n2 = get_neighbors_set(csr, node2);
    n1.intersection(&n2).count()
}

fn get_neighbors_set(csr: &ChunkedCsr, node: VertexId) -> HashSet<u64> {
    let mut neighbors = HashSet::new();
    if let Some(&u_idx) = csr.vertex_map.get(&node.as_u64()) {
        let start = csr.offsets[u_idx as usize] as usize;
        let end = csr.offsets[(u_idx + 1) as usize] as usize;
        let actual_end = end.min(csr.targets.len());

        for off in start..actual_end {
            neighbors.insert(csr.targets[off]);
        }
    }
    neighbors
}
