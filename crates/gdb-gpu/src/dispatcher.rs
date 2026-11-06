use crate::backend::{BfsResult, CpuFallbackBackend, GpuComputeBackend, PageRankResult};
#[allow(unused_imports)]
use crate::cuda::CudaComputeBackend;
#[allow(unused_imports)]
use crate::metal::MetalComputeBackend;
use gdb_core::{GdbResult, VertexId};
use gdb_storage::ChunkedCsr;
use std::sync::atomic::{AtomicU64, Ordering};
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
    /// Maximum VRAM allocation (in bytes) before switching to windowed CSR streaming (discrete GPU paging)
    pub max_vram_bytes: usize,
    /// Total hardware GPU kernel dispatches executed
    pub dispatch_count: Arc<AtomicU64>,
}

impl Default for GpuDispatcher {
    fn default() -> Self {
        Self::disabled()
    }
}

impl GpuDispatcher {
    /// Default VRAM limit for discrete GPUs (2 GB)
    pub const DEFAULT_MAX_VRAM_BYTES: usize = 2 * 1024 * 1024 * 1024;

    /// Creates a disabled dispatcher running purely on CPU SIMD fallback.
    pub fn disabled() -> Self {
        Self {
            backend: Arc::new(CpuFallbackBackend),
            enabled: false,
            device_id: 0,
            threshold_edges: 10_000,
            max_vram_bytes: Self::DEFAULT_MAX_VRAM_BYTES,
            dispatch_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Creates an enabled dispatcher (convenience helper).
    pub fn enabled(device_id: u32, threshold_edges: usize) -> Self {
        Self::new(true, device_id, threshold_edges)
    }

    /// Creates a new dispatcher with explicit enabled flag, device id, and offload threshold.
    pub fn new(enabled: bool, device_id: u32, threshold_edges: usize) -> Self {
        Self::new_with_vram(
            enabled,
            device_id,
            threshold_edges,
            Self::DEFAULT_MAX_VRAM_BYTES,
        )
    }

    /// Creates a new dispatcher with explicit enabled flag, device id, offload threshold, and max VRAM bytes.
    pub fn new_with_vram(
        enabled: bool,
        device_id: u32,
        threshold_edges: usize,
        max_vram_bytes: usize,
    ) -> Self {
        if !enabled {
            return Self {
                backend: Arc::new(CpuFallbackBackend),
                enabled: false,
                device_id,
                threshold_edges,
                max_vram_bytes,
                dispatch_count: Arc::new(AtomicU64::new(0)),
            };
        }

        #[cfg(target_os = "macos")]
        let backend: Arc<dyn GpuComputeBackend> =
            Arc::new(MetalComputeBackend::with_device(device_id));

        #[cfg(target_os = "linux")]
        let backend: Arc<dyn GpuComputeBackend> = {
            if CudaComputeBackend::is_available() {
                Arc::new(CudaComputeBackend::with_device_and_vram(
                    device_id,
                    max_vram_bytes,
                ))
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
            max_vram_bytes,
            dispatch_count: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn with_threshold(mut self, threshold: usize) -> Self {
        self.threshold_edges = threshold;
        self
    }

    pub fn with_max_vram(mut self, max_vram_bytes: usize) -> Self {
        self.max_vram_bytes = max_vram_bytes;
        if self.enabled {
            return Self::new_with_vram(
                true,
                self.device_id,
                self.threshold_edges,
                max_vram_bytes,
            );
        }
        self
    }

    pub fn with_device(mut self, device_id: u32) -> Self {
        self.device_id = device_id;
        if self.enabled {
            return Self::new_with_vram(
                true,
                device_id,
                self.threshold_edges,
                self.max_vram_bytes,
            );
        }
        self
    }

    pub fn max_vram_bytes(&self) -> usize {
        self.max_vram_bytes
    }

    pub fn total_dispatches(&self) -> u64 {
        self.dispatch_count.load(Ordering::Relaxed)
    }

    pub fn backend_name(&self) -> &'static str {
        if !self.enabled {
            "CPU Vectorized Engine (GPU Disabled)"
        } else {
            self.backend.name()
        }
    }

    #[inline]
    fn should_use_gpu(&self, csr: &ChunkedCsr, is_analytics_algo: bool) -> bool {
        if !self.enabled {
            return false;
        }
        let edges = csr.edge_count();
        if is_analytics_algo {
            edges > 0 || csr.vertex_count() > 0
        } else {
            edges >= self.threshold_edges
        }
    }

    /// Dispatches parallel Breadth-First Search to GPU or CPU.
    pub fn bfs(
        &self,
        csr: &ChunkedCsr,
        start_vid: VertexId,
        max_depth: u32,
    ) -> GdbResult<BfsResult> {
        if self.should_use_gpu(csr, false) {
            self.dispatch_count.fetch_add(1, Ordering::Relaxed);
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
        if self.should_use_gpu(csr, true) {
            self.dispatch_count.fetch_add(1, Ordering::Relaxed);
            self.backend.pagerank(csr, damping, iterations)
        } else {
            CpuFallbackBackend.pagerank(csr, damping, iterations)
        }
    }

    /// Dispatches high-precision f64 PageRank computation with convergence tolerance.
    pub fn pagerank_f64(
        &self,
        csr: &ChunkedCsr,
        damping: f64,
        max_iter: usize,
        tolerance: f64,
    ) -> GdbResult<Vec<(VertexId, f64)>> {
        if self.should_use_gpu(csr, true) {
            self.dispatch_count.fetch_add(1, Ordering::Relaxed);
            self.backend.pagerank_f64(csr, damping, max_iter, tolerance)
        } else {
            CpuFallbackBackend.pagerank_f64(csr, damping, max_iter, tolerance)
        }
    }

    /// Dispatches Weakly Connected Components (WCC) computation to GPU or CPU.
    pub fn wcc(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u64)>> {
        if self.should_use_gpu(csr, true) {
            self.dispatch_count.fetch_add(1, Ordering::Relaxed);
            self.backend.wcc(csr)
        } else {
            CpuFallbackBackend.wcc(csr)
        }
    }

    /// Dispatches Louvain Community Detection to GPU or CPU.
    pub fn louvain(&self, csr: &ChunkedCsr, max_iter: usize) -> GdbResult<Vec<(VertexId, u64)>> {
        if self.should_use_gpu(csr, true) {
            self.dispatch_count.fetch_add(1, Ordering::Relaxed);
            self.backend.louvain(csr, max_iter)
        } else {
            CpuFallbackBackend.louvain(csr, max_iter)
        }
    }

    /// Dispatches Triangle Counting to GPU or CPU.
    pub fn triangle_count(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u64)>> {
        if self.should_use_gpu(csr, true) {
            self.dispatch_count.fetch_add(1, Ordering::Relaxed);
            self.backend.triangle_count(csr)
        } else {
            CpuFallbackBackend.triangle_count(csr)
        }
    }

    /// Dispatches Triangle Counting + Local Clustering Coefficient to GPU or CPU.
    pub fn triangle_count_with_lcc(
        &self,
        csr: &ChunkedCsr,
    ) -> GdbResult<Vec<(VertexId, u64, f64)>> {
        if self.should_use_gpu(csr, true) {
            self.dispatch_count.fetch_add(1, Ordering::Relaxed);
            self.backend.triangle_count_with_lcc(csr)
        } else {
            CpuFallbackBackend.triangle_count_with_lcc(csr)
        }
    }

    /// Dispatches Single-Source Shortest Path (SSSP) to GPU or CPU.
    pub fn sssp(&self, csr: &ChunkedCsr, source: VertexId) -> GdbResult<Vec<(VertexId, u32)>> {
        if self.should_use_gpu(csr, true) {
            self.dispatch_count.fetch_add(1, Ordering::Relaxed);
            self.backend.sssp(csr, source)
        } else {
            CpuFallbackBackend.sssp(csr, source)
        }
    }

    /// Dispatches Label Propagation Algorithm (LPA) to GPU or CPU.
    pub fn lpa(&self, csr: &ChunkedCsr, max_iter: usize) -> GdbResult<Vec<(VertexId, u64)>> {
        if self.should_use_gpu(csr, true) {
            self.dispatch_count.fetch_add(1, Ordering::Relaxed);
            self.backend.lpa(csr, max_iter)
        } else {
            CpuFallbackBackend.lpa(csr, max_iter)
        }
    }

    /// Dispatches K-Core Decomposition to GPU or CPU.
    pub fn kcore(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u32)>> {
        if self.should_use_gpu(csr, true) {
            self.dispatch_count.fetch_add(1, Ordering::Relaxed);
            self.backend.kcore(csr)
        } else {
            CpuFallbackBackend.kcore(csr)
        }
    }

    /// Dispatches Degree Centrality to GPU or CPU.
    pub fn degree(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u32, u32, u32)>> {
        if self.should_use_gpu(csr, true) {
            self.dispatch_count.fetch_add(1, Ordering::Relaxed);
            self.backend.degree(csr)
        } else {
            CpuFallbackBackend.degree(csr)
        }
    }

    /// Dispatches Betweenness Centrality to GPU or CPU.
    pub fn betweenness(
        &self,
        csr: &ChunkedCsr,
        normalized: bool,
    ) -> GdbResult<Vec<(VertexId, f64)>> {
        if self.should_use_gpu(csr, true) {
            self.dispatch_count.fetch_add(1, Ordering::Relaxed);
            self.backend.betweenness(csr, normalized)
        } else {
            CpuFallbackBackend.betweenness(csr, normalized)
        }
    }

    /// Dispatches Harmonic Closeness Centrality to GPU or CPU.
    pub fn closeness(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, f64)>> {
        if self.should_use_gpu(csr, true) {
            self.dispatch_count.fetch_add(1, Ordering::Relaxed);
            self.backend.closeness(csr)
        } else {
            CpuFallbackBackend.closeness(csr)
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
        if self.enabled && !vectors.is_empty() {
            self.dispatch_count.fetch_add(1, Ordering::Relaxed);
        }
        self.backend.vector_similarity(vectors, dim, query, k, metric)
    }
}
