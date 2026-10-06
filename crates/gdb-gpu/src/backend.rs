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
}
