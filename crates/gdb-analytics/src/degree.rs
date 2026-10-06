use gdb_core::VertexId;
use gdb_storage::ChunkedCsr;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DegreeMetric {
    pub in_degree: u32,
    pub out_degree: u32,
    pub total_degree: u32,
}

/// Computes In-Degree, Out-Degree and Total Degree for every vertex.
pub fn degree_centrality(csr: &ChunkedCsr) -> HashMap<VertexId, DegreeMetric> {
    let num_v = csr.vertex_count();
    if num_v == 0 {
        return HashMap::new();
    }

    let mut out_degrees = vec![0u32; num_v];
    let mut in_degrees = vec![0u32; num_v];

    for u in 0..num_v {
        let start = csr.offsets[u] as usize;
        let end = csr.offsets[u + 1] as usize;
        let actual_end = end.min(csr.targets.len());

        out_degrees[u] = actual_end.saturating_sub(start) as u32;

        for off in start..actual_end {
            let target_raw = csr.targets[off];
            if let Some(&v_idx) = csr.vertex_map.get(&target_raw) {
                in_degrees[v_idx as usize] += 1;
            }
        }
    }

    let mut result = HashMap::with_capacity(num_v);
    for i in 0..num_v {
        let in_d = in_degrees[i];
        let out_d = out_degrees[i];
        result.insert(
            VertexId(csr.reverse_map[i]),
            DegreeMetric {
                in_degree: in_d,
                out_degree: out_d,
                total_degree: in_d + out_d,
            },
        );
    }
    result
}
