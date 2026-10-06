pub mod betweenness;
pub mod closeness;
pub mod degree;
pub mod engine;
pub mod kcore;
pub mod louvain;
pub mod lpa;
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
    }

    #[test]
    fn test_analytics_engine_record_batches() {
        let csr = build_test_triangle_graph();
        let pr_batch = AnalyticsEngine::run_pagerank(&csr, 0.85, 20, 1e-5).unwrap();
        assert_eq!(pr_batch.num_rows(), 3);
        assert_eq!(pr_batch.num_columns(), 2);

        let wcc_batch = AnalyticsEngine::run_wcc(&csr).unwrap();
        assert_eq!(wcc_batch.num_rows(), 3);

        let tri_batch = AnalyticsEngine::run_triangle_count(&csr).unwrap();
        assert_eq!(tri_batch.num_rows(), 3);
    }
}
