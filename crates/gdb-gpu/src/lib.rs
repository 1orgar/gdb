pub mod backend;
pub mod dispatcher;
pub mod metal;

pub use backend::{BfsResult, CpuFallbackBackend, GpuComputeBackend, PageRankResult};
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
}
