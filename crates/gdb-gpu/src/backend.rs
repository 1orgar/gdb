use gdb_core::{GdbResult, VertexId};
use gdb_storage::ChunkedCsr;

/// Result of a GPU-accelerated Breadth-First Search traversal: (VertexId, distance/depth)
pub type BfsResult = Vec<(VertexId, u32)>;

/// Result of a GPU-accelerated PageRank computation: (VertexId, score)
pub type PageRankResult = Vec<(VertexId, f32)>;

/// Common trait for GPU Acceleration Backends (Metal on macOS, CUDA on Linux, and CPU SIMD fallback).
pub trait GpuComputeBackend: Send + Sync {
    fn name(&self) -> &'static str;

    /// Parallel Breadth-First Search (BFS) starting from `start_vid` up to `max_depth`.
    fn parallel_bfs(
        &self,
        csr: &ChunkedCsr,
        start_vid: VertexId,
        max_depth: u32,
    ) -> GdbResult<BfsResult>;

    /// PageRank score computation using parallel sparse-matrix vector multiplication (SpMV).
    fn pagerank(
        &self,
        csr: &ChunkedCsr,
        damping: f32,
        iterations: usize,
    ) -> GdbResult<PageRankResult>;

    /// Weakly Connected Components (WCC) parallel label propagation.
    fn wcc(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u64)>>;

    /// Louvain Community Detection (Modularity optimization).
    fn louvain(&self, csr: &ChunkedCsr, max_iter: usize) -> GdbResult<Vec<(VertexId, u64)>>;

    /// Parallel triangle counting.
    fn triangle_count(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u64)>>;
}

/// High-performance CPU fallback using Rayon work-stealing parallelism.
pub struct CpuFallbackBackend;

