use gdb_gpu::cuda::CudaComputeBackend;
use gdb_gpu::{GpuComputeBackend, GpuDispatcher};

#[test]
fn test_cuda_compute_backend_creation() {
    let backend = CudaComputeBackend::with_device(0);
    assert_eq!(backend.device_id(), 0);
    assert!(!backend.device_name().is_empty());

    // Fallback or active context
    let is_avail = CudaComputeBackend::is_available();
    println!("CUDA is_available: {}, has_active_cuda_context: {}", is_avail, backend.has_active_cuda_context());
}

#[test]
fn test_gpu_dispatcher_fallback_behavior() {
    let disp = GpuDispatcher::new(true, 0, 500);
    assert!(disp.enabled);
    assert_eq!(disp.device_id, 0);
    assert_eq!(disp.threshold_edges, 500);
    assert_eq!(disp.max_vram_bytes(), GpuDispatcher::DEFAULT_MAX_VRAM_BYTES);
    assert!(!disp.backend_name().is_empty());

    let disp2 = disp.with_max_vram(1024 * 1024).with_threshold(100).with_device(1);
    assert_eq!(disp2.max_vram_bytes(), 1024 * 1024);
    assert_eq!(disp2.threshold_edges, 100);
    assert_eq!(disp2.device_id, 1);
}

#[test]
fn test_cuda_vram_paging_streaming() {
    use gdb_core::{EdgeId, EdgeType, VertexId};
    use gdb_storage::ChunkedCsr;

    // Small graph: 4 nodes
    let edges = vec![
        EdgeId::simple(VertexId(1), EdgeType(0), VertexId(2)),
        EdgeId::simple(VertexId(2), EdgeType(0), VertexId(3)),
        EdgeId::simple(VertexId(3), EdgeType(0), VertexId(4)),
        EdgeId::simple(VertexId(4), EdgeType(0), VertexId(1)),
    ];
    let csr = ChunkedCsr::from_edges(edges);

    // Backend with tiny VRAM limit (16 bytes = max 2 edges per chunk)
    let backend = CudaComputeBackend::with_device_and_vram(0, 16);
    assert_eq!(backend.max_vram_bytes(), 16);

    let bfs = backend.parallel_bfs(&csr, VertexId(1), 3).unwrap();
    assert_eq!(bfs.len(), 4);

    let pr = backend.pagerank(&csr, 0.85, 10).unwrap();
    assert_eq!(pr.len(), 4);
}
