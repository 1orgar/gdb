pub mod cluster;
pub mod handlers;
pub mod state;

pub use cluster::*;
pub use handlers::*;
pub use state::*;

use axum::routing::{get, post};
use axum::Router;
use clap::Parser;
use gdb_core::schema::GraphSchema;
use gdb_flight::{GdbClientFlightService, GdbFlightService};
use gdb_gpu::GpuDispatcher;
use gdb_planner::QueryExecutor;
use gdb_raft::MultiRaftManager;
use gdb_s3::S3StorageManager;
use gdb_storage::PartitionStorageEngine;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Instant;
use tonic::transport::Server as TonicServer;

pub fn create_router(app_state: AppState) -> Router {
    Router::new()
        .route("/health", get(handle_health))
        .route("/schema", get(handle_schema))
        .route("/graphs", get(handle_graphs))
        .route("/pregel/status", get(handle_pregel_status))
        .route("/cluster", get(handle_cluster))
        .route("/cluster/join", post(handle_cluster_join))
        .route("/resources", get(handle_resources))
        .route("/gpu", get(handle_gpu))
        .route("/metrics", get(handle_metrics))
        .route("/compact", post(handle_compact))
        .route("/snapshot", post(handle_snapshot))
        .route("/query", post(handle_query))
        .route("/batch", post(handle_batch))
        .route("/storage/out_edges", post(handle_storage_out_edges))
        .route("/storage/all_edges", get(handle_storage_all_edges))
        .route("/storage/all_edges_bin", get(handle_storage_all_edges_bin))
        .route("/storage/vertex_properties", post(handle_storage_vertex_properties))
        .route("/replicate", post(handle_replicate))
        .route("/raft/replicate", post(handle_replicate))
        .with_state(app_state)
}

pub async fn create_test_server(
    wal_dir: PathBuf,
) -> Result<(SocketAddr, tokio::task::JoinHandle<()>, AppState), Box<dyn std::error::Error + Send + Sync>> {
    let schema = Arc::new(RwLock::new(GraphSchema::new("default")));
    let raft_manager = Arc::new(MultiRaftManager::new(1, 1, wal_dir));
    let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
    raft_manager.register_partition(0, storage.clone(), vec![])?;

    let ring_nodes = Arc::new(RwLock::new(vec![RingNode {
        node_id: 1,
        http_url: "http://127.0.0.1:0".to_string(),
        flight_address: "127.0.0.1".to_string(),
        flight_port: 0,
        client_flight_address: "127.0.0.1".to_string(),
        client_flight_port: 0,
    }]));

    let remote_expander = Arc::new(ClusterEdgeExpander {
        ring_nodes: ring_nodes.clone(),
        node_id: 1,
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(2000))
            .build()?,
        replication_factor: 1,
        cache: Arc::new(RwLock::new(HashMap::new())),
    });

    let executor = Arc::new(
        QueryExecutor::new(schema.clone(), storage.clone())
            .with_remote_expander(remote_expander.clone())
            .with_remote_property_resolver(remote_expander),
    );

    let global_csr_cache = Arc::new(RwLock::new(None));
    let default_space = Arc::new(GraphSpace {
        name: "default".to_string(),
        schema: schema.clone(),
        storage: storage.clone(),
        executor: executor.clone(),
        global_csr_cache: global_csr_cache.clone(),
    });
    let mut initial_spaces = HashMap::new();
    initial_spaces.insert("default".to_string(), default_space);

    let app_state = AppState {
        node_id: 1,
        partitions: 1,
        flight_address: "127.0.0.1".to_string(),
        port: 0,
        client_flight_address: "127.0.0.1".to_string(),
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
        gpu_max_vram_mb: 2048,
        s3_manager: None,
        s3_bucket: None,
        global_csr_cache,
        spaces: Arc::new(RwLock::new(initial_spaces)),
        active_space: Arc::new(RwLock::new("default".to_string())),
        pregel_status: Arc::new(RwLock::new(PregelTelemetry::default())),
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
    run_server_with_shutdown(args, async {
        let _ = tokio::signal::ctrl_c().await;
    }).await
}

