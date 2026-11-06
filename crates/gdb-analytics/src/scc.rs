use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::HashMap;

/// Computes Strongly Connected Components (SCC) returning sorted (vertex_id, component_id) pairs
/// without HashMap or per-vertex Vec heap allocations, using flat CSR transpose.
pub fn strongly_connected_components_vec(csr: &ChunkedCsr) -> Vec<(u64, u64)> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return Vec::new();
    }

    // 1. Flat CSR transpose (2 contiguous Vec<u32> allocations instead of num_v Vecs)
    let num_e = csr.targets.len();
    let mut rev_offsets = vec![0u32; num_v + 1];
    for u_idx in 0..num_v {
        let start = csr.offsets[u_idx] as usize;
        let end = (csr.offsets[u_idx + 1] as usize).min(num_e);
        for off in start..end {
            if let Some(v_idx) = csr.dense_target_at(off) {
                rev_offsets[v_idx + 1] += 1;
            }
        }
    }
    for i in 0..num_v {
        rev_offsets[i + 1] += rev_offsets[i];
    }
    let mut rev_sources = vec![0u32; rev_offsets[num_v] as usize];
    let mut cursor = rev_offsets.clone();
    for u_idx in 0..num_v {
        let start = csr.offsets[u_idx] as usize;
        let end = (csr.offsets[u_idx + 1] as usize).min(num_e);
        for off in start..end {
            if let Some(v_idx) = csr.dense_target_at(off) {
                let pos = cursor[v_idx] as usize;
                cursor[v_idx] += 1;
                rev_sources[pos] = u_idx as u32;
            }
        }
    }

    // 2. Iterative first DFS pass to record finish order
    let mut visited = vec![false; num_v];
    let mut order = Vec::with_capacity(num_v);
    let mut stack: Vec<(usize, usize)> = Vec::with_capacity(1024);

    for i in 0..num_v {
        if !visited[i] {
            visited[i] = true;
            let start = csr.offsets[i] as usize;
            stack.push((i, start));

            while let Some((u, next_off)) = stack.last_mut() {
                let u_val = *u;
                let end = (csr.offsets[u_val + 1] as usize).min(num_e);
                if *next_off < end {
                    let off = *next_off;
                    *next_off += 1;
                    if let Some(v_idx) = csr.dense_target_at(off) {
                        if !visited[v_idx] {
                            visited[v_idx] = true;
                            let v_start = csr.offsets[v_idx] as usize;
                            stack.push((v_idx, v_start));
                        }
                    }
                } else {
                    order.push(u_val);
                    stack.pop();
                }
            }
        }
    }

    // 3. Iterative second DFS pass on reverse graph in reverse finish order
    visited.fill(false);
    let mut comp_ids = vec![0u64; num_v];
    let mut back_stack = Vec::with_capacity(1024);

    for &u_idx in order.iter().rev() {
        if !visited[u_idx] {
            let root_vid = csr.reverse_map[u_idx];
            visited[u_idx] = true;
            back_stack.push(u_idx);

            while let Some(curr) = back_stack.pop() {
                comp_ids[curr] = root_vid;
                let r_start = rev_offsets[curr] as usize;
                let r_end = rev_offsets[curr + 1] as usize;
                for r_off in r_start..r_end {
                    let prev = rev_sources[r_off] as usize;
                    if !visited[prev] {
                        visited[prev] = true;
                        back_stack.push(prev);
                    }
                }
            }
        }
    }

    let mut out = Vec::with_capacity(num_v);
    for i in 0..num_v {
        out.push((csr.reverse_map[i], comp_ids[i]));
    }
    out
}

/// Computes Strongly Connected Components (SCC) using iterative Kosaraju's algorithm
/// (eliminating call-stack overflow on deep graphs) and O(1) dense CSR indexing.
pub fn strongly_connected_components(csr: &ChunkedCsr) -> HashMap<VertexId, VertexId> {
    strongly_connected_components_vec(csr)
        .into_iter()
        .map(|(v, c)| (VertexId(v), VertexId(c)))
        .collect()
}
