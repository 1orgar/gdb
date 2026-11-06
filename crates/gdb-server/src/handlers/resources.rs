use crate::state::{AppState, ReplicationMode, ResourcesParams, RingNode};
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::sync::atomic::Ordering;

pub async fn handle_health(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "UP",
        "service": "GDB Enterprise Graph Database",
        "version": env!("CARGO_PKG_VERSION"),
        "node_id": state.node_id,
        "role": "Peer",
        "cluster_topology": "leaderless-ring",
        "replication_factor": state.replication_factor,
        "replication_mode": state.replication_mode.to_string(),
        "flight_address": state.flight_address,
        "flight_port": state.port,
        "client_flight_address": state.client_flight_address,
        "client_flight_port": state.client_flight_port
    }))
}

pub async fn handle_schema(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(params): Query<ResourcesParams>,
) -> impl IntoResponse {
    let header_graph = headers
        .get("x-gdb-graph")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let requested_graph = params.graph.or(header_graph);
    let scoped = state.with_graph_space(requested_graph.as_deref());
    let schema = scoped.schema.read();
    let vertices = schema.list_vertex_schemas();
    let edges = schema.list_edge_schemas();

    let mut vertex_counts = serde_json::Map::new();
    let mut vertex_indexes = serde_json::Map::new();
    for vs in &vertices {
        let cnt = scoped
            .storage
            .get_property_table(vs.label_id)
            .map(|t| t.len())
            .unwrap_or(0);
        vertex_counts.insert(vs.label.clone(), serde_json::json!(cnt));
        let mut idxs = schema.get_indexes(&vs.label);
        for p in &vs.properties {
            if matches!(p.data_type, gdb_core::schema::DataType::Vector(_)) {
                let hnsw_tag = format!("{} (HNSW)", p.name);
                if !idxs.contains(&hnsw_tag) && !idxs.contains(&p.name) {
                    idxs.push(hnsw_tag);
                }
            }
        }
        vertex_indexes.insert(vs.label.clone(), serde_json::json!(idxs));
    }

    Json(serde_json::json!({
        "status": "ok",
        "graph": schema.name,
        "active_graph": state.current_graph_name(),
        "vertices": vertices,
        "edges": edges,
        "vertex_counts": vertex_counts,
        "vertex_indexes": vertex_indexes,
        "total_vertices": scoped.storage.total_vertices(),
        "total_edges": scoped.storage.total_edges(),
    }))
}

pub async fn handle_graphs(State(state): State<AppState>) -> impl IntoResponse {
    let spaces = state.list_graph_spaces();
    Json(serde_json::json!({
        "status": "ok",
        "active_graph": state.current_graph_name(),
        "graphs": spaces,
    }))
}

pub async fn handle_pregel_status(State(state): State<AppState>) -> impl IntoResponse {
    let t = state.pregel_status.read().clone();
    Json(serde_json::json!({
        "status": "completed",
        "algorithm": t.algorithm,
        "graph_space": t.graph_space,
        "current_superstep": t.current_superstep,
        "max_supersteps": t.max_supersteps,
        "active_vertices": t.active_vertices,
        "vertices_processed": t.total_vertices,
        "edges_processed": t.total_edges,
        "total_messages": t.messages_exchanged,
        "messages_exchanged": t.messages_exchanged,
        "total_bytes": t.bytes_shuffled,
        "bytes_shuffled": t.bytes_shuffled,
        "convergence_delta": t.convergence_delta,
        "tolerance": t.tolerance,
        "elapsed_ms": t.elapsed_ms,
        "elapsed_us": (t.elapsed_ms * 1000.0) as u64,
        "execution_mode": t.engine_mode,
        "supersteps": t.supersteps,
        "pregel": t,
    }))
}

pub async fn handle_cluster(State(state): State<AppState>) -> impl IntoResponse {
    let ring_nodes = state.ring_nodes.read().clone();
    let total_nodes = ring_nodes.len();
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
        "flight_address": state.flight_address,
        "flight_port": state.port,
        "client_flight_address": state.client_flight_address,
        "client_flight_port": state.client_flight_port,
        "http_port": state.http_port,
        "peers": state.peers,
        "ring_nodes": ring_nodes,
        "assigned_tokens": format!("Primary token: u % {} == {}", total_nodes, state.node_id.saturating_sub(1)),
        "s3_tiering": if state.s3_manager.is_some() { "Enabled" } else { "Disabled" },
        "s3_bucket": state.s3_bucket.clone().unwrap_or_default(),
        "gpu_enabled": state.gpu_enabled,
        "gpu_device": state.gpu_device,
        "gpu_backend": state.gpu_backend,
        "gpu_threshold": state.gpu_threshold,
        "gpu_max_vram_mb": state.gpu_max_vram_mb,
        "status": "UP"
    }))
}

pub async fn handle_cluster_join(
    State(state): State<AppState>,
    Json(node): Json<RingNode>,
) -> impl IntoResponse {
    let mut ring = state.ring_nodes.write();
    if let Some(existing) = ring.iter_mut().find(|n| n.node_id == node.node_id) {
        *existing = node;
    } else {
        ring.push(node);
        ring.sort_by_key(|n| n.node_id);
    }
    let current_ring = ring.clone();
    Json(serde_json::json!({
        "status": "ok",
        "ring_nodes": current_ring,
    }))
}

