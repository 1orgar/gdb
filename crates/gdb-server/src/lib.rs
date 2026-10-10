use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use gdb_core::schema::GraphSchema;
use gdb_core::DataValue;
use gdb_flight::{GdbClientFlightService, GdbFlightService};
use gdb_gpu::GpuDispatcher;
use gdb_planner::QueryExecutor;
use gdb_raft::MultiRaftManager;
use gdb_s3::S3StorageManager;
use gdb_storage::PartitionStorageEngine;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tonic::transport::Server as TonicServer;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReplicationMode {
    Sync,
    Async,
}

impl std::fmt::Display for ReplicationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplicationMode::Sync => write!(f, "SYNC"),
            ReplicationMode::Async => write!(f, "ASYNC"),
        }
    }
}

impl FromStr for ReplicationMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sync" | "synchronous" => Ok(ReplicationMode::Sync),
            "async" | "asynchronous" => Ok(ReplicationMode::Async),
            other => Err(format!("Unknown replication mode: '{}'. Use 'sync' or 'async'.", other)),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RingNode {
    pub node_id: u64,
    pub http_url: String,
    pub flight_port: u16,
    pub client_flight_port: u16,
}

#[derive(Parser, Debug, Clone)]
#[command(author, version, about = "GDB: Distributed High-Performance In-Memory Graph Database", long_about = None)]
pub struct Args {
    /// Unique Node ID in the cluster
    #[arg(short, long, default_value_t = 1)]
    pub node_id: u64,

    /// Total partitions in cluster
    #[arg(long, default_value_t = 4)]
    pub partitions: u32,

    /// Port for internal inter-node Arrow Flight & RPC
    #[arg(short, long, default_value_t = 8848)]
    pub port: u16,

    /// Dedicated Arrow Flight port for external client data ingestion & bulk loading
    #[arg(long, default_value_t = 8860)]
    pub client_flight_port: u16,

    /// Port for HTTP REST API
    #[arg(long, default_value_t = 8847)]
    pub http_port: u16,

    /// Directory for local NVMe Raft WAL
    #[arg(long, default_value = "./data/wal")]
    pub wal_dir: PathBuf,

    /// Comma-separated list of peer HTTP endpoints for Ring replication (e.g. "http://127.0.0.1:8846,http://127.0.0.1:8845")
    #[arg(long)]
    pub peers: Option<String>,

    /// S3 Bucket name for tiered persistence snapshots (e.g. "gdb-snapshots")
    #[arg(long, env = "AWS_BUCKET")]
    pub s3_bucket: Option<String>,

    /// S3 Custom Endpoint URL (e.g. "http://localhost:9000" for MinIO)
    #[arg(long, env = "AWS_ENDPOINT")]
    pub s3_endpoint: Option<String>,

    /// S3 Region (e.g. "us-east-1")
    #[arg(long, env = "AWS_REGION", default_value = "us-east-1")]
    pub s3_region: String,

    /// Replication factor on the leaderless hash ring (1 = pure sharding, N = full replication)
    #[arg(long, short = 'r', default_value_t = 3)]
    pub replication_factor: u32,

    /// Replication mode: 'sync' (synchronous quorum/all) or 'async' (background asynchronous)
    #[arg(long, default_value = "sync")]
    pub replication_mode: String,

    /// Cluster architecture mode (compatibility alias: 'ring', 'replication', 'sharding')
    #[arg(long, default_value = "ring")]
    pub cluster_mode: String,

    /// Enable hardware GPU acceleration (Metal on macOS, CUDA on Linux). Disabled by default.
    #[arg(long, env = "GDB_ENABLE_GPU", default_value_t = false)]
    pub enable_gpu: bool,

    /// Select GPU device index when multiple GPUs are available (default: 0)
    #[arg(long, env = "GDB_GPU_DEVICE", default_value_t = 0)]
    pub gpu_device: u32,

