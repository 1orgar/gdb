use crate::backend::{BfsResult, CpuFallbackBackend, GpuComputeBackend, PageRankResult};
use crate::metal::MetalComputeBackend;
use gdb_core::{GdbResult, VertexId};
use gdb_storage::ChunkedCsr;
use std::sync::Arc;

/// Adaptive Hybrid Dispatcher: routes graph analytics between CPU and GPU
/// based on work-size thresholds and hardware availability.
pub struct GpuDispatcher {
    backend: Arc<dyn GpuComputeBackend>,
    /// Minimum number of edges required to trigger GPU hardware dispatch
    pub threshold_edges: usize,
}

impl Default for GpuDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuDispatcher {
    pub fn new() -> Self {
        #[cfg(target_os = "macos")]
        let backend: Arc<dyn GpuComputeBackend> = Arc::new(MetalComputeBackend::new());

        #[cfg(not(target_os = "macos"))]
        let backend: Arc<dyn GpuComputeBackend> = Arc::new(CpuFallbackBackend);

        Self {
            backend,
            threshold_edges: 10_000,
        }
    }

    pub fn with_threshold(mut self, threshold: usize) -> Self {
        self.threshold_edges = threshold;
        self
    }

    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }

    /// Dispatches parallel Breadth-First Search to GPU or CPU.
    pub fn bfs(
        &self,
        csr: &ChunkedCsr,
        start_vid: VertexId,
        max_depth: u32,
    ) -> GdbResult<BfsResult> {
        let edges = csr.edge_count();
        if edges >= self.threshold_edges {
            tracing::info!(
                "Dispatching BFS to GPU accelerator ({}) for {} edges",
                self.backend.name(),
                edges
            );
            self.backend.parallel_bfs(csr, start_vid, max_depth)
        } else {
            CpuFallbackBackend.parallel_bfs(csr, start_vid, max_depth)
        }
    }

    /// Dispatches PageRank computation to GPU or CPU.
    pub fn pagerank(
        &self,
        csr: &ChunkedCsr,
        damping: f32,
        iterations: usize,
    ) -> GdbResult<PageRankResult> {
        let edges = csr.edge_count();
        if edges >= self.threshold_edges {
            tracing::info!(
                "Dispatching PageRank to GPU accelerator ({}) for {} edges",
                self.backend.name(),
                edges
            );
            self.backend.pagerank(csr, damping, iterations)
        } else {
            CpuFallbackBackend.pagerank(csr, damping, iterations)
        }
    }
}
