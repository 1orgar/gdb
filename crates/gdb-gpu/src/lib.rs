pub mod backend;
pub mod cuda;
pub mod dispatcher;
pub mod metal;

pub use backend::{BfsResult, CpuFallbackBackend, GpuComputeBackend, PageRankResult};
pub use cuda::CudaComputeBackend;
pub use dispatcher::GpuDispatcher;
pub use metal::MetalComputeBackend;

#[cfg(test)]
mod tests {
    use super::*;
    use gdb_core::{EdgeId, EdgeType, VertexId};
    use gdb_storage::ChunkedCsr;

    #[test]
    fn test_gpu_bfs_and_pagerank() {
        // Triangle graph: 1 -> 2, 2 -> 3, 3 -> 1
        let edges = vec![
            EdgeId::simple(VertexId(1), EdgeType(1), VertexId(2)),
            EdgeId::simple(VertexId(2), EdgeType(1), VertexId(3)),
            EdgeId::simple(VertexId(3), EdgeType(1), VertexId(1)),
        ];
        let csr = ChunkedCsr::from_edges(edges);

        let dispatcher = GpuDispatcher::new().with_threshold(1);

        // Test BFS from Vertex 1
        let bfs_res = dispatcher.bfs(&csr, VertexId(1), 2).unwrap();
        assert_eq!(bfs_res.len(), 3);

        let dist_map: std::collections::HashMap<VertexId, u32> = bfs_res.into_iter().collect();
        assert_eq!(dist_map[&VertexId(1)], 0);
        assert_eq!(dist_map[&VertexId(2)], 1);
        assert_eq!(dist_map[&VertexId(3)], 2);

        // Test PageRank
        let pr_res = dispatcher.pagerank(&csr, 0.85, 20).unwrap();
        assert_eq!(pr_res.len(), 3);
        // In a symmetric triangle, all 3 vertices should have approximately equal PageRank (1/3)
        for (_, score) in pr_res {
            assert!((score - 0.333).abs() < 0.05);
        }
    }

    #[test]
    fn test_cuda_backend_execution() {
        let edges = vec![
            EdgeId::simple(VertexId(10), EdgeType(1), VertexId(20)),
            EdgeId::simple(VertexId(20), EdgeType(1), VertexId(30)),
        ];
        let csr = ChunkedCsr::from_edges(edges);
        let cuda = CudaComputeBackend::new();

        assert_eq!(cuda.device_id(), 0);
        let bfs = cuda.parallel_bfs(&csr, VertexId(10), 2).unwrap();
        assert_eq!(bfs.len(), 3);

        let pr = cuda.pagerank(&csr, 0.85, 10).unwrap();
        assert_eq!(pr.len(), 3);
    }
}