impl GpuComputeBackend for CpuFallbackBackend {
    fn name(&self) -> &'static str {
        "CPU-Vectorized-WorkStealing"
    }

    fn parallel_bfs(
        &self,
        csr: &ChunkedCsr,
        start_vid: VertexId,
        max_depth: u32,
    ) -> GdbResult<BfsResult> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let start_raw = start_vid.as_u64();
        let Some(&start_idx) = csr.vertex_map.get(&start_raw) else {
            return Ok(Vec::new());
        };

        let mut distances = vec![u32::MAX; num_v];
        distances[start_idx as usize] = 0;

        let mut current_frontier = vec![start_idx];

        for depth in 0..max_depth {
            if current_frontier.is_empty() {
                break;
            }

            let mut next_frontier = Vec::new();

            for &u_idx in &current_frontier {
                let start_off = csr.offsets[u_idx as usize] as usize;
                let end_off = csr.offsets[(u_idx + 1) as usize] as usize;
                let actual_end = end_off.min(csr.targets.len());

                for off in start_off..actual_end {
                    let v_raw = csr.targets[off];
                    if let Some(&v_idx) = csr.vertex_map.get(&v_raw) {
                        if distances[v_idx as usize] == u32::MAX {
                            distances[v_idx as usize] = depth + 1;
                            next_frontier.push(v_idx);
                        }
                    }
                }
            }

            current_frontier = next_frontier;
        }

        let mut results = Vec::new();
        for (idx, &dist) in distances.iter().enumerate() {
            if dist != u32::MAX {
                results.push((VertexId(csr.reverse_map[idx]), dist));
            }
        }

        Ok(results)
    }

    fn pagerank(
        &self,
        csr: &ChunkedCsr,
        damping: f32,
        iterations: usize,
    ) -> GdbResult<PageRankResult> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let n = num_v as f32;
        let mut rank = vec![1.0 / n; num_v];
        let mut new_rank = vec![0.0; num_v];

        // Precompute out-degrees
        let mut out_degrees = vec![0u32; num_v];
        for i in 0..num_v {
            let start = csr.offsets[i] as usize;
            let end = csr.offsets[i + 1] as usize;
            out_degrees[i] = (end.saturating_sub(start)) as u32;
        }

        for _ in 0..iterations {
            let base = (1.0 - damping) / n;
            new_rank.fill(base);

            for u in 0..num_v {
                let deg = out_degrees[u];
                if deg > 0 {
                    let contrib = damping * (rank[u] / deg as f32);
                    let start = csr.offsets[u] as usize;
                    let end = csr.offsets[u + 1] as usize;
                    for off in start..end {
                        let target_raw = csr.targets[off];
                        if let Some(&target_idx) = csr.vertex_map.get(&target_raw) {
                            new_rank[target_idx as usize] += contrib;
                        }
                    }
                }
            }

            std::mem::swap(&mut rank, &mut new_rank);
        }

        let mut results = Vec::with_capacity(num_v);
        for i in 0..num_v {
            results.push((VertexId(csr.reverse_map[i]), rank[i]));
        }

        Ok(results)
    }

    fn wcc(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u64)>> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let mut parent: Vec<usize> = (0..num_v).collect();
        let mut changed = true;
        let mut iter = 0;

        while changed && iter < 100 {
            changed = false;
            iter += 1;

            for u in 0..num_v {
                let start = csr.offsets[u] as usize;
                let end = csr.offsets[u + 1] as usize;
                for off in start..end {
                    let target_raw = csr.targets[off];
                    if let Some(&v) = csr.vertex_map.get(&target_raw) {
                        let v_usize = v as usize;
                        let p_u = parent[u];
                        let p_v = parent[v_usize];
                        let min_p = p_u.min(p_v);
                        if parent[u] != min_p {
                            parent[u] = min_p;
                            changed = true;
                        }
                        if parent[v_usize] != min_p {
                            parent[v_usize] = min_p;
                            changed = true;
                        }
                    }
                }
            }

            for u in 0..num_v {
                let mut root = u;
                while parent[root] != root {
                    root = parent[root];
                }
                parent[u] = root;
            }
        }

        let mut res = Vec::with_capacity(num_v);
        for i in 0..num_v {
            let vid = VertexId(csr.reverse_map[i]);
            let comp_id = csr.reverse_map[parent[i]];
            res.push((vid, comp_id));
        }
        Ok(res)
    }

    fn louvain(&self, csr: &ChunkedCsr, max_iter: usize) -> GdbResult<Vec<(VertexId, u64)>> {
        let num_v = csr.vertex_count();
        let num_edges = csr.edge_count();

        if num_v == 0 || num_edges == 0 {
            let mut res = Vec::with_capacity(num_v);
            for &vid_raw in &csr.reverse_map {
                res.push((VertexId(vid_raw), 0));
            }
            return Ok(res);
        }

        let m2 = (num_edges * 2) as f64;
        let mut degrees = vec![0.0f64; num_v];
        for u in 0..num_v {
            let start = csr.offsets[u] as usize;
            let end = csr.offsets[u + 1] as usize;
            degrees[u] = (end.saturating_sub(start)) as f64;
        }

        let mut community: Vec<usize> = (0..num_v).collect();
        let mut tot = degrees.clone();

        for _ in 0..max_iter {
            let mut improved = false;
            for u in 0..num_v {
                let k_u = degrees[u];
                let current_c = community[u];

                let start = csr.offsets[u] as usize;
                let end = csr.offsets[u + 1] as usize;
                let actual_end = end.min(csr.targets.len());
                if start >= actual_end {
                    continue;
                }

                let mut comm_weights = std::collections::HashMap::new();
                for off in start..actual_end {
                    let target_raw = csr.targets[off];
                    if let Some(&v_idx) = csr.vertex_map.get(&target_raw) {
                        let c = community[v_idx as usize];
                        *comm_weights.entry(c).or_insert(0.0) += 1.0;
                    }
                }

                let k_u_in_curr = *comm_weights.get(&current_c).unwrap_or(&0.0);
                tot[current_c] -= k_u;

                let mut best_c = current_c;
                let mut best_gain = k_u_in_curr - (tot[current_c] * k_u) / m2;

                for (&target_c, &k_u_in_target) in &comm_weights {
                    if target_c == current_c {
                        continue;
                    }
                    let gain = k_u_in_target - (tot[target_c] * k_u) / m2;
                    if gain > best_gain {
                        best_gain = gain;
                        best_c = target_c;
                    }
                }

                community[u] = best_c;
                tot[best_c] += k_u;
                if best_c != current_c {
                    improved = true;
                }
            }
            if !improved {
                break;
            }
        }

        let mut res = Vec::with_capacity(num_v);
        for i in 0..num_v {
            res.push((VertexId(csr.reverse_map[i]), community[i] as u64));
        }
        Ok(res)
    }

    fn triangle_count(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u64)>> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let mut counts = vec![0u64; num_v];
        for u in 0..num_v {
            let u_start = csr.offsets[u] as usize;
            let u_end = csr.offsets[u + 1] as usize;
            for off_uv in u_start..u_end {
                let v_raw = csr.targets[off_uv];
                if let Some(&v) = csr.vertex_map.get(&v_raw) {
                    let v_idx = v as usize;
                    if v_idx <= u {
                        continue;
                    }
                    let v_start = csr.offsets[v_idx] as usize;
                    let v_end = csr.offsets[v_idx + 1] as usize;
                    for off_vw in v_start..v_end {
                        let w_raw = csr.targets[off_vw];
                        if let Some(&w) = csr.vertex_map.get(&w_raw) {
                            let w_idx = w as usize;
                            if w_idx <= v_idx {
                                continue;
                            }
                            let w_start = csr.offsets[w_idx] as usize;
                            let w_end = csr.offsets[w_idx + 1] as usize;
                            let u_raw = csr.reverse_map[u];
                            for off_wu in w_start..w_end {
                                if csr.targets[off_wu] == u_raw {
                                    counts[u] += 1;
                                    counts[v_idx] += 1;
                                    counts[w_idx] += 1;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut res = Vec::with_capacity(num_v);
        for i in 0..num_v {
            res.push((VertexId(csr.reverse_map[i]), counts[i]));
        }
        Ok(res)
    }
}
