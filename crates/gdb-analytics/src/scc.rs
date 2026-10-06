use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::HashMap;

/// Computes Strongly Connected Components (SCC) using Kosaraju's algorithm.
pub fn strongly_connected_components(csr: &ChunkedCsr) -> HashMap<VertexId, VertexId> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return HashMap::new();
    }

    // 1. Build reverse adjacency lists
    let mut rev_adj: Vec<Vec<usize>> = vec![Vec::new(); num_v];
    for u_idx in 0..num_v {
        let start = csr.offsets[u_idx] as usize;
        let end = csr.offsets[u_idx + 1] as usize;
        let actual_end = end.min(csr.targets.len());

        for off in start..actual_end {
            let target_raw = csr.targets[off];
            if let Some(&v_idx) = csr.vertex_map.get(&target_raw) {
                rev_adj[v_idx as usize].push(u_idx);
            }
        }
    }

    // 2. First DFS pass to record finish order
    let mut visited = vec![false; num_v];
    let mut order = Vec::with_capacity(num_v);

    for i in 0..num_v {
        if !visited[i] {
            dfs_forward(i, csr, &mut visited, &mut order);
        }
    }

    // 3. Second DFS pass on reverse graph in reverse finish order
    visited.fill(false);
    let mut component_map = HashMap::with_capacity(num_v);

    for &u_idx in order.iter().rev() {
        if !visited[u_idx] {
            let root_vid = VertexId(csr.reverse_map[u_idx]);
            dfs_backward(u_idx, &rev_adj, &mut visited, csr, root_vid, &mut component_map);
        }
    }

    component_map
}

fn dfs_forward(
    u: usize,
    csr: &ChunkedCsr,
    visited: &mut [bool],
    order: &mut Vec<usize>,
) {
    visited[u] = true;
    let start = csr.offsets[u] as usize;
    let end = csr.offsets[u + 1] as usize;
    let actual_end = end.min(csr.targets.len());

    for off in start..actual_end {
        let target_raw = csr.targets[off];
        if let Some(&v) = csr.vertex_map.get(&target_raw) {
            let v_idx = v as usize;
            if !visited[v_idx] {
                dfs_forward(v_idx, csr, visited, order);
            }
        }
    }
    order.push(u);
}

fn dfs_backward(
    u: usize,
    rev_adj: &[Vec<usize>],
    visited: &mut [bool],
    csr: &ChunkedCsr,
    root_vid: VertexId,
    component_map: &mut HashMap<VertexId, VertexId>,
) {
    visited[u] = true;
    let vid = VertexId(csr.reverse_map[u]);
    component_map.insert(vid, root_vid);

    for &v in &rev_adj[u] {
        if !visited[v] {
            dfs_backward(v, rev_adj, visited, csr, root_vid, component_map);
        }
    }
}