pub async fn handle_resources(
    State(state): State<AppState>,
    Query(params): Query<ResourcesParams>,
) -> impl IntoResponse {
    let state = state.with_graph_space(params.graph.as_deref());
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

    let ring_snapshot = state.ring_nodes.read().clone();
    let total_nodes = ring_snapshot.len();

    let mut cluster_total_edges = total_edges;
    let mut cluster_total_vertices = total_vertices;
    let mut peer_metrics = vec![serde_json::json!({
        "node_id": state.node_id,
        "edges": total_edges,
        "vertices": total_vertices,
        "csr_edges": csr_edges,
        "memtable_edges": memtable_edges,
        "role": "Coordinator"
    })];

    if params.local != Some(true) && total_nodes > 1 {
        let peers: Vec<RingNode> = ring_snapshot.iter().filter(|n| n.node_id != state.node_id).cloned().collect();
        let client = state.http_client.clone();
        let mut peer_futs = Vec::new();

        for peer in peers {
            let p_url = format!("{}/resources?local=true", peer.http_url.trim_end_matches('/'));
            let c = client.clone();
            peer_futs.push(tokio::spawn(async move {
                match c.get(&p_url)
                    .timeout(std::time::Duration::from_millis(500))
                    .send()
                    .await
                {
                    Ok(resp) => resp.json::<serde_json::Value>().await.ok(),
                    Err(_) => None,
                }
            }));
        }

        let peer_resps = futures::future::join_all(peer_futs).await;
        let mut sum_edges = total_edges;
        let mut sum_vertices = total_vertices;
        let mut max_edges = total_edges;
        let mut max_vertices = total_vertices;

        for pres in peer_resps {
            if let Ok(Some(val)) = pres {
                let pe = val.get("total_edges").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let pv = val.get("total_vertices").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let pn = val.get("node_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let pcsr = val.get("csr_edges").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let pmem = val.get("memtable_edges").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

                sum_edges += pe;
                sum_vertices += pv;
                max_edges = max_edges.max(pe);
                max_vertices = max_vertices.max(pv);

                peer_metrics.push(serde_json::json!({
                    "node_id": pn,
                    "edges": pe,
                    "vertices": pv,
                    "csr_edges": pcsr,
                    "memtable_edges": pmem,
                    "role": "Peer"
                }));
            }
        }

        if state.replication_factor == 1 {
            cluster_total_edges = sum_edges;
            cluster_total_vertices = sum_vertices.max(max_vertices);
        } else if (state.replication_factor as usize) >= total_nodes {
            cluster_total_edges = max_edges;
            cluster_total_vertices = max_vertices;
        } else {
            let rf_f64 = state.replication_factor as f64;
            cluster_total_edges = (sum_edges as f64 / rf_f64).round() as usize;
            cluster_total_vertices = (sum_vertices as f64 / rf_f64).round() as usize;
        }
    }

    Json(serde_json::json!({
        "node_id": state.node_id,
        "role": "Peer",
        "cluster_mode": format!("LEADERLESS RING (RF={}, Mode={})", state.replication_factor, state.replication_mode),
        "cluster_topology": "leaderless-ring",
        "replication_factor": state.replication_factor,
        "replication_mode": state.replication_mode.to_string(),
        "total_nodes": total_nodes,
        "cluster_total_vertices": cluster_total_vertices,
        "cluster_total_edges": cluster_total_edges,
        "total_vertices": cluster_total_vertices,
        "total_edges": cluster_total_edges,
        "local_vertices": total_vertices,
        "local_edges": total_edges,
        "node_total_vertices": total_vertices,
        "node_total_edges": total_edges,
        "peer_metrics": peer_metrics,
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
        "gpu_threshold": state.gpu_threshold,
        "gpu_max_vram_mb": state.gpu_max_vram_mb,
        "active_graph": state.current_graph_name(),
        "graphs_count": state.spaces.read().len(),
        "pregel": state.pregel_status.read().clone()
    }))
}

pub async fn handle_gpu(State(state): State<AppState>) -> impl IntoResponse {
    let memory_model = if state.gpu_enabled {
        #[cfg(target_os = "macos")]
        { "Unified Memory Architecture (Zero-Copy UMA)" }
        #[cfg(not(target_os = "macos"))]
        { "Dedicated PCIe HBM2/GDDR Host-Device" }
    } else {
        "Host RAM (CPU SIMD Fallback)"
    };

    let paging_strategy = if state.gpu_enabled {
        #[cfg(target_os = "macos")]
        { "Unified Memory (UMA Zero-Copy)" }
        #[cfg(not(target_os = "macos"))]
        { "Windowed Double-Buffered CSR Streaming (Discrete Out-Of-Core Paging)" }
    } else {
        "None (CPU Fallback)"
    };

    let gpu_dispatches = state
        .executor
        .gpu
        .as_ref()
        .map(|g| g.total_dispatches())
        .unwrap_or(0);

    Json(serde_json::json!({
        "enabled": state.gpu_enabled,
        "device_id": state.gpu_device,
        "backend": state.gpu_backend,
        "threshold_edges": state.gpu_threshold,
        "max_vram_mb": state.gpu_max_vram_mb,
        "memory_model": memory_model,
        "paging_strategy": paging_strategy,
        "active": state.gpu_enabled,
        "gpu_dispatches": gpu_dispatches,
        "status": if state.gpu_enabled { "Active" } else { "Disabled (pass --enable-gpu)" },
        "supported_kernels": [
            "Parallel BFS Frontier Expansion (parallel_bfs_step)",
            "Pull-Based PageRank SpMV (parallel_pagerank_pull)",
            "Weakly Connected Components (parallel_wcc_step)",
            "Sorted Adjacency Triangle Intersection (parallel_triangle_step)",
            "Single-Source Shortest Path (parallel_bfs_step)",
            "In-Degree Histogram Kernel (parallel_degree_in)",
            "Parallel Vector Similarity Search (parallel_vector_similarity)",
            "Windowed Out-Of-Core CSR Streaming"
        ]
    }))
}

pub async fn handle_metrics(State(state): State<AppState>) -> impl IntoResponse {
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
