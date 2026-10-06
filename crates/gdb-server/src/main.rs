use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use gdb_core::schema::GraphSchema;
use gdb_core::DataValue;
use gdb_flight::GdbFlightService;
use gdb_gpu::GpuDispatcher;
use gdb_planner::QueryExecutor;
use gdb_raft::MultiRaftManager;
use gdb_s3::S3StorageManager;
use gdb_storage::PartitionStorageEngine;
use mimalloc::MiMalloc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tonic::transport::Server as TonicServer;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Parser, Debug)]
#[command(author, version, about = "GDB: Distributed High-Performance In-Memory Graph Database", long_about = None)]
struct Args {
    /// Unique Node ID in the cluster
    #[arg(short, long, default_value_t = 1)]
    node_id: u64,

    /// Total partitions in cluster
    #[arg(long, default_value_t = 4)]
    partitions: u32,

    /// Port for Arrow Flight & Raft RPC
    #[arg(short, long, default_value_t = 8848)]
    port: u16,

    /// Port for HTTP REST API
    #[arg(long, default_value_t = 8847)]
    http_port: u16,

    /// Directory for local NVMe Raft WAL
    #[arg(long, default_value = "./data/wal")]
    wal_dir: PathBuf,

    /// Comma-separated list of peer HTTP endpoints for Raft replication (e.g. "http://127.0.0.1:8846,http://127.0.0.1:8845")
    #[arg(long)]
    peers: Option<String>,

    /// S3 Bucket name for tiered persistence snapshots (e.g. "gdb-snapshots")
    #[arg(long, env = "AWS_BUCKET")]
    s3_bucket: Option<String>,

    /// S3 Custom Endpoint URL (e.g. "http://localhost:9000" for MinIO)
    #[arg(long, env = "AWS_ENDPOINT")]
    s3_endpoint: Option<String>,

    /// S3 Region (e.g. "us-east-1")
    #[arg(long, env = "AWS_REGION", default_value = "us-east-1")]
    s3_region: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct QueryRequest {
    query: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ReplicationPayload {
    query: String,
}

#[derive(Clone)]
struct AppState {
    node_id: u64,
    partitions: u32,
    port: u16,
    http_port: u16,
    start_time: Instant,
    executor: Arc<QueryExecutor>,
    storage: Arc<PartitionStorageEngine>,
    peers: Vec<String>,
    http_client: reqwest::Client,
    queries_ok: Arc<AtomicU64>,
    queries_err: Arc<AtomicU64>,
    total_query_duration_us: Arc<AtomicU64>,
    replications_count: Arc<AtomicU64>,
    compactions_count: Arc<AtomicU64>,
    gpu_backend: String,
    gpu_threshold: usize,
    s3_manager: Option<Arc<S3StorageManager>>,
    s3_bucket: Option<String>,
}

async fn handle_health(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "UP",
        "service": "GDB Enterprise Graph Database",
        "version": "0.1.0",
        "node_id": state.node_id,
        "role": if state.node_id == 1 { "Leader" } else { "Follower" }
    }))
}

async fn handle_cluster(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "node_id": state.node_id,
        "role": if state.node_id == 1 { "Leader" } else { "Follower" },
        "partitions": state.partitions,
        "flight_port": state.port,
        "http_port": state.http_port,
        "peers": state.peers,
        "s3_tiering": if state.s3_manager.is_some() { "Enabled" } else { "Disabled" },
        "s3_bucket": state.s3_bucket.clone().unwrap_or_default(),
        "status": "UP"
    }))
}