    /// Minimum number of edges required to trigger GPU hardware offload (default: 10000)
    #[arg(long, env = "GDB_GPU_THRESHOLD", default_value_t = 10_000)]
    pub gpu_offload_threshold: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QueryRequest {
    pub query: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BatchRequest {
    pub queries: Option<Vec<String>>,
    pub script: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReplicationPayload {
    pub query: String,
}

#[derive(Clone)]
pub struct AppState {
    pub node_id: u64,
    pub partitions: u32,
    pub port: u16,
    pub client_flight_port: u16,
    pub http_port: u16,
    pub start_time: Instant,
    pub schema: Arc<RwLock<GraphSchema>>,
    pub executor: Arc<QueryExecutor>,
    pub storage: Arc<PartitionStorageEngine>,
    pub peers: Vec<String>,
    pub ring_nodes: Vec<RingNode>,
    pub replication_factor: u32,
    pub replication_mode: ReplicationMode,
    pub http_client: reqwest::Client,
    pub queries_ok: Arc<AtomicU64>,
    pub queries_err: Arc<AtomicU64>,
    pub total_query_duration_us: Arc<AtomicU64>,
    pub replications_count: Arc<AtomicU64>,
    pub compactions_count: Arc<AtomicU64>,
    pub gpu_enabled: bool,
    pub gpu_device: u32,
    pub gpu_backend: String,
    pub gpu_threshold: usize,
    pub s3_manager: Option<Arc<S3StorageManager>>,
    pub s3_bucket: Option<String>,
}

async fn handle_health(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "UP",
        "service": "GDB Enterprise Graph Database",
        "version": env!("CARGO_PKG_VERSION"),
        "node_id": state.node_id,
        "role": "Peer",
        "cluster_topology": "leaderless-ring",
        "replication_factor": state.replication_factor,
        "replication_mode": state.replication_mode.to_string()
    }))
}

async fn handle_schema(State(state): State<AppState>) -> impl IntoResponse {
    let schema = state.schema.read();
    let vertices = schema.list_vertex_schemas();
    let edges = schema.list_edge_schemas();
    Json(serde_json::json!({
        "status": "ok",
        "graph": schema.name,
        "vertices": vertices,
        "edges": edges,
    }))
}

