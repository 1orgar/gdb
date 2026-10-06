use gdb_core::schema::GraphSchema;
use gdb_core::{EdgeId, EdgeType, VertexId};
use gdb_storage::PartitionStorageEngine;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Instant;

#[test]
fn test_extreme_traversal_speed() {
    let schema = Arc::new(RwLock::new(GraphSchema::new("bench")));
    let engine = PartitionStorageEngine::new(0, schema);

    // Build a graph with 10,000 vertices, each with 20 outgoing edges = 200,000 edges
    let num_v = 10_000u64;
    let degree = 20u64;
    let et = EdgeType(1);

    let ver = engine.next_commit_version();
    for src in 0..num_v {
        for d in 1..=degree {
            let dst = (src + d) % num_v;
            engine.insert_edge(EdgeId::simple(VertexId(src), et, VertexId(dst)), ver);
        }
    }

    // Compact into dense Chunked-CSR
    engine.compact();

    // Now benchmark traversal: perform 500,000 vertex lookups (10,000,000 edge visits)
    let lookups: usize = 500_000;
    let start = Instant::now();

    let mut total_edges_visited: usize = 0;
    for i in 0..lookups {
        let vid = VertexId((i as u64) % num_v);
        let edges = engine.get_out_edges(vid, Some(et), ver);
        total_edges_visited += edges.len();
    }

    let elapsed = start.elapsed();
    let hops_per_sec = (total_edges_visited as f64) / elapsed.as_secs_f64();

    println!("\n==================================================");
    println!(
        "CSR Traversal Benchmark on Apple M5:\nLookups: {}\nEdges visited: {}\nTime: {:?}\nThroughput: {:.2} MILLION hops/sec",
        lookups, total_edges_visited, elapsed, hops_per_sec / 1_000_000.0
    );
    println!("==================================================\n");

    assert_eq!(total_edges_visited, lookups * (degree as usize));
    assert!(hops_per_sec > 5_000_000.0);
}
