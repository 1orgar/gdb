pub mod betweenness;
pub mod closeness;
pub mod degree;
pub mod engine;
pub mod kcore;
pub mod louvain;
pub mod lpa;
pub mod node2vec;
pub mod pagerank;
pub mod scc;
pub mod similarity;
pub mod sssp;
pub mod triangles;
pub mod wcc;

pub use betweenness::betweenness_centrality;
pub use closeness::closeness_centrality;
pub use degree::{degree_centrality, DegreeMetric};
pub use engine::AnalyticsEngine;
pub use kcore::k_core_decomposition;
pub use louvain::louvain;
pub use lpa::label_propagation;
pub use node2vec::node2vec;
pub use pagerank::pagerank;
pub use scc::strongly_connected_components;
pub use similarity::{common_neighbors, cosine_similarity, jaccard_similarity};
pub use sssp::single_source_shortest_path;
pub use triangles::{triangle_count, TriangleMetric};
pub use wcc::weakly_connected_components;

#[cfg(test)]
mod tests {
    use super::*;
    use gdb_core::{EdgeId, EdgeType, VertexId};
    use gdb_storage::ChunkedCsr;

    fn build_test_triangle_graph() -> ChunkedCsr {
        // Triangle: 1 <-> 2, 2 <-> 3, 3 <-> 1 (bidirectional)
        let et = EdgeType(1);
        let edges = vec![
            EdgeId::simple(VertexId(1), et, VertexId(2)),
            EdgeId::simple(VertexId(2), et, VertexId(1)),
            EdgeId::simple(VertexId(2), et, VertexId(3)),
            EdgeId::simple(VertexId(3), et, VertexId(2)),
            EdgeId::simple(VertexId(3), et, VertexId(1)),
            EdgeId::simple(VertexId(1), et, VertexId(3)),
        ];
        ChunkedCsr::from_edges(edges)
    }

    #[test]
    fn test_wcc_and_scc() {
        let csr = build_test_triangle_graph();
        let wcc_res = weakly_connected_components(&csr);
        assert_eq!(wcc_res.len(), 3);
        // All vertices belong to the same connected component
        let c1 = wcc_res[&VertexId(1)];
        let c2 = wcc_res[&VertexId(2)];
        let c3 = wcc_res[&VertexId(3)];
        assert_eq!(c1, c2);
        assert_eq!(c2, c3);

        let scc_res = strongly_connected_components(&csr);
        assert_eq!(scc_res.len(), 3);
        assert_eq!(scc_res[&VertexId(1)], scc_res[&VertexId(2)]);
    }