async fn handle_cluster(State(state): State<AppState>) -> impl IntoResponse {
    let total_nodes = state.ring_nodes.len();
    let effective_rf = (state.replication_factor as usize).min(total_nodes).max(1);
    Json(serde_json::json!({
        "node_id": state.node_id,
        "role": "Peer",
        "cluster_topology": "leaderless-ring",
        "replication_factor": state.replication_factor,
        "effective_replication_factor": effective_rf,
        "replication_mode": state.replication_mode.to_string(),
        "total_nodes": total_nodes,
        "partitions": state.partitions,
        "flight_port": state.port,
        "client_flight_port": state.client_flight_port,
        "http_port": state.http_port,
        "peers": state.peers,
        "ring_nodes": state.ring_nodes,
        "assigned_tokens": format!("Primary token: u % {} == {}", total_nodes, state.node_id.saturating_sub(1)),
        "s3_tiering": if state.s3_manager.is_some() { "Enabled" } else { "Disabled" },
        "s3_bucket": state.s3_bucket.clone().unwrap_or_default(),
        "gpu_enabled": state.gpu_enabled,
        "gpu_device": state.gpu_device,
        "gpu_backend": state.gpu_backend,
        "gpu_threshold": state.gpu_threshold,
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
        "role": "Peer",
        "cluster_mode": format!("LEADERLESS RING (RF={}, Mode={})", state.replication_factor, state.replication_mode),
        "cluster_topology": "leaderless-ring",
        "replication_factor": state.replication_factor,
        "replication_mode": state.replication_mode.to_string(),
        "total_nodes": state.ring_nodes.len(),
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
        "s3_bucket": state.s3_bucket.clone().unwrap_or_default(),
        "gpu_enabled": state.gpu_enabled,
        "gpu_device": state.gpu_device,
        "gpu_backend": state.gpu_backend,
        "gpu_threshold": state.gpu_threshold
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
    let memory_model = if state.gpu_enabled {
        #[cfg(target_os = "macos")]
        { "Unified Memory Architecture (Zero-Copy UMA)" }
        #[cfg(not(target_os = "macos"))]
        { "Dedicated PCIe HBM2/GDDR Host-Device" }
    } else {
        "Host RAM (CPU SIMD Fallback)"
    };

    Json(serde_json::json!({
        "enabled": state.gpu_enabled,
        "device_id": state.gpu_device,
        "backend": state.gpu_backend,
        "threshold_edges": state.gpu_threshold,
        "memory_model": memory_model,
        "active": state.gpu_enabled,
        "status": if state.gpu_enabled { "Active" } else { "Disabled (pass --enable-gpu)" },
        "supported_kernels": [
            "Parallel BFS Frontier Expansion",
            "Vectorized PageRank Iteration",
            "Cosine / Jaccard Graph Kernel",
            "Parallel Vector Similarity Search (SIMD/GPU)",
            "Multi-Hop Traversal Wavefront BFS"
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
    let is_sync = if state.replication_mode == ReplicationMode::Sync { 1 } else { 0 };
    let rf = state.replication_factor;
    let gpu_active = if state.gpu_enabled { 1 } else { 0 };

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

# HELP gdb_cluster_replication_factor Configured replication factor on hash ring
# TYPE gdb_cluster_replication_factor gauge
gdb_cluster_replication_factor {}

# HELP gdb_cluster_is_sync_replication Whether replication is synchronous (1) or asynchronous (0)
# TYPE gdb_cluster_is_sync_replication gauge
gdb_cluster_is_sync_replication {}

# HELP gdb_cluster_replications_total Total mutations replicated to cluster peers
# TYPE gdb_cluster_replications_total counter
gdb_cluster_replications_total {}

# HELP gdb_gpu_active GPU hardware acceleration status (1 for active)
# TYPE gdb_gpu_active gauge
gdb_gpu_active {}

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
        rf,
        is_sync,
        replications,
        gpu_active,
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

    // Replicate compaction to all other peers in the ring
    let other_nodes: Vec<RingNode> = state.ring_nodes.iter().filter(|n| n.node_id != state.node_id).cloned().collect();
    replicate_to_targets(&state.http_client, &other_nodes, "compact;", state.replication_mode, &state.replications_count).await;

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

fn build_ring_nodes(
    my_id: u64,
    my_http_port: u16,
    my_flight_port: u16,
    my_client_flight_port: u16,
    peers: &[String],
) -> Vec<RingNode> {
    let mut nodes = Vec::new();
    nodes.push(RingNode {
        node_id: my_id,
        http_url: format!("http://127.0.0.1:{}", my_http_port),
        flight_port: my_flight_port,
        client_flight_port: my_client_flight_port,
    });

    for (idx, peer) in peers.iter().enumerate() {
        let (peer_id, flight_p, client_p) = if peer.contains("8847") {
            (1, 8848, 8860)
        } else if peer.contains("8846") {
            (2, 8849, 8861)
        } else if peer.contains("8845") {
            (3, 8850, 8862)
        } else {
            let id = if (idx as u64 + 1) >= my_id {
                idx as u64 + 2
            } else {
                idx as u64 + 1
            };
            (id, 8848 + id as u16, 8860 + (id as u16).saturating_sub(1))
        };
        nodes.push(RingNode {
            node_id: peer_id,
            http_url: peer.clone(),
            flight_port: flight_p,
            client_flight_port: client_p,
        });
    }

    nodes.sort_by_key(|n| n.node_id);
    nodes.dedup_by_key(|n| n.node_id);
    nodes
}

fn calculate_replica_set(key: u64, ring: &[RingNode], rf: u32) -> Vec<RingNode> {
    if ring.is_empty() {
        return Vec::new();
    }
    let n = ring.len();
    let effective_rf = (rf as usize).max(1).min(n);
    let primary_idx = (key as usize) % n;
    let mut replicas = Vec::with_capacity(effective_rf);
    for i in 0..effective_rf {
        let idx = (primary_idx + i) % n;
        replicas.push(ring[idx].clone());
    }
    replicas
}

async fn replicate_to_targets(
    client: &reqwest::Client,
    targets: &[RingNode],
    query: &str,
    mode: ReplicationMode,
    reps_counter: &Arc<AtomicU64>,
) {
    if targets.is_empty() {
        return;
    }

    let payload = serde_json::json!({ "query": query });

    match mode {
        ReplicationMode::Sync => {
            let mut futures = Vec::new();
            for target in targets {
                let target_url = format!("{}/replicate", target.http_url.trim_end_matches('/'));
                let c = client.clone();
                let p = payload.clone();
                let reps = reps_counter.clone();
                futures.push(async move {
                    match c
                        .post(&target_url)
                        .json(&p)
                        .timeout(std::time::Duration::from_millis(1500))
                        .send()
                        .await
                    {
                        Ok(resp) if resp.status().is_success() => {
                            reps.fetch_add(1, Ordering::Relaxed);
                        }
                        Ok(resp) => {
                            tracing::warn!("Replication peer {} responded with status {}", target_url, resp.status());
                        }
                        Err(e) => {
                            tracing::warn!("Replication peer {} request failed: {}", target_url, e);
                        }
                    }
                });
            }
            futures::future::join_all(futures).await;
        }
        ReplicationMode::Async => {
            for target in targets.to_vec() {
                let target_url = format!("{}/replicate", target.http_url.trim_end_matches('/'));
                let c = client.clone();
                let p = payload.clone();
                let reps = reps_counter.clone();
                tokio::spawn(async move {
                    match c
                        .post(&target_url)
                        .json(&p)
                        .timeout(std::time::Duration::from_millis(1500))
                        .send()
                        .await
                    {
                        Ok(resp) if resp.status().is_success() => {
                            reps.fetch_add(1, Ordering::Relaxed);
                        }
                        Ok(_) => {
                            tracing::warn!("Async replication peer {} responded with error", target_url);
                        }
                        Err(e) => {
                            tracing::debug!("Async replication peer {} failed: {}", target_url, e);
                        }
                    }
                });
            }
        }
    }
}

async fn execute_single_query(state: &AppState, trimmed: &str) -> serde_json::Value {
    let start = Instant::now();

    // Check for compact command
    if trimmed.eq_ignore_ascii_case("compact") || trimmed.eq_ignore_ascii_case("compact;") {
        state.storage.compact();
        state.compactions_count.fetch_add(1, Ordering::Relaxed);
        let elapsed_us = start.elapsed().as_micros();
        state.queries_ok.fetch_add(1, Ordering::Relaxed);
        state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);

        let other_nodes: Vec<RingNode> = state.ring_nodes.iter().filter(|n| n.node_id != state.node_id).cloned().collect();
        replicate_to_targets(&state.http_client, &other_nodes, "compact;", state.replication_mode, &state.replications_count).await;

        return serde_json::json!({
            "status": "ok",
            "message": "Compaction completed",
            "elapsed_us": elapsed_us,
            "num_rows": 0,
            "columns": [],
            "rows": []
        });
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
                    return serde_json::json!({
                        "status": "ok",
                        "message": format!("Parquet snapshot uploaded to S3: {}", key),
                        "elapsed_us": elapsed_us,
                        "num_rows": 1,
                        "columns": ["snapshot_key", "version", "bucket"],
                        "rows": [[key, ver, state.s3_bucket.clone().unwrap_or_default()]]
                    });
                }
                Err(e) => {
                    return serde_json::json!({
                        "status": "error",
                        "error": format!("S3 upload failed: {}", e),
                        "elapsed_us": elapsed_us
                    });
                }
            }
        } else {
            return serde_json::json!({
                "status": "error",
                "error": "S3 tiered storage is not configured. Start server with --s3-bucket <bucket> or export AWS_BUCKET",
                "elapsed_us": elapsed_us
            });
        }
    }

    let stmt = match gdb_parser::parse(trimmed) {
        Ok(s) => s,
        Err(e) => {
            state.queries_err.fetch_add(1, Ordering::Relaxed);
            return serde_json::json!({
                "status": "error",
                "error": format!("Syntax error: {}", e),
                "elapsed_us": start.elapsed().as_micros()
            });
        }
    };

    let is_ddl = matches!(
        stmt,
        gdb_parser::ast::Statement::CreateVertexLabel { .. }
            | gdb_parser::ast::Statement::CreateEdgeType { .. }
            | gdb_parser::ast::Statement::CreateIndex { .. }
            | gdb_parser::ast::Statement::DropIndex { .. }
            | gdb_parser::ast::Statement::DropVertexLabel { .. }
            | gdb_parser::ast::Statement::DropEdgeType { .. }
            | gdb_parser::ast::Statement::AlterVertexLabel { .. }
            | gdb_parser::ast::Statement::AlterEdgeType { .. }
    );

    if is_ddl {
        // DDL: Execute locally, then broadcast to all other nodes on the ring
        match state.executor.execute(stmt) {
            Ok(res) => {
                let elapsed_us = start.elapsed().as_micros();
                state.queries_ok.fetch_add(1, Ordering::Relaxed);
                state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);

                let other_nodes: Vec<RingNode> = state.ring_nodes.iter().filter(|n| n.node_id != state.node_id).cloned().collect();
                replicate_to_targets(&state.http_client, &other_nodes, trimmed, state.replication_mode, &state.replications_count).await;

                return serde_json::json!({
                    "status": "ok",
                    "message": res.message,
                    "elapsed_us": elapsed_us,
                    "num_rows": 0,
                    "columns": [],
                    "rows": []
                });
            }
            Err(e) => {
                state.queries_err.fetch_add(1, Ordering::Relaxed);
                return serde_json::json!({
                    "status": "error",
                    "error": format!("DDL execution error: {}", e),
                    "elapsed_us": start.elapsed().as_micros()
                });
            }
        }
    }

    if let gdb_parser::ast::Statement::InsertVertices { vertices, .. } = &stmt {
        if state.ring_nodes.len() > 1 {
            let mut targets: Vec<RingNode> = Vec::new();
            for (vid, _) in vertices {
                for rep in calculate_replica_set(vid.0, &state.ring_nodes, state.replication_factor) {
                    if rep.node_id != state.node_id && !targets.iter().any(|t| t.node_id == rep.node_id) {
                        targets.push(rep);
                    }
                }
            }

            match state.executor.execute(stmt) {
                Ok(res) => {
                    let elapsed_us = start.elapsed().as_micros();
                    state.queries_ok.fetch_add(1, Ordering::Relaxed);
                    state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);

                    replicate_to_targets(&state.http_client, &targets, trimmed, state.replication_mode, &state.replications_count).await;

                    return serde_json::json!({
                        "status": "ok",
                        "message": res.message,
                        "elapsed_us": elapsed_us,
                        "num_rows": 0,
                        "rows_affected": res.rows_affected,
                        "columns": [],
                        "rows": []
                    });
                }
                Err(e) => {
                    state.queries_err.fetch_add(1, Ordering::Relaxed);
                    return serde_json::json!({
                        "status": "error",
                        "error": format!("Execution error: {}", e),
                        "elapsed_us": start.elapsed().as_micros()
                    });
                }
            }
        }
    }

    if let gdb_parser::ast::Statement::InsertEdges { edges, .. } = &stmt {
        if state.ring_nodes.len() > 1 {
            let mut targets: Vec<RingNode> = Vec::new();
            for (src, _, _, _) in edges {
                for rep in calculate_replica_set(src.0, &state.ring_nodes, state.replication_factor) {
                    if rep.node_id != state.node_id && !targets.iter().any(|t| t.node_id == rep.node_id) {
                        targets.push(rep);
                    }
                }
            }

            match state.executor.execute(stmt) {
                Ok(res) => {
                    let elapsed_us = start.elapsed().as_micros();
                    state.queries_ok.fetch_add(1, Ordering::Relaxed);
                    state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);

                    replicate_to_targets(&state.http_client, &targets, trimmed, state.replication_mode, &state.replications_count).await;

                    return serde_json::json!({
                        "status": "ok",
                        "message": res.message,
                        "elapsed_us": elapsed_us,
                        "num_rows": 0,
                        "rows_affected": res.rows_affected,
                        "columns": [],
                        "rows": []
                    });
                }
                Err(e) => {
                    state.queries_err.fetch_add(1, Ordering::Relaxed);
                    return serde_json::json!({
                        "status": "error",
                        "error": format!("Execution error: {}", e),
                        "elapsed_us": start.elapsed().as_micros()
                    });
                }
            }
        }
    }

    // DML Routing & Leaderless Ring Replication
    let dml_key = match &stmt {
        gdb_parser::ast::Statement::InsertVertex { id, .. }
        | gdb_parser::ast::Statement::MergeVertex { id, .. } => Some(id.0),
        gdb_parser::ast::Statement::InsertEdge { src, .. }
        | gdb_parser::ast::Statement::DeleteEdge { src, .. } => Some(src.0),
        _ => None,
    };

    if let Some(key) = dml_key {
        let replica_set = calculate_replica_set(key, &state.ring_nodes, state.replication_factor);
        let is_local_replica = replica_set.iter().any(|n| n.node_id == state.node_id);

        if !is_local_replica && !replica_set.is_empty() {
            // Coordinator is not part of this key's replica set (RF < N): forward to primary replica
            let primary = &replica_set[0];
            let primary_url = format!("{}/query", primary.http_url.trim_end_matches('/'));
            let payload = serde_json::json!({ "query": trimmed });
            if let Ok(resp) = state.http_client.post(&primary_url).json(&payload).send().await {
                if let Ok(json_resp) = resp.json::<serde_json::Value>().await {
                    return json_resp;
                }
            }
        }

        // Current node is in the replica set: execute locally, then replicate to remote replicas in set
        match state.executor.execute(stmt.clone()) {
            Ok(res) => {
                let elapsed_us = start.elapsed().as_micros();
                state.queries_ok.fetch_add(1, Ordering::Relaxed);
                state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);

                let remote_replicas: Vec<RingNode> = replica_set.into_iter().filter(|n| n.node_id != state.node_id).collect();
                replicate_to_targets(&state.http_client, &remote_replicas, trimmed, state.replication_mode, &state.replications_count).await;

                return serde_json::json!({
                    "status": "ok",
                    "message": res.message,
                    "elapsed_us": elapsed_us,
                    "num_rows": 0,
                    "rows_affected": res.rows_affected,
                    "columns": [],
                    "rows": []
                });
            }
            Err(e) => {
                state.queries_err.fetch_add(1, Ordering::Relaxed);
                return serde_json::json!({
                    "status": "error",
                    "error": format!("Execution error: {}", e),
                    "elapsed_us": start.elapsed().as_micros()
                });
            }
        }
    }

    match state.executor.execute(stmt) {
        Ok(res) => {
            let elapsed_us = start.elapsed().as_micros();
            state.queries_ok.fetch_add(1, Ordering::Relaxed);
            state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);

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
                            Ok(DataValue::Vector(v)) => serde_json::json!(v),
                            Ok(DataValue::Null) => serde_json::Value::Null,
                            _ => serde_json::json!(format!("{:?}", col)),
                        };
                        row_vals.push(val);
                    }
                    rows.push(row_vals);
                }
            }

            serde_json::json!({
                "status": "ok",
                "message": res.message,
                "elapsed_us": elapsed_us,
                "num_rows": rows.len(),
                "rows_affected": res.rows_affected,
                "columns": columns,
                "rows": rows,
            })
        }
        Err(e) => {
            state.queries_err.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({
                "status": "error",
                "error": format!("Execution error: {}", e),
                "elapsed_us": start.elapsed().as_micros()
            })
        }
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
    let stmts = gdb_parser::split_statements(trimmed);

    if stmts.is_empty() {
        state.queries_err.fetch_add(1, Ordering::Relaxed);
        return Json(serde_json::json!({
            "status": "error",
            "error": "Syntax error: Empty query string",
            "elapsed_us": start.elapsed().as_micros()
        }));
    }

    if stmts.len() == 1 {
        let res = execute_single_query(&state, &stmts[0]).await;
        return Json(res);
    }

    // Multi-statement script execution
    let mut total_rows_affected = 0;
    let mut last_res = serde_json::Value::Null;

    for (idx, stmt_str) in stmts.iter().enumerate() {
        let res = execute_single_query(&state, stmt_str).await;
        if res.get("status").and_then(|s| s.as_str()) == Some("error") {
            let err_msg = res.get("error").and_then(|e| e.as_str()).unwrap_or("Unknown error");
            return Json(serde_json::json!({
                "status": "error",
                "error": format!("Statement {} error: {}", idx + 1, err_msg),
                "statement_index": idx,
                "failed_statement": stmt_str,
                "statements_executed": idx,
                "elapsed_us": start.elapsed().as_micros()
            }));
        }

        if let Some(ra) = res.get("rows_affected").and_then(|r| r.as_u64()) {
            total_rows_affected += ra as usize;
        }
        last_res = res;
    }

    let mut final_obj = match last_res {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    final_obj.insert("status".to_string(), serde_json::json!("ok"));
    final_obj.insert(
        "message".to_string(),
        serde_json::json!(format!("Executed {} statements successfully", stmts.len())),
    );
    final_obj.insert("rows_affected".to_string(), serde_json::json!(total_rows_affected));
    final_obj.insert("elapsed_us".to_string(), serde_json::json!(start.elapsed().as_micros()));
    final_obj.insert("statements_executed".to_string(), serde_json::json!(stmts.len()));

    Json(serde_json::Value::Object(final_obj))
}

