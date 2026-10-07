use gdb_analytics::AnalyticsEngine;
use gdb_core::schema::{DataType, GraphSchema, PropertySpec};
use gdb_core::{DataValue, EdgeId, VertexId};
use gdb_s3::{csr_to_parquet, S3StorageManager};
use gdb_storage::PartitionStorageEngine;
use object_store::memory::InMemory;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("\x1b[1;36m");
    println!("============================================================");
    println!(" GDB Enterprise: Bulk Loading & Graph Analytics Pipeline   ");
    println!("============================================================\x1b[0m\n");

    // 1. Initialize Schema
    let mut schema = GraphSchema::new("social_network");
    let user_label = schema.register_vertex_label(
        "User",
        vec![
            PropertySpec::new("name", DataType::String, false),
            PropertySpec::new("age", DataType::Int64, false),
        ],
    )?;
    let follows_edge = schema.register_edge_type("FOLLOWS", vec![])?;
    let schema_ref = Arc::new(RwLock::new(schema));

    let engine = PartitionStorageEngine::new(0, schema_ref.clone());

    // 2. High-Throughput Bulk Ingestion: 50,000 Vertices and 1,000,000 Edges
    let num_vertices: u64 = 50_000;
    let avg_degree: u64 = 20;
    let total_edges = num_vertices * avg_degree;

    println!("\x1b[1;33m[1/5] Ingesting {} vertices into In-Memory Arrow Storage...\x1b[0m", num_vertices);
    let start_v = Instant::now();
    for vid in 0..num_vertices {
        let mut props = HashMap::new();
        props.insert("name".into(), DataValue::String(format!("User_{}", vid)));
        props.insert("age".into(), DataValue::Int64(20 + (vid % 50) as i64));
        engine.set_vertex_properties(VertexId(vid), user_label, props)?;
    }
    let elapsed_v = start_v.elapsed();
    println!("  -> Vertices ingested in {:?} ({:.0} vertices/sec)", elapsed_v, num_vertices as f64 / elapsed_v.as_secs_f64());

    println!("\x1b[1;33m[2/5] Bulk-loading {} edges into Lock-Free Delta MemTable...\x1b[0m", total_edges);
    let start_e = Instant::now();
    let ver = engine.next_commit_version();
    for src in 0..num_vertices {
        for d in 1..=avg_degree {
            let dst = (src + d * 7) % num_vertices;
            engine.insert_edge(EdgeId::simple(VertexId(src), follows_edge, VertexId(dst)), ver);
        }
    }
    let elapsed_e = start_e.elapsed();
    println!("  -> Edges ingested in {:?} ({:.2} MILLION edges/sec)", elapsed_e, (total_edges as f64 / elapsed_e.as_secs_f64()) / 1_000_000.0);

    // 3. Compact into Cache-Aligned Chunked-CSR
    println!("\x1b[1;33m[3/5] Compacting Delta buffer into dense Chunked-CSR topology...\x1b[0m");
    let start_c = Instant::now();
    engine.compact();
    let elapsed_c = start_c.elapsed();
    let csr = engine.current_csr();
    println!("  -> Compaction completed in {:?}", elapsed_c);
    println!("  -> In-Memory CSR: {} vertices, {} edges", csr.vertex_count(), csr.edge_count());

    // 4. Run Enterprise Graph Analytics Algorithms
    println!("\x1b[1;33m[4/5] Running Nebula Enterprise Analytics Suite on Apple M5...\x1b[0m\n");

    // Algorithm A: PageRank
    let start_pr = Instant::now();
    let pr_batch = AnalyticsEngine::run_pagerank(&csr, 0.85, 20, 1e-5)?;
    let time_pr = start_pr.elapsed();
    println!("  \x1b[1;32m[✓] PageRank (20 iterations, damping 0.85):\x1b[0m took {:?}", time_pr);
    println!("      Calculated scores for {} vertices", pr_batch.num_rows());

    // Algorithm B: Weakly Connected Components (WCC)
    let start_wcc = Instant::now();
    let wcc_batch = AnalyticsEngine::run_wcc(&csr)?;
    let time_wcc = start_wcc.elapsed();
    println!("  \x1b[1;32m[✓] WCC (Weakly Connected Components):\x1b[0m took {:?}", time_wcc);
    println!("      Found components for {} vertices", wcc_batch.num_rows());

    // Algorithm C: Louvain Community Detection
    let start_louvain = Instant::now();
    let louvain_batch = AnalyticsEngine::run_louvain(&csr, 5)?;
    let time_louvain = start_louvain.elapsed();
    println!("  \x1b[1;32m[✓] Louvain Community Detection (5 iters):\x1b[0m took {:?}", time_louvain);
    println!("      Assigned communities to {} vertices", louvain_batch.num_rows());

    // Algorithm D: Single-Source Shortest Path (SSSP)
    let start_sssp = Instant::now();
    let sssp_batch = AnalyticsEngine::run_sssp(&csr, VertexId(0))?;
    let time_sssp = start_sssp.elapsed();
    println!("  \x1b[1;32m[✓] SSSP (Breadth-First Shortest Path from v#0):\x1b[0m took {:?}", time_sssp);
    println!("      Calculated shortest distances to {} vertices", sssp_batch.num_rows());

    // 5. Tiering & Parquet Snapshot to S3
    println!("\n\x1b[1;33m[5/5] Creating compressed Parquet snapshot and uploading to S3...\x1b[0m");
    let start_s3 = Instant::now();
    let s3_store = Arc::new(InMemory::new());
    let s3_manager = S3StorageManager::new(s3_store);
    let key = s3_manager.upload_csr_snapshot(0, ver, &engine).await?;
    let parquet_bytes = csr_to_parquet(&csr)?;
    let elapsed_s3 = start_s3.elapsed();
    println!("  -> Uploaded snapshot to S3 key: {}", key);
    println!("  -> Parquet file size: {:.2} MB (ZSTD compressed in {:?})", parquet_bytes.len() as f64 / (1024.0 * 1024.0), elapsed_s3);

    println!("\x1b[1;32m\nAll pipeline steps completed successfully!\x1b[0m");
    Ok(())
}
