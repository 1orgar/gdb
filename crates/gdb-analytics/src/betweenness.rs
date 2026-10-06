use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::{HashMap, VecDeque};

/// Brandes' algorithm for Betweenness Centrality on unweighted graphs.
pub fn betweenness_centrality(csr: &ChunkedCsr, normalized: bool) -> HashMap<VertexId, f64> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return HashMap::new();
    }

    let mut cb = vec![0.0f64; num_v];

    for s in 0..num_v {
        let mut stack = Vec::with_capacity(num_v);
        let mut p: Vec<Vec<usize>> = vec![Vec::new(); num_v];
        let mut sigma = vec![0.0f64; num_v];
        sigma[s] = 1.0;
        let mut d = vec![-1i32; num_v];
        d[s] = 0;

        let mut queue = VecDeque::new();
        queue.push_back(s);

        while let Some(v) = queue.pop_front() {
            stack.push(v);

            let start = csr.offsets[v] as usize;
            let end = csr.offsets[v + 1] as usize;
            let actual_end = end.min(csr.targets.len());

            for off in start..actual_end {
                let target_raw = csr.targets[off];
                if let Some(&w_idx) = csr.vertex_map.get(&target_raw) {
                    let w = w_idx as usize;

                    // w found for the first time?
                    if d[w] < 0 {
                        d[w] = d[v] + 1;
                        queue.push_back(w);
                    }

                    // shortest path to w via v?
                    if d[w] == d[v] + 1 {
                        sigma[w] += sigma[v];
                        p[w].push(v);
                    }
                }
            }
        }

        let mut delta = vec![0.0f64; num_v];
        // S returns vertices in order of non-increasing distance from s
        while let Some(w) = stack.pop() {
            for &v in &p[w] {
                delta[v] += (sigma[v] / sigma[w]) * (1.0 + delta[w]);
            }
            if w != s {
                cb[w] += delta[w];
            }
        }
    }

    // Normalization factor for directed graphs: 1 / ((n-1)(n-2))
    if normalized && num_v > 2 {
        let factor = 1.0 / (((num_v - 1) * (num_v - 2)) as f64);
        for val in cb.iter_mut() {
            *val *= factor;
        }
    }

    let mut result = HashMap::with_capacity(num_v);
    for i in 0..num_v {
        result.insert(VertexId(csr.reverse_map[i]), cb[i]);
    }
    result
}