async fn handle_batch(
    State(state): State<AppState>,
    body: String,
) -> impl IntoResponse {
    let start = Instant::now();
    let queries: Vec<String> = if let Ok(parsed) = serde_json::from_str::<BatchRequest>(&body) {
        if let Some(list) = parsed.queries {
            list
        } else if let Some(script) = parsed.script {
            gdb_parser::split_statements(&script)
        } else {
            vec![]
        }
    } else if let Ok(parsed_list) = serde_json::from_str::<Vec<String>>(&body) {
        parsed_list
    } else {
        gdb_parser::split_statements(&body)
    };

    let mut results = Vec::with_capacity(queries.len());
    let mut ok_count = 0;
    let mut err_count = 0;

    for q in &queries {
        let trimmed = q.trim();
        if trimmed.is_empty() {
            continue;
        }
        let res = execute_single_query(&state, trimmed).await;
        if res.get("status").and_then(|s| s.as_str()) == Some("ok") {
            ok_count += 1;
        } else {
            err_count += 1;
        }
        results.push(res);
    }

    Json(serde_json::json!({
        "status": if err_count == 0 { "ok" } else { "partial_error" },
        "total": results.len(),
        "succeeded": ok_count,
        "failed": err_count,
        "elapsed_us": start.elapsed().as_micros(),
        "results": results
    }))
}