pub async fn run_server_with_shutdown<F>(args: Args, shutdown: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let _ = tracing_subscriber::fmt().try_init();

    println!("\x1b[1;36m");
    println!("  ██████╗ ██████╗ ██████╗ ");
    println!(" ██╔════╝ ██╔══██╗██╔══██╗");
    println!(" ██║  ███╗██║  ██║██████╔╝");
    println!(" ██║   ██║██║  ██║██╔══██╗");
    println!(" ╚██████╔╝██████╔╝██████╔╝");
    println!("  ╚═════╝ ╚═════╝ ╚═════╝ ");
    println!(" Distributed HTAP In-Memory Graph Database (Nebula Alternative)");
    println!("\x1b[0m");

    let max_vram_bytes = args.gpu_max_vram_mb * 1024 * 1024;
    let gpu_dispatcher = GpuDispatcher::new_with_vram(
        args.enable_gpu,
        args.gpu_device,
        args.gpu_offload_threshold,
        max_vram_bytes,
    );
    println!(
        "\x1b[1;32m[+] Hardware Acceleration:\x1b[0m {} (Status: {}, Device: #{}, Threshold: {} edges, VRAM Limit: {} MB)",
        gpu_dispatcher.backend_name(),
        if args.enable_gpu { "Active" } else { "Disabled" },
        args.gpu_device,
        args.gpu_offload_threshold,
        args.gpu_max_vram_mb
    );
    println!(
        "\x1b[1;32m[+] Node ID:\x1b[0m {} | \x1b[1;32mPartitions:\x1b[0m {}",
        args.node_id, args.partitions
    );

    let detected_ip = detect_local_address();
    let flight_address = args.flight_address.unwrap_or_else(|| detected_ip.clone());
    let client_flight_address = args.client_flight_address.unwrap_or_else(|| flight_address.clone());

    println!(
        "\x1b[1;32m[+] Arrow Flight Internal Address:\x1b[0m {}:{}",
        flight_address, args.port
    );
    println!(
        "\x1b[1;32m[+] Client Flight Public Address:\x1b[0m {}:{}",
        client_flight_address, args.client_flight_port
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

    let ring_nodes_vec = build_ring_nodes(
        args.node_id,
        args.http_port,
        &flight_address,
        args.port,
        &client_flight_address,
        args.client_flight_port,
        &peers,
    );
    let my_ring_node = ring_nodes_vec.iter().find(|n| n.node_id == args.node_id).cloned().unwrap();
    let total_nodes = ring_nodes_vec.len() as u64;
    let ring_nodes = Arc::new(RwLock::new(ring_nodes_vec));
    let replication_mode = ReplicationMode::from_str(&args.replication_mode).unwrap_or(ReplicationMode::Sync);
    let effective_rf = (args.replication_factor as usize).min(total_nodes as usize).max(1);

    let remote_expander = Arc::new(ClusterEdgeExpander {
        ring_nodes: ring_nodes.clone(),
        node_id: args.node_id,
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(2000))
            .build()?,
        replication_factor: args.replication_factor,
        cache: Arc::new(RwLock::new(HashMap::new())),
    });

    let executor = Arc::new(
        QueryExecutor::with_gpu(
            schema.clone(),
            storage_p0.clone(),
            Arc::new(gpu_dispatcher.clone()),
        )
        .with_remote_expander(remote_expander.clone())
        .with_remote_property_resolver(remote_expander),
    );

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

    let global_csr_cache = Arc::new(RwLock::new(None));
    let default_space = Arc::new(GraphSpace {
        name: "default".to_string(),
        schema: schema.clone(),
        storage: storage_p0.clone(),
        executor: executor.clone(),
        global_csr_cache: global_csr_cache.clone(),
    });
    let mut initial_spaces = HashMap::new();
    initial_spaces.insert("default".to_string(), default_space);
    let spaces_arc = Arc::new(RwLock::new(initial_spaces));

    let app_state = AppState {
        node_id: args.node_id,
        partitions: args.partitions,
        flight_address: flight_address.clone(),
        port: args.port,
        client_flight_address: client_flight_address.clone(),
        client_flight_port: args.client_flight_port,
        http_port: args.http_port,
        start_time: Instant::now(),
        schema: schema.clone(),
        executor: executor.clone(),
        storage: storage_p0.clone(),
        peers: peers.clone(),
        ring_nodes: ring_nodes.clone(),
        replication_factor: args.replication_factor,
        replication_mode,
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
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
        gpu_max_vram_mb: args.gpu_max_vram_mb,
        s3_manager,
        s3_bucket: args.s3_bucket.clone(),
        global_csr_cache,
        spaces: spaces_arc.clone(),
        active_space: Arc::new(RwLock::new("default".to_string())),
        pregel_status: Arc::new(RwLock::new(PregelTelemetry::default())),
    };

    if !peers.is_empty() {
        let join_client = app_state.http_client.clone();
        let my_node = my_ring_node.clone();
        let ring_arc = ring_nodes.clone();
        let peers_list = peers.clone();

        tokio::spawn(async move {
            for peer in &peers_list {
                let join_url = format!("{}/cluster/join", peer.trim_end_matches('/'));
                match join_client
                    .post(&join_url)
                    .json(&my_node)
                    .timeout(std::time::Duration::from_millis(3000))
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        if let Ok(body) = resp.json::<serde_json::Value>().await {
                            if let Some(nodes_arr) = body.get("ring_nodes").and_then(|v| v.as_array()) {
                                let mut ring = ring_arc.write();
                                for item in nodes_arr {
                                    if let Ok(rn) = serde_json::from_value::<RingNode>(item.clone()) {
                                        if let Some(existing) = ring.iter_mut().find(|e| e.node_id == rn.node_id) {
                                            *existing = rn;
                                        } else {
                                            ring.push(rn);
                                        }
                                    }
                                }
                                ring.sort_by_key(|n| n.node_id);
                            }
                        }
                        println!("\x1b[1;32m[+] Cluster Handshake:\x1b[0m Announced self to {}", join_url);
                    }
                    Ok(resp) => {
                        tracing::warn!("Cluster join handshake to {} returned status {}", join_url, resp.status());
                    }
                    Err(e) => {
                        tracing::debug!("Cluster join handshake to {} failed: {}", join_url, e);
                    }
                }
            }
        });
    }

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
    let resolver_spaces = spaces_arc.clone();
    let space_resolver = Arc::new(move |graph_name: &str| {
        let norm = AppState::normalize_graph_name(graph_name);
        resolver_spaces.read().get(&norm).map(|sp| {
            (sp.storage.clone(), sp.schema.clone(), sp.executor.clone())
        })
    });
    let client_flight_svc = GdbClientFlightService::new(storage_p0.clone(), schema.clone(), executor.clone())
        .with_space_resolver(space_resolver);

    println!("\x1b[1;32m[+] Client Flight Server (Fast Ingestion/Query):\x1b[0m Listening on {}", client_flight_addr);
    println!("\x1b[1;33m[ Ready to process openCypher / GQL / CALL queries ]\x1b[0m\n");

    TonicServer::builder()
        .add_service(client_flight_svc.into_server())
        .serve_with_shutdown(client_flight_addr, shutdown)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_test_server_and_routes() {
        let tmp = tempfile::tempdir().unwrap();
        let (addr, handle, _state) = create_test_server(tmp.path().to_path_buf()).await.unwrap();
        let client = reqwest::Client::new();
        let base = format!("http://{}", addr);

        // 1. Health
        let res = client.get(format!("{}/health", base)).send().await.unwrap();
        assert_eq!(res.status(), 200);

        // 2. Schema
        let res = client.get(format!("{}/schema", base)).send().await.unwrap();
        assert_eq!(res.status(), 200);

        // 3. Cluster
        let res = client.get(format!("{}/cluster", base)).send().await.unwrap();
        assert_eq!(res.status(), 200);

        // 4. Resources
        let res = client.get(format!("{}/resources?local=true", base)).send().await.unwrap();
        assert_eq!(res.status(), 200);

        // 5. GPU
        let res = client.get(format!("{}/gpu", base)).send().await.unwrap();
        assert_eq!(res.status(), 200);

        // 6. Metrics
        let res = client.get(format!("{}/metrics", base)).send().await.unwrap();
        assert_eq!(res.status(), 200);

        // 7. Cluster Join
        let join_payload = serde_json::json!({
            "node_id": 99,
            "http_url": "http://127.0.0.1:9999",
            "flight_address": "127.0.0.1",
            "flight_port": 9998,
            "client_flight_address": "127.0.0.1",
            "client_flight_port": 9997
        });
        let res = client.post(format!("{}/cluster/join", base)).json(&join_payload).send().await.unwrap();
        assert_eq!(res.status(), 200);

        // 8. Compact
        let res = client.post(format!("{}/compact", base)).send().await.unwrap();
        assert_eq!(res.status(), 200);

        // 9. Query & Batch
        let q_res = client.post(format!("{}/query", base))
            .json(&serde_json::json!({ "query": "SHOW SCHEMA;", "local_only": true }))
            .send().await.unwrap();
        assert_eq!(q_res.status(), 200);

        let b_res = client.post(format!("{}/batch", base))
            .json(&serde_json::json!({ "queries": ["SHOW SCHEMA;"], "local_only": true }))
            .send().await.unwrap();
        assert_eq!(b_res.status(), 200);

        // 10. v0.6.0: Graph Spaces isolation, HNSW Vector Index, and Pregel Telemetry
        let c_res = client.post(format!("{}/query", base))
            .json(&serde_json::json!({ "query": "CREATE GRAPH IF NOT EXISTS fraud_space;", "local_only": true }))
            .send().await.unwrap();
        assert_eq!(c_res.status(), 200);

        let ddl_res = client.post(format!("{}/query", base))
            .json(&serde_json::json!({
                "query": "CREATE VERTEX Doc (title STRING, emb VECTOR(4)); CREATE VECTOR INDEX ON Doc(emb); CREATE EDGE CITES ();",
                "graph": "fraud_space",
                "local_only": true
            }))
            .send().await.unwrap();
        assert_eq!(ddl_res.status(), 200);

        let ins_res = client.post(format!("{}/query", base))
            .json(&serde_json::json!({
                "query": "INSERT VERTEX Doc (id, title, emb) VALUES (1, 'A', [1.0, 0.0, 0.0, 0.0]), (2, 'B', [0.9, 0.1, 0.0, 0.0]), (3, 'C', [0.0, 1.0, 0.0, 0.0]); INSERT EDGE CITES FROM 1 TO 2; INSERT EDGE CITES FROM 2 TO 3;",
                "graph": "fraud_space",
                "local_only": true
            }))
            .send().await.unwrap();
        assert_eq!(ins_res.status(), 200);

        // Verify HNSW similarity search inside fraud_space
        let vec_res = client.post(format!("{}/query", base))
            .json(&serde_json::json!({
                "query": "CALL vector.similaritySearch('Doc', 'emb', [1.0, 0.0, 0.0, 0.0], 2, 'cosine') YIELD vertex_id, score;",
                "graph": "fraud_space",
                "local_only": true
            }))
            .send().await.unwrap();
        assert_eq!(vec_res.status(), 200);
        let vec_json: serde_json::Value = vec_res.json().await.unwrap();
        assert_eq!(vec_json["status"], "ok");
        assert_eq!(vec_json["rows"].as_array().unwrap().len(), 2);
        assert_eq!(vec_json["rows"][0][0], 1);

        // Verify default space is isolated (has 0 Doc vertices)
        let def_schema = client.get(format!("{}/schema?graph=default", base)).send().await.unwrap();
        let def_json: serde_json::Value = def_schema.json().await.unwrap();
        assert_eq!(def_json["vertices"].as_array().unwrap().len(), 0);

        let fraud_schema = client.get(format!("{}/schema?graph=fraud_space", base)).send().await.unwrap();
        let fraud_json: serde_json::Value = fraud_schema.json().await.unwrap();
        assert_eq!(fraud_json["vertices"].as_array().unwrap().len(), 1);
        assert_eq!(fraud_json["vertex_counts"]["Doc"], 3);

        // Verify Pregel telemetry
        let pr_res = client.post(format!("{}/query", base))
            .json(&serde_json::json!({
                "query": "CALL algo.pageRank() YIELD vertex_id, score;",
                "graph": "fraud_space",
                "local_only": true
            }))
            .send().await.unwrap();
        assert_eq!(pr_res.status(), 200);
        let pr_json: serde_json::Value = pr_res.json().await.unwrap();
        assert!(pr_json["pregel"].is_object());

        let pregel_status = client.get(format!("{}/pregel/status", base)).send().await.unwrap();
        assert_eq!(pregel_status.status(), 200);
        let ps_json: serde_json::Value = pregel_status.json().await.unwrap();
        assert_eq!(ps_json["graph_space"], "fraud_space");

        handle.abort();
    }

    #[tokio::test]
    async fn test_run_server_with_shutdown() {
        let tmp = tempfile::tempdir().unwrap();
        let l1 = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let l2 = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let p2 = l2.local_addr().unwrap().port();
        let l3 = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let p3 = l3.local_addr().unwrap().port();
        drop(l1);
        drop(l2);
        drop(l3);

        let args = Args {
            wal_dir: tmp.path().to_path_buf(),
            partitions: 1,
            port: p1,
            client_flight_port: p2,
            http_port: p3,
            flight_address: Some("127.0.0.1".to_string()),
            client_flight_address: Some("127.0.0.1".to_string()),
            node_id: 1,
            peers: Some("http://127.0.0.1:59999".to_string()),
            replication_factor: 1,
            replication_mode: "sync".to_string(),
            enable_gpu: false,
            gpu_device: 0,
            gpu_offload_threshold: 1000,
            gpu_max_vram_mb: 512,
            s3_bucket: Some("test-bucket".to_string()),
            s3_endpoint: Some("http://localhost:9000".to_string()),
            s3_region: "us-east-1".to_string(),
            cluster_mode: "ring".to_string(),
        };

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let _ = run_server_with_shutdown(args, async move {
                let _ = rx.await;
            }).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let _ = tx.send(());
        let _ = handle.await;
    }
}
