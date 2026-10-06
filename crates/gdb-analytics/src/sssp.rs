use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::{HashMap, VecDeque};

/// Single-Source Shortest Path (SSSP) on unweighted graph using BFS.
/// Returns map: TargetVertexId -> Distance (u32::MAX if unreachable).
pub fn single_source_shortest_path(
    csr: &ChunkedCsr,
    source: VertexId,
) -> HashMap<VertexId, u32> {
    let num_v = csr.vertex_count();
    let mut distances = HashMap::with_capacity(num_v);

    for &vid_raw in &csr.reverse_map {
        distances.insert(VertexId(vid_raw), u32::MAX);
    }

    let Some(&start_idx) = csr.vertex_map.get(&source.as_u64()) else {
        return distances;
    };

    let mut dist_vec = vec![u32::MAX; num_v];
    dist_vec[start_idx as usize] = 0;

    let mut queue = VecDeque::new();
    queue.push_back(start_idx as usize);

    while let Some(u) = queue.pop_front() {
        let current_d = dist_vec[u];
        let start = csr.offsets[u] as usize;
        let end = csr.offsets[u + 1] as usize;
        let actual_end = end.min(csr.targets.len());

        for off in start..actual_end {
            let target_raw = csr.targets[off];
            if let Some(&v_idx) = csr.vertex_map.get(&target_raw) {
                let v = v_idx as usize;
                if dist_vec[v] == u32::MAX {
                    dist_vec[v] = current_d + 1;
                    queue.push_back(v);
                }
            }
        }
    }

    for (idx, &d) in dist_vec.iter().enumerate() {
        distances.insert(VertexId(csr.reverse_map[idx]), d);
    }

    distances
}
