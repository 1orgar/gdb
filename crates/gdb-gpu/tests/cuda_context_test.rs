use gdb_gpu::cuda::CudaComputeBackend;
use gdb_gpu::GpuDispatcher;

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
    assert!(!disp.backend_name().is_empty());
}