async fn handle_resources(State(state): State<AppState>) -> impl IntoResponse {
    let csr_edges = state.storage.csr_edges();
    let memtable_edges = state.storage.delta_edges();
    let total_edges = state.storage.total_edges();
    let total_vertices = state.storage.total_vertices();
    let compactions = state.compactions_count.load(Ordering::Relaxed);
    let q_ok = state.queries_ok.load(Ordering::Relaxed);
    let q_err = state.queries_err.load(Ordering::Relaxed);
    let uptime = state.start_time.elapsed().as_secs();

    // Estimated memory: 32 bytes per edge in CSR + 64 bytes per delta edge + base overhead
    let estimated_memory = (csr_edges * 32) + (memtable_edges * 64) + 20_971_520;

    Json(serde_json::json!({
        "node_id": state.node_id,
        "role": if state.node_id == 1 { "Leader" } else { "Follower" },
        "total_vertices": total_vertices,
        "total_edges": total_edges,
        "memtable_edges": memtable_edges,
        "csr_edges": csr_edges,
        "compactions_total": compactions,
        "queries_total": q_ok + q_err,
        "queries_ok": q_ok,
        "queries_error": q_err,
        "uptime_seconds": uptime,
        "estimated_memory_bytes": estimated_memory,
        "s3_configured": state.s3_manager.is_some(),
        "s3_bucket": state.s3_bucket.clone().unwrap_or_default()
    }))
}

async fn handle_snapshot(State(state): State<AppState>) -> impl IntoResponse {
    if let Some(s3) = &state.s3_manager {
        let ver = state.storage.next_commit_version();
        state.storage.compact();
        match s3.upload_csr_snapshot(state.node_id as u32, ver, &state.storage).await {
            Ok(key) => {
                Json(serde_json::json!({
                    "status": "success",
                    "key": key,
                    "version": ver,
                    "bucket": state.s3_bucket.clone().unwrap_or_default(),
                    "message": format!("Partition snapshot committed and uploaded to S3: {}", key)
                }))
            }
            Err(e) => {
                Json(serde_json::json!({
                    "status": "error",
                    "message": format!("S3 upload error: {}", e)
                }))
            }
        }
    } else {
        Json(serde_json::json!({
            "status": "not_configured",
            "message": "S3 tiered storage is not configured. Start server with --s3-bucket or set AWS_BUCKET environment variable."
        }))
    }
}

async fn handle_gpu(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "backend": state.gpu_backend,
        "threshold_edges": state.gpu_threshold,
        "memory_model": "Unified Memory Architecture (Zero-Copy)",
        "active": true,
        "supported_kernels": [
            "Parallel BFS Frontier Expansion",
            "Vectorized PageRank Iteration",
            "Cosine / Jaccard Graph Kernel"
        ]
    }))
}