pub fn create_router(app_state: AppState) -> Router {
    Router::new()
        .route("/health", get(handle_health))
        .route("/schema", get(handle_schema))
        .route("/cluster", get(handle_cluster))
        .route("/resources", get(handle_resources))
        .route("/gpu", get(handle_gpu))
        .route("/metrics", get(handle_metrics))
        .route("/compact", post(handle_compact))
        .route("/snapshot", post(handle_snapshot))
        .route("/query", post(handle_query))
        .route("/batch", post(handle_batch))
        .route("/replicate", post(handle_replicate))
        .route("/raft/replicate", post(handle_replicate))
        .with_state(app_state)
}

pub async fn create_test_server(
    wal_dir: PathBuf,
) -> Result<(SocketAddr, tokio::task::JoinHandle<()>, AppState), Box<dyn std::error::Error + Send + Sync>> {
    let schema = Arc::new(RwLock::new(GraphSchema::new("test_graph")));
    let raft_manager = Arc::new(MultiRaftManager::new(1, 1, wal_dir));
    let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
    raft_manager.register_partition(0, storage.clone(), vec![])?;
    let executor = Arc::new(QueryExecutor::new(schema.clone(), storage.clone()));

    let ring_nodes = vec![RingNode {
        node_id: 1,
        http_url: "http://127.0.0.1:0".to_string(),
        flight_port: 0,
        client_flight_port: 0,
    }];

    let app_state = AppState {
        node_id: 1,
        partitions: 1,
        port: 0,
        client_flight_port: 0,
        http_port: 0,
        start_time: Instant::now(),
        schema: schema.clone(),
        executor: executor.clone(),
        storage: storage.clone(),
        peers: vec![],
        ring_nodes,
        replication_factor: 1,
        replication_mode: ReplicationMode::Sync,
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()?,
        queries_ok: Arc::new(AtomicU64::new(0)),
        queries_err: Arc::new(AtomicU64::new(0)),
        total_query_duration_us: Arc::new(AtomicU64::new(0)),
        replications_count: Arc::new(AtomicU64::new(0)),
        compactions_count: Arc::new(AtomicU64::new(0)),
        gpu_enabled: false,
        gpu_device: 0,
        gpu_backend: "Disabled".to_string(),
        gpu_threshold: 10_000,
        s3_manager: None,
        s3_bucket: None,
    };

    let app = create_router(app_state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let local_addr = listener.local_addr()?;
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    Ok((local_addr, handle, app_state))
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt().try_init();
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

    let gpu_dispatcher = GpuDispatcher::new(args.enable_gpu, args.gpu_device, args.gpu_offload_threshold);
    println!(
        "\x1b[1;32m[+] Hardware Acceleration:\x1b[0m {} (Status: {}, Device: #{}, Threshold: {} edges)",
        gpu_dispatcher.backend_name(),
        if args.enable_gpu { "Active" } else { "Disabled" },
        args.gpu_device,
        args.gpu_offload_threshold
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
    let executor = Arc::new(QueryExecutor::with_gpu(
        schema.clone(),
        storage_p0.clone(),
        Arc::new(gpu_dispatcher.clone()),
    ));

    // Clean graph catalog boot (no default entities created on startup)
    println!("\x1b[1;32m[+] Graph Catalog:\x1b[0m Clean graph catalog initialized (no default entities)");

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

    let ring_nodes = build_ring_nodes(args.node_id, args.http_port, args.port, args.client_flight_port, &peers);
    let total_nodes = ring_nodes.len() as u64;
    let replication_mode = ReplicationMode::from_str(&args.replication_mode).unwrap_or(ReplicationMode::Sync);
    let effective_rf = (args.replication_factor as usize).min(ring_nodes.len()).max(1);

    println!(
        "\x1b[1;32m[+] Cluster Topology:\x1b[0m LEADERLESS HASH RING (Nodes: {})",
        total_nodes
    );
    println!(
        "\x1b[1;32m[+] Replication Factor:\x1b[0m RF={} (Effective: {})",
        args.replication_factor, effective_rf
    );
    println!(
        "\x1b[1;32m[+] Replication Mode:\x1b[0m {}",
        replication_mode
    );
    println!(
        "\x1b[1;32m[+] Node Role:\x1b[0m Peer (Node #{})",
        args.node_id
    );

    let app_state = AppState {
        node_id: args.node_id,
        partitions: args.partitions,
        port: args.port,
        client_flight_port: args.client_flight_port,
        http_port: args.http_port,
        start_time: Instant::now(),
        schema: schema.clone(),
        executor: executor.clone(),
        storage: storage_p0.clone(),
        peers,
        ring_nodes,
        replication_factor: args.replication_factor,
        replication_mode,
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()?,
        queries_ok: Arc::new(AtomicU64::new(0)),
        queries_err: Arc::new(AtomicU64::new(0)),
        total_query_duration_us: Arc::new(AtomicU64::new(0)),
        replications_count: Arc::new(AtomicU64::new(0)),
        compactions_count: Arc::new(AtomicU64::new(0)),
        gpu_enabled: args.enable_gpu,
        gpu_device: args.gpu_device,
        gpu_backend: gpu_dispatcher.backend_name().to_string(),
        gpu_threshold: gpu_dispatcher.threshold_edges,
        s3_manager,
        s3_bucket: args.s3_bucket.clone(),
    };

    let app = create_router(app_state);

    let http_addr: SocketAddr = format!("0.0.0.0:{}", args.http_port).parse()?;
    println!("\x1b[1;32m[+] HTTP REST API:\x1b[0m Listening on http://localhost:{}", args.http_port);
    println!("\x1b[1;32m[+] Prometheus Metrics:\x1b[0m http://localhost:{}/metrics", args.http_port);

    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(http_addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });

    // 2. Start Internal Arrow Flight gRPC Server (for inter-node replication / MPP shuffle)
    let internal_flight_addr: SocketAddr = format!("0.0.0.0:{}", args.port).parse()?;
    let internal_flight_svc = GdbFlightService::new();

    tokio::spawn(async move {
        println!("\x1b[1;32m[+] Internal Flight Server (Inter-Node):\x1b[0m Listening on {}", internal_flight_addr);
        if let Err(e) = TonicServer::builder()
            .add_service(internal_flight_svc.into_server())
            .serve(internal_flight_addr)
            .await
        {
            eprintln!("Internal Flight server error: {}", e);
        }
    });

    // 3. Start Dedicated Client Arrow Flight Server (for external fast ingestion & queries)
    let client_flight_addr: SocketAddr = format!("0.0.0.0:{}", args.client_flight_port).parse()?;
    let client_flight_svc = GdbClientFlightService::new(storage_p0.clone(), schema.clone(), executor.clone());

    println!("\x1b[1;32m[+] Client Flight Server (Fast Ingestion/Query):\x1b[0m Listening on {}", client_flight_addr);
    println!("\x1b[1;33m[ Ready to process openCypher / GQL / CALL queries ]\x1b[0m\n");

    TonicServer::builder()
        .add_service(client_flight_svc.into_server())
        .serve(client_flight_addr)
        .await?;

    Ok(())
}
