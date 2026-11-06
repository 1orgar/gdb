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

        let dispatcher = GpuDispatcher::enabled(0, 1);
        assert!(dispatcher.enabled);
        assert_eq!(dispatcher.device_id, 0);

        let default_disp = GpuDispatcher::default();
        assert!(!default_disp.enabled);

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
        assert!(!cuda.name().is_empty());
        assert!(!cuda.device_name().is_empty());
        let _ = cuda.has_active_cuda_context();
        let _ = CudaComputeBackend::is_available();

        let bfs = cuda.parallel_bfs(&csr, VertexId(10), 2).unwrap();
        assert_eq!(bfs.len(), 3);

        let pr = cuda.pagerank(&csr, 0.85, 10).unwrap();
        assert_eq!(pr.len(), 3);

        let wcc = cuda.wcc(&csr).unwrap();
        assert_eq!(wcc.len(), 3);

        let louvain = cuda.louvain(&csr, 5).unwrap();
        assert_eq!(louvain.len(), 3);

        let tri = cuda.triangle_count(&csr).unwrap();
        assert_eq!(tri.len(), 3);
    }

    #[test]
    fn test_gpu_wcc_louvain_triangles() {
        // Triangle graph
        let edges = vec![
            EdgeId::simple(VertexId(1), EdgeType(1), VertexId(2)),
            EdgeId::simple(VertexId(2), EdgeType(1), VertexId(3)),
            EdgeId::simple(VertexId(3), EdgeType(1), VertexId(1)),
        ];
        let csr = ChunkedCsr::from_edges(edges);
        let dispatcher = GpuDispatcher::enabled(0, 1);

        // WCC: all in same component
        let wcc_res = dispatcher.wcc(&csr).unwrap();
        assert_eq!(wcc_res.len(), 3);
        let comp0 = wcc_res[0].1;
        assert_eq!(wcc_res[1].1, comp0);
        assert_eq!(wcc_res[2].1, comp0);

        // Louvain
        let louvain_res = dispatcher.louvain(&csr, 10).unwrap();
        assert_eq!(louvain_res.len(), 3);

        // Triangles
        let tri_res = dispatcher.triangle_count(&csr).unwrap();
        assert_eq!(tri_res.len(), 3);
        for (_, t) in &tri_res {
            assert_eq!(*t, 1);
        }

        // Triangles with LCC
        let tri_lcc = dispatcher.triangle_count_with_lcc(&csr).unwrap();
        assert_eq!(tri_lcc.len(), 3);
        for (_, t, lcc) in &tri_lcc {
            assert_eq!(*t, 1);
            assert!((*lcc - 1.0).abs() < 1e-6);
        }

        // SSSP
        let sssp_res = dispatcher.sssp(&csr, VertexId(1)).unwrap();
        assert_eq!(sssp_res.len(), 3);

        // Degree
        let deg_res = dispatcher.degree(&csr).unwrap();
        assert_eq!(deg_res.len(), 3);
        for (_, in_d, out_d, tot_d) in deg_res {
            assert_eq!(in_d, 1);
            assert_eq!(out_d, 1);
            assert_eq!(tot_d, 2);
        }

        // K-Core, LPA, Betweenness, Closeness
        let kcore_res = dispatcher.kcore(&csr).unwrap();
        assert_eq!(kcore_res.len(), 3);
        for (_, k) in kcore_res {
            assert_eq!(k, 2);
        }

        let lpa_res = dispatcher.lpa(&csr, 10).unwrap();
        assert_eq!(lpa_res.len(), 3);

        let bet_res = dispatcher.betweenness(&csr, true).unwrap();
        assert_eq!(bet_res.len(), 3);

        let close_res = dispatcher.closeness(&csr).unwrap();
        assert_eq!(close_res.len(), 3);

        // Vector similarity on GPU
        let vecs = vec![1.0f32, 0.0, 0.0, 1.0];
        let sim_res = dispatcher.vector_similarity(&vecs, 2, &[1.0, 0.0], 2, "cosine").unwrap();
        assert_eq!(sim_res.len(), 2);
        assert_eq!(sim_res[0].0, 0);
        assert!((sim_res[0].1 - 1.0).abs() < 1e-5);
        assert!(dispatcher.total_dispatches() >= 8);
    }
}
