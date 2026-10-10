use gdb_core::{EdgeId, EdgeType, VertexId};
use gdb_storage::ChunkedCsr;
use gdb_analytics::node2vec;

#[test]
fn test_node2vec_large_sparse_vertex_ids() {
    // Construct edges between large non-contiguous vertex IDs
    // (replicating the benchmark where IDs are around 1,822,546 while num_v is smaller)
    let edges = vec![
        EdgeId::new(VertexId(100_000), EdgeType(1), 0, VertexId(1_822_546)),
        EdgeId::new(VertexId(1_822_546), EdgeType(1), 0, VertexId(2_000_000)),
        EdgeId::new(VertexId(2_000_000), EdgeType(1), 0, VertexId(100_000)),
        EdgeId::new(VertexId(100_000), EdgeType(1), 0, VertexId(3_500_000)),
        EdgeId::new(VertexId(3_500_000), EdgeType(1), 0, VertexId(2_000_000)),
    ];

    let csr = ChunkedCsr::from_edges(edges);
    assert_eq!(csr.vertex_count(), 4); // Only 4 distinct vertices

    // This must not panic with out-of-bounds index on 1_822_546 or 3_500_000
    let embeddings = node2vec(&csr, 16, 5, 2, 1.0, 1.0);

    assert_eq!(embeddings.len(), 4);
    for (vid, emb) in embeddings {
        assert_eq!(emb.len(), 16);
        assert!(
            vid.0 == 100_000 || vid.0 == 1_822_546 || vid.0 == 2_000_000 || vid.0 == 3_500_000,
            "Unexpected vertex ID: {}", vid.0
        );
    }
}

#[test]
fn test_node2vec_disconnected_and_edge_cases() {
    let csr = ChunkedCsr::new();
    let res = node2vec(&csr, 16, 5, 2, 1.0, 1.0);
    assert!(res.is_empty());

    let edges = vec![
        EdgeId::new(VertexId(42), EdgeType(1), 0, VertexId(42)), // self-loop
    ];
    let csr = ChunkedCsr::from_edges(edges);
    let res = node2vec(&csr, 8, 3, 1, 1.0, 1.0);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].0, VertexId(42));
    assert_eq!(res[0].1.len(), 8);
}