    #[test]
    fn test_triangle_count_and_lcc() {
        let csr = build_test_triangle_graph();
        let tri_res = triangle_count(&csr);
        assert_eq!(tri_res.len(), 3);
        // In a triangle, each vertex is part of 1 triangle, and LCC is 1.0 (clique)
        for (_, metric) in tri_res {
            assert_eq!(metric.triangles, 1);
            assert!((metric.clustering_coefficient - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_kcore_and_degree() {
        let csr = build_test_triangle_graph();
        let kcore_res = k_core_decomposition(&csr);
        assert_eq!(kcore_res.len(), 3);
        // Triangle has degree 2 for all nodes, so coreness is 2
        for (_, k) in kcore_res {
            assert_eq!(k, 2);
        }

        let deg_res = degree_centrality(&csr);
        for (_, deg) in deg_res {
            assert_eq!(deg.in_degree, 2);
            assert_eq!(deg.out_degree, 2);
            assert_eq!(deg.total_degree, 4);
        }
    }

    #[test]
    fn test_sssp_and_similarity() {
        let csr = build_test_triangle_graph();
        let sssp_res = single_source_shortest_path(&csr, VertexId(1));
        assert_eq!(sssp_res[&VertexId(1)], 0);
        assert_eq!(sssp_res[&VertexId(2)], 1);
        assert_eq!(sssp_res[&VertexId(3)], 1);

        let jaccard = jaccard_similarity(&csr, VertexId(1), VertexId(2));
        assert!(jaccard > 0.0);
        let common = common_neighbors(&csr, VertexId(1), VertexId(2));
        assert_eq!(common, 1); // Vertex 3 is common neighbor

        let cosine = cosine_similarity(&csr, VertexId(1), VertexId(2));
        assert!(cosine > 0.0);
        let cosine_self = cosine_similarity(&csr, VertexId(1), VertexId(1));
        assert!((cosine_self - 1.0).abs() < 1e-6);

        // Empty CSR similarity
        let empty_csr = ChunkedCsr::new();
        assert_eq!(jaccard_similarity(&empty_csr, VertexId(1), VertexId(2)), 1.0);
        assert_eq!(cosine_similarity(&empty_csr, VertexId(1), VertexId(2)), 0.0);
        assert_eq!(common_neighbors(&empty_csr, VertexId(1), VertexId(2)), 0);
    }

    #[test]
    fn test_betweenness_and_closeness_and_lpa() {
        let csr = build_test_triangle_graph();

        // Betweenness
        let bc_unnorm = betweenness_centrality(&csr, false);
        assert_eq!(bc_unnorm.len(), 3);
        let bc_norm = betweenness_centrality(&csr, true);
        assert_eq!(bc_norm.len(), 3);

        // Closeness
        let cc = closeness_centrality(&csr);
        assert_eq!(cc.len(), 3);
        for (_, score) in cc {
            assert!(score > 0.0);
        }

        // LPA
        let lpa = label_propagation(&csr, 10);
        assert_eq!(lpa.len(), 3);

        // Empty graph handling
        let empty_csr = ChunkedCsr::new();
        assert!(betweenness_centrality(&empty_csr, false).is_empty());
        assert!(closeness_centrality(&empty_csr).is_empty());
        assert!(label_propagation(&empty_csr, 10).is_empty());
    }

    #[test]
    fn test_analytics_engine_record_batches() {
        let csr = build_test_triangle_graph();
        let pr_batch = AnalyticsEngine::run_pagerank(&csr, 0.85, 20, 1e-5).unwrap();
        assert_eq!(pr_batch.num_rows(), 3);
        assert_eq!(pr_batch.num_columns(), 2);

        let wcc_batch = AnalyticsEngine::run_wcc(&csr).unwrap();
        assert_eq!(wcc_batch.num_rows(), 3);

        let scc_batch = AnalyticsEngine::run_scc(&csr).unwrap();
        assert_eq!(scc_batch.num_rows(), 3);

        let louvain_batch = AnalyticsEngine::run_louvain(&csr, 10).unwrap();
        assert_eq!(louvain_batch.num_rows(), 3);

        let lpa_batch = AnalyticsEngine::run_lpa(&csr, 10).unwrap();
        assert_eq!(lpa_batch.num_rows(), 3);

        let kcore_batch = AnalyticsEngine::run_kcore(&csr).unwrap();
        assert_eq!(kcore_batch.num_rows(), 3);

        let tri_batch = AnalyticsEngine::run_triangle_count(&csr).unwrap();
        assert_eq!(tri_batch.num_rows(), 3);

        let bc_batch = AnalyticsEngine::run_betweenness(&csr, true).unwrap();
        assert_eq!(bc_batch.num_rows(), 3);

        let cc_batch = AnalyticsEngine::run_closeness(&csr).unwrap();
        assert_eq!(cc_batch.num_rows(), 3);

        let deg_batch = AnalyticsEngine::run_degree(&csr).unwrap();
        assert_eq!(deg_batch.num_rows(), 3);

        let sssp_batch = AnalyticsEngine::run_sssp(&csr, VertexId(1)).unwrap();
        assert_eq!(sssp_batch.num_rows(), 3);

        let sim_batch = AnalyticsEngine::run_similarity(&csr, VertexId(1), VertexId(2)).unwrap();
        assert_eq!(sim_batch.num_rows(), 1);
        assert_eq!(sim_batch.num_columns(), 5);
    }
}