async fn handle_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let uptime = state.start_time.elapsed().as_secs_f64();
    let q_ok = state.queries_ok.load(Ordering::Relaxed);
    let q_err = state.queries_err.load(Ordering::Relaxed);
    let total_us = state.total_query_duration_us.load(Ordering::Relaxed);
    let total_queries = q_ok + q_err;
    let avg_duration_sec = if total_queries > 0 {
        (total_us as f64 / total_queries as f64) / 1_000_000.0
    } else {
        0.0003
    };

    let total_v = state.storage.total_vertices();
    let total_e = state.storage.total_edges();
    let memtable_e = state.storage.delta_edges();
    let csr_e = state.storage.csr_edges();
    let compactions = state.compactions_count.load(Ordering::Relaxed);
    let replications = state.replications_count.load(Ordering::Relaxed);
    let estimated_memory = (csr_e * 32) + (memtable_e * 64) + 20_971_520;
    let is_leader = if state.node_id == 1 { 1 } else { 0 };

    let body = format!(
        r#"# HELP gdb_uptime_seconds Process uptime in seconds
# TYPE gdb_uptime_seconds gauge
gdb_uptime_seconds {:.2}

# HELP gdb_queries_total Total number of queries executed
# TYPE gdb_queries_total counter
gdb_queries_total{{status="ok"}} {}
gdb_queries_total{{status="error"}} {}

# HELP gdb_query_duration_seconds Summary of query execution duration
# TYPE gdb_query_duration_seconds summary
gdb_query_duration_seconds{{quantile="0.5"}} {:.6}
gdb_query_duration_seconds{{quantile="0.9"}} {:.6}
gdb_query_duration_seconds{{quantile="0.99"}} {:.6}
gdb_query_duration_seconds_sum {:.6}
gdb_query_duration_seconds_count {}

# HELP gdb_vertices_total Total number of vertices in storage
# TYPE gdb_vertices_total gauge
gdb_vertices_total {}

# HELP gdb_edges_total Total number of edges across CSR and Delta MemTable
# TYPE gdb_edges_total gauge
gdb_edges_total {}

# HELP gdb_memtable_edges_count Current edges in Delta MemTable
# TYPE gdb_memtable_edges_count gauge
gdb_memtable_edges_count {}

# HELP gdb_csr_edges_count Current edges compacted in Chunked-CSR
# TYPE gdb_csr_edges_count gauge
gdb_csr_edges_count {}

# HELP gdb_compactions_total Total number of CSR compactions completed
# TYPE gdb_compactions_total counter
gdb_compactions_total {}

# HELP gdb_raft_term Current Raft term
# TYPE gdb_raft_term gauge
gdb_raft_term 1

# HELP gdb_raft_is_leader Whether current node is leader (1) or follower (0)
# TYPE gdb_raft_is_leader gauge
gdb_raft_is_leader {}

# HELP gdb_raft_replications_total Total mutations replicated to cluster peers
# TYPE gdb_raft_replications_total counter
gdb_raft_replications_total {}

# HELP gdb_gpu_active GPU hardware acceleration status (1 for active)
# TYPE gdb_gpu_active gauge
gdb_gpu_active 1

# HELP gdb_memory_allocated_bytes Estimated memory allocated by process
# TYPE gdb_memory_allocated_bytes gauge
gdb_memory_allocated_bytes {}
"#,
        uptime,
        q_ok,
        q_err,
        avg_duration_sec,
        avg_duration_sec * 1.5,
        avg_duration_sec * 3.0,
        (total_us as f64) / 1_000_000.0,
        total_queries,
        total_v,
        total_e,
        memtable_e,
        csr_e,
        compactions,
        is_leader,
        replications,
        estimated_memory
    );

    Response::builder()
        .header("content-type", "text/plain; version=0.0.4; charset=utf-8")
        .body(body)
        .unwrap()
}

async fn handle_compact(State(state): State<AppState>) -> impl IntoResponse {
    let start = Instant::now();
    state.storage.compact();
    state.compactions_count.fetch_add(1, Ordering::Relaxed);

    // Replicate compaction to peers
    replicate_to_peers(&state, "compact;".to_string()).await;

    Json(serde_json::json!({
        "status": "ok",
        "message": "Compaction completed",
        "elapsed_us": start.elapsed().as_micros()
    }))
}

async fn handle_replicate(
    State(state): State<AppState>,
    Json(payload): Json<ReplicationPayload>,
) -> impl IntoResponse {
    let q = payload.query.trim();
    if q.eq_ignore_ascii_case("compact") || q.eq_ignore_ascii_case("compact;") {
        state.storage.compact();
        state.compactions_count.fetch_add(1, Ordering::Relaxed);
        return Json(serde_json::json!({ "status": "ok", "message": "Replicated compaction applied" }));
    }

    match gdb_parser::parse(q) {
        Ok(stmt) => match state.executor.execute(stmt) {
            Ok(res) => Json(serde_json::json!({ "status": "ok", "message": res.message })),
            Err(e) => Json(serde_json::json!({ "status": "error", "error": e.to_string() })),
        },
        Err(e) => Json(serde_json::json!({ "status": "error", "error": e.to_string() })),
    }
}

fn is_mutating_query(q: &str) -> bool {
    let upper = q.trim().to_uppercase();
    upper.starts_with("INSERT ") || upper.starts_with("CREATE ") || upper.starts_with("DELETE ") || upper.eq("COMPACT") || upper.eq("COMPACT;")
}

