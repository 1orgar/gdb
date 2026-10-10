use crate::backend::{BfsResult, CpuFallbackBackend, GpuComputeBackend, PageRankResult};
#[allow(unused_imports)]
use crate::cuda::CudaComputeBackend;
#[allow(unused_imports)]
use crate::metal::MetalComputeBackend;
use gdb_core::{GdbResult, VertexId};
use gdb_storage::ChunkedCsr;
use std::sync::Arc;

/// Adaptive Hybrid Dispatcher: routes graph analytics between CPU and GPU
/// based on work-size thresholds and hardware availability.
#[derive(Clone)]
pub struct GpuDispatcher {
    backend: Arc<dyn GpuComputeBackend>,
    /// Whether GPU hardware acceleration is enabled
    pub enabled: bool,
    /// Selected GPU device index
    pub device_id: u32,
    /// Minimum number of edges required to trigger GPU hardware dispatch
    pub threshold_edges: usize,
}

impl Default for GpuDispatcher {
    fn default() -> Self {
        Self::disabled()
    }
}

impl GpuDispatcher {
    /// Creates a disabled dispatcher running purely on CPU SIMD fallback.
    pub fn disabled() -> Self {
        Self {
            backend: Arc::new(CpuFallbackBackend),
            enabled: false,
            device_id: 0,
            threshold_edges: 10_000,
        }
    }

    /// Creates an enabled dispatcher (convenience helper).
    pub fn enabled(device_id: u32, threshold_edges: usize) -> Self {
        Self::new(true, device_id, threshold_edges)
    }

    /// Creates a new dispatcher with explicit enabled flag, device id, and offload threshold.
    pub fn new(enabled: bool, device_id: u32, threshold_edges: usize) -> Self {
        if !enabled {
            return Self {
                backend: Arc::new(CpuFallbackBackend),
                enabled: false,
                device_id,
                threshold_edges,
            };
        }

        #[cfg(target_os = "macos")]
        let backend: Arc<dyn GpuComputeBackend> = Arc::new(MetalComputeBackend::with_device(device_id));

        #[cfg(target_os = "linux")]
        let backend: Arc<dyn GpuComputeBackend> = {
            if CudaComputeBackend::is_available() {
                Arc::new(CudaComputeBackend::with_device(device_id))
            } else {
                Arc::new(CpuFallbackBackend)
            }
        };

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let backend: Arc<dyn GpuComputeBackend> = Arc::new(CpuFallbackBackend);

        Self {
            backend,
            enabled: true,
            device_id,
            threshold_edges,
        }
    }

    pub fn with_threshold(mut self, threshold: usize) -> Self {
        self.threshold_edges = threshold;
        self
    }

    pub fn with_device(mut self, device_id: u32) -> Self {
        self.device_id = device_id;
        if self.enabled {
            return Self::new(true, device_id, self.threshold_edges);
        }
        self
    }

    pub fn backend_name(&self) -> &'static str {
        if !self.enabled {
            "CPU Vectorized Engine (GPU Disabled)"
        } else {
            self.backend.name()
        }
    }

    /// Dispatches parallel Breadth-First Search to GPU or CPU.
    pub fn bfs(
        &self,
        csr: &ChunkedCsr,
        start_vid: VertexId,
        max_depth: u32,
    ) -> GdbResult<BfsResult> {
        let edges = csr.edge_count();
        if self.enabled && edges >= self.threshold_edges {
            tracing::info!(
                "Dispatching BFS to GPU accelerator ({}, device #{}) for {} edges",
                self.backend.name(),
                self.device_id,
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
        if self.enabled && edges >= self.threshold_edges {
            tracing::info!(
                "Dispatching PageRank to GPU accelerator ({}, device #{}) for {} edges",
                self.backend.name(),
                self.device_id,
                edges
            );
            self.backend.pagerank(csr, damping, iterations)
        } else {
            CpuFallbackBackend.pagerank(csr, damping, iterations)
        }
    }

    /// Dispatches Weakly Connected Components (WCC) computation to GPU or CPU.
    pub fn wcc(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u64)>> {
        let edges = csr.edge_count();
        if self.enabled && edges >= self.threshold_edges {
            tracing::info!(
                "Dispatching WCC to GPU accelerator ({}, device #{}) for {} edges",
                self.backend.name(),
                self.device_id,
                edges
            );
            self.backend.wcc(csr)
        } else {
            CpuFallbackBackend.wcc(csr)
        }
    }

    /// Dispatches Louvain Community Detection to GPU or CPU.
    pub fn louvain(&self, csr: &ChunkedCsr, max_iter: usize) -> GdbResult<Vec<(VertexId, u64)>> {
        let edges = csr.edge_count();
        if self.enabled && edges >= self.threshold_edges {
            tracing::info!(
                "Dispatching Louvain to GPU accelerator ({}, device #{}) for {} edges",
                self.backend.name(),
                self.device_id,
                edges
            );
            self.backend.louvain(csr, max_iter)
        } else {
            CpuFallbackBackend.louvain(csr, max_iter)
        }
    }

    /// Dispatches Triangle Counting to GPU or CPU.
    pub fn triangle_count(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u64)>> {
        let edges = csr.edge_count();
        if self.enabled && edges >= self.threshold_edges {
            tracing::info!(
                "Dispatching Triangle Counting to GPU accelerator ({}, device #{}) for {} edges",
                self.backend.name(),
                self.device_id,
                edges
            );
            self.backend.triangle_count(csr)
        } else {
            CpuFallbackBackend.triangle_count(csr)
        }
    }

    /// Dispatches parallel Vector Similarity Search to GPU accelerator or CPU.
    pub fn vector_similarity(
        &self,
        vectors: &[f32],
        dim: usize,
        query: &[f32],
        k: usize,
        metric: &str,
    ) -> GdbResult<Vec<(usize, f32)>> {
        self.backend.vector_similarity(vectors, dim, query, k, metric)
    }
}