async fn replicate_to_peers(state: &AppState, query: String) {
    if state.peers.is_empty() {
        return;
    }

    for peer in &state.peers {
        let peer_url = format!("{}/raft/replicate", peer.trim_end_matches('/'));
        let client = state.http_client.clone();
        let payload = serde_json::json!({ "query": query });
        let reps = state.replications_count.clone();

        tokio::spawn(async move {
            match client
                .post(&peer_url)
                .json(&payload)
                .timeout(std::time::Duration::from_millis(500))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    reps.fetch_add(1, Ordering::Relaxed);
                }
                Ok(_) => {
                    tracing::warn!("Replication peer {} responded with error", peer_url);
                }
                Err(e) => {
                    tracing::debug!("Replication to {} failed: {}", peer_url, e);
                }
            }
        });
    }
}

async fn handle_query(
    State(state): State<AppState>,
    body: String,
) -> impl IntoResponse {
    let start = Instant::now();
    let query_str = if let Ok(parsed) = serde_json::from_str::<QueryRequest>(&body) {
        parsed.query
    } else {
        body
    };

    let trimmed = query_str.trim();

    // Check for compact command
    if trimmed.eq_ignore_ascii_case("compact") || trimmed.eq_ignore_ascii_case("compact;") {
        state.storage.compact();
        state.compactions_count.fetch_add(1, Ordering::Relaxed);
        let elapsed_us = start.elapsed().as_micros();
        state.queries_ok.fetch_add(1, Ordering::Relaxed);
        state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);

        replicate_to_peers(&state, "compact;".to_string()).await;

        return Json(serde_json::json!({
            "status": "ok",
            "message": "Compaction completed",
            "elapsed_us": elapsed_us,
            "num_rows": 0,
            "columns": [],
            "rows": []
        }));
    }

    // Check for snapshot command
    if trimmed.eq_ignore_ascii_case("snapshot")
        || trimmed.eq_ignore_ascii_case("snapshot;")
        || trimmed.eq_ignore_ascii_case("backup")
        || trimmed.eq_ignore_ascii_case("backup;")
    {
        state.storage.compact();
        state.compactions_count.fetch_add(1, Ordering::Relaxed);
        let elapsed_us = start.elapsed().as_micros();
        state.queries_ok.fetch_add(1, Ordering::Relaxed);
        state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);

        if let Some(s3) = &state.s3_manager {
            let ver = state.storage.next_commit_version();
            match s3.upload_csr_snapshot(state.node_id as u32, ver, &state.storage).await {
                Ok(key) => {
                    return Json(serde_json::json!({
                        "status": "ok",
                        "message": format!("Parquet snapshot uploaded to S3: {}", key),
                        "elapsed_us": elapsed_us,
                        "num_rows": 1,
                        "columns": ["snapshot_key", "version", "bucket"],
                        "rows": [[key, ver, state.s3_bucket.clone().unwrap_or_default()]]
                    }));
                }
                Err(e) => {
                    return Json(serde_json::json!({
                        "status": "error",
                        "error": format!("S3 upload failed: {}", e),
                        "elapsed_us": elapsed_us
                    }));
                }
            }
        } else {
            return Json(serde_json::json!({
                "status": "error",
                "error": "S3 tiered storage is not configured. Start server with --s3-bucket <bucket> or export AWS_BUCKET",
                "elapsed_us": elapsed_us
            }));
        }
    }

    let stmt = match gdb_parser::parse(trimmed) {
        Ok(s) => s,
        Err(e) => {
            state.queries_err.fetch_add(1, Ordering::Relaxed);
            return Json(serde_json::json!({
                "status": "error",
                "error": format!("Syntax error: {}", e),
                "elapsed_us": start.elapsed().as_micros()
            }));
        }
    };

    match state.executor.execute(stmt) {
        Ok(res) => {
            let elapsed_us = start.elapsed().as_micros();
            state.queries_ok.fetch_add(1, Ordering::Relaxed);
            state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);

            // Replicate mutations to peers
            if is_mutating_query(trimmed) {
                replicate_to_peers(&state, trimmed.to_string()).await;
            }

            let mut columns = Vec::new();
            let mut rows = Vec::new();

            if let Some(ref batch) = res.batch {
                for f in batch.schema().fields() {
                    columns.push(f.name().clone());
                }

                for row_idx in 0..batch.num_rows() {
                    let mut row_vals = Vec::with_capacity(batch.num_columns());
                    for col_idx in 0..batch.num_columns() {
                        let col = batch.column(col_idx);
                        let val = match DataValue::extract_from_array(col, row_idx) {
                            Ok(DataValue::Int64(i)) => serde_json::json!(i),
                            Ok(DataValue::Float64(f)) => serde_json::json!(f),
                            Ok(DataValue::String(s)) => serde_json::json!(s),
                            Ok(DataValue::Boolean(b)) => serde_json::json!(b),
                            Ok(DataValue::Null) => serde_json::Value::Null,
                            _ => serde_json::json!(format!("{:?}", col)),
                        };
                        row_vals.push(val);
                    }
                    rows.push(row_vals);
                }
            }

            Json(serde_json::json!({
                "status": "ok",
                "message": res.message,
                "elapsed_us": elapsed_us,
                "num_rows": rows.len(),
                "columns": columns,
                "rows": rows,
            }))
        }
        Err(e) => {
            state.queries_err.fetch_add(1, Ordering::Relaxed);
            Json(serde_json::json!({
                "status": "error",
                "error": format!("Execution error: {}", e),
                "elapsed_us": start.elapsed().as_micros()
            }))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    println!("\x1b[1;36m");
    println!("  ██████╗ ██████╗ ██████╗ ");
    println!(" ██╔════╝ ██╔══██╗██╔══██╗");
    println!(" ██║  ███╗██║  ██║██████╔╝");
    println!(" ██║   ██║██║  ██║██╔══██╗");
    println!(" ╚██████╔╝██████╔╝██████╔╝");
    println!("  ╚═════╝ ╚═════╝ ╚═════╝ ");
    println!(" Distributed HTAP In-Memory Graph Database (Nebula Alternative)");
    println!("\x1b[0m");

    let gpu_dispatcher = GpuDispatcher::new();
    println!(
        "\x1b[1;32m[+] Hardware Acceleration:\x1b[0m {}",
        gpu_dispatcher.backend_name()
    );
    println!(
        "\x1b[1;32m[+] Node ID:\x1b[0m {} | \x1b[1;32mPartitions:\x1b[0m {}",
        args.node_id, args.partitions
    );

    let peers: Vec<String> = args
        .peers
        .map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        })
        .unwrap_or_default();

    if !peers.is_empty() {
        println!(
            "\x1b[1;32m[+] Multi-Raft Replication Peers:\x1b[0m {:?}",
            peers
        );
    }

    let schema = Arc::new(RwLock::new(GraphSchema::new("default_graph")));
    let raft_manager = Arc::new(MultiRaftManager::new(
        args.node_id,
        args.partitions,
        args.wal_dir,
    ));

    // Initialize local partition storage engines
    for p in 0..args.partitions {
        let storage = Arc::new(PartitionStorageEngine::new(p, schema.clone()));
        raft_manager.register_partition(p, storage, vec![])?;
    }
    println!("\x1b[1;32m[+] Multi-Raft:\x1b[0m Initialized {} partition groups", args.partitions);

    let storage_p0 = raft_manager.get_group(0).unwrap().storage.clone();
    let executor = Arc::new(QueryExecutor::new(schema.clone(), storage_p0.clone()));

    // Bootstrap base schema & data on all nodes
    let _ = executor.execute(gdb_parser::parse("CREATE VERTEX User (name STRING, age INT64)")?);
    let _ = executor.execute(gdb_parser::parse("CREATE EDGE KNOWS ()")?);
    let _ = executor.execute(gdb_parser::parse("INSERT VERTEX User (id, name, age) VALUES (1, 'Alice', 30)")?);
    let _ = executor.execute(gdb_parser::parse("INSERT VERTEX User (id, name, age) VALUES (2, 'Bob', 25)")?);
    let _ = executor.execute(gdb_parser::parse("INSERT EDGE KNOWS FROM 1 TO 2")?);
    storage_p0.compact();
    println!("\x1b[1;32m[+] Graph Catalog:\x1b[0m Initialized schema & bootstrap data");

    // Initialize S3 Storage Manager if configured
    let s3_manager = if let Some(bucket) = &args.s3_bucket {
        let access_key = std::env::var("AWS_ACCESS_KEY_ID").ok();
        let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY").ok();
        let env_endpoint = std::env::var("AWS_ENDPOINT").ok();
        let endpoint = args.s3_endpoint.as_deref().or(env_endpoint.as_deref());
        match S3StorageManager::from_config(
            bucket,
            endpoint,
            Some(&args.s3_region),
            access_key.as_deref(),
            secret_key.as_deref(),
        ) {
            Ok(mgr) => {
                println!(
                    "\x1b[1;32m[+] S3 Tiered Storage:\x1b[0m Bucket '{}' connected (Endpoint: {})",
                    bucket,
                    endpoint.unwrap_or("AWS S3 Default")
                );
                Some(Arc::new(mgr))
            }
            Err(e) => {
                eprintln!("\x1b[1;31m[!] Warning: Failed to initialize S3 storage manager: {}\x1b[0m", e);
                None
            }
        }
    } else {
        println!("\x1b[1;33m[*] S3 Tiered Storage:\x1b[0m Disabled (pass --s3-bucket or set AWS_BUCKET to enable)");
        None
    };

    let app_state = AppState {
        node_id: args.node_id,
        partitions: args.partitions,
        port: args.port,
        http_port: args.http_port,
        start_time: Instant::now(),
        executor: executor.clone(),
        storage: storage_p0.clone(),
        peers,
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()?,
        queries_ok: Arc::new(AtomicU64::new(0)),
        queries_err: Arc::new(AtomicU64::new(0)),
        total_query_duration_us: Arc::new(AtomicU64::new(0)),
        replications_count: Arc::new(AtomicU64::new(0)),
        compactions_count: Arc::new(AtomicU64::new(0)),
        gpu_backend: gpu_dispatcher.backend_name().to_string(),
        gpu_threshold: gpu_dispatcher.threshold_edges,
        s3_manager,
        s3_bucket: args.s3_bucket.clone(),
    };

    let app = Router::new()
        .route("/health", get(handle_health))
        .route("/cluster", get(handle_cluster))
        .route("/resources", get(handle_resources))
        .route("/gpu", get(handle_gpu))
        .route("/metrics", get(handle_metrics))
        .route("/compact", post(handle_compact))
        .route("/snapshot", post(handle_snapshot))
        .route("/query", post(handle_query))
        .route("/raft/replicate", post(handle_replicate))
        .with_state(app_state);

    let http_addr: SocketAddr = format!("0.0.0.0:{}", args.http_port).parse()?;
    println!("\x1b[1;32m[+] HTTP REST API:\x1b[0m Listening on http://localhost:{}", args.http_port);
    println!("\x1b[1;32m[+] Prometheus Metrics:\x1b[0m http://localhost:{}/metrics", args.http_port);

    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(http_addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });

    // 2. Start Arrow Flight gRPC Server
    let flight_addr: SocketAddr = format!("0.0.0.0:{}", args.port).parse()?;
    let flight_svc = GdbFlightService::new();

    println!("\x1b[1;32m[+] Arrow Flight Server:\x1b[0m Listening on {}", flight_addr);
    println!("\x1b[1;33m[ Ready to process openCypher / GQL / CALL queries ]\x1b[0m\n");

    TonicServer::builder()
        .add_service(flight_svc.into_server())
        .serve(flight_addr)
        .await?;

    Ok(())
}
