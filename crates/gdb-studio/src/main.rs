use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod ui;

#[derive(Parser, Debug, Clone)]
#[command(name = "gdb-studio", about = "GDB Studio — Web UI & Interactive Graph Workspace")]
struct Args {
    /// Port to serve the Web UI on
    #[arg(short, long, default_value = "3000")]
    port: u16,

    /// Bind address
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// Target GDB Cluster HTTP REST endpoint URL
    #[arg(short, long, default_value = "http://localhost:8847")]
    cluster_url: String,
}

#[derive(Clone)]
struct AppState {
    default_cluster_url: String,
    http_client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct EndpointParam {
    endpoint: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QueryRequest {
    query: String,
    endpoint: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphNode {
    id: String,
    label: String,
    properties: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphEdge {
    source: String,
    target: String,
    label: String,
    properties: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphData {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StudioQueryResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    elapsed_us: u128,
    num_rows: usize,
    columns: Vec<String>,
    rows: Vec<Vec<serde_json::Value>>,
    graph: GraphData,
}

async fn handle_index() -> impl IntoResponse {
    Html(ui::HTML_INDEX)
}

async fn handle_health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "UP",
        "service": "gdb-studio",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn handle_cluster_status(
    State(state): State<Arc<AppState>>,
    Query(params): Query<EndpointParam>,
) -> impl IntoResponse {
    let target = params.endpoint.unwrap_or_else(|| state.default_cluster_url.clone());
    let cluster_url = format!("{}/cluster", target.trim_end_matches('/'));

    match state.http_client.get(&cluster_url).send().await {
        Ok(res) if res.status().is_success() => {
            if let Ok(info) = res.json::<serde_json::Value>().await {
                return Json(serde_json::json!({
                    "cluster_url": target,
                    "status": "connected",
                    "node_id": info.get("node_id"),
                    "role": info.get("role"),
                    "cluster_topology": info.get("cluster_topology"),
                    "replication_factor": info.get("replication_factor"),
                    "effective_replication_factor": info.get("effective_replication_factor"),
                    "replication_mode": info.get("replication_mode"),
                    "total_nodes": info.get("total_nodes"),
                    "partitions": info.get("partitions"),
                    "ring_nodes": info.get("ring_nodes"),
                    "gpu_enabled": info.get("gpu_enabled"),
                    "gpu_device": info.get("gpu_device"),
                    "gpu_backend": info.get("gpu_backend"),
                    "gpu_threshold": info.get("gpu_threshold")
                }));
            }
        }
        _ => {}
    }

    Json(serde_json::json!({
        "cluster_url": target,
        "status": "disconnected",
        "nodes": []
    }))
}

async fn handle_proxy_cluster(
    State(state): State<Arc<AppState>>,
    Query(params): Query<EndpointParam>,
) -> impl IntoResponse {
    let target = params.endpoint.unwrap_or_else(|| state.default_cluster_url.clone());
    let url = format!("{}/cluster", target.trim_end_matches('/'));
    match state.http_client.get(&url).send().await {
        Ok(res) => {
            let status = res.status().as_u16();
            let json: serde_json::Value = res.json().await.unwrap_or_default();
            (
                axum::http::StatusCode::from_u16(status).unwrap_or(axum::http::StatusCode::OK),
                Json(json),
            )
        }
        Err(e) => (
            axum::http::StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": format!("Failed to reach cluster at {}: {}", url, e) })),
        ),
    }
}

async fn handle_proxy_resources(
    State(state): State<Arc<AppState>>,
    Query(params): Query<EndpointParam>,
) -> impl IntoResponse {
    let target = params.endpoint.unwrap_or_else(|| state.default_cluster_url.clone());
    let url = format!("{}/resources", target.trim_end_matches('/'));
    match state.http_client.get(&url).send().await {
        Ok(res) => {
            let status = res.status().as_u16();
            let json: serde_json::Value = res.json().await.unwrap_or_default();
            (
                axum::http::StatusCode::from_u16(status).unwrap_or(axum::http::StatusCode::OK),
                Json(json),
            )
        }
        Err(e) => (
            axum::http::StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": format!("Failed to reach cluster at {}: {}", url, e) })),
        ),
    }
}

async fn handle_proxy_gpu(
    State(state): State<Arc<AppState>>,
    Query(params): Query<EndpointParam>,
) -> impl IntoResponse {
    let target = params.endpoint.unwrap_or_else(|| state.default_cluster_url.clone());
    let url = format!("{}/gpu", target.trim_end_matches('/'));
    match state.http_client.get(&url).send().await {
        Ok(res) => {
            let status = res.status().as_u16();
            let json: serde_json::Value = res.json().await.unwrap_or_default();
            (
                axum::http::StatusCode::from_u16(status).unwrap_or(axum::http::StatusCode::OK),
                Json(json),
            )
        }
        Err(e) => (
            axum::http::StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": format!("Failed to reach cluster at {}: {}", url, e) })),
        ),
    }
}

async fn handle_proxy_compact(
    State(state): State<Arc<AppState>>,
    Query(params): Query<EndpointParam>,
) -> impl IntoResponse {
    let target = params.endpoint.unwrap_or_else(|| state.default_cluster_url.clone());
    let url = format!("{}/compact", target.trim_end_matches('/'));
    match state.http_client.post(&url).send().await {
        Ok(res) => {
            let status = res.status().as_u16();
            let json: serde_json::Value = res.json().await.unwrap_or_default();
            (
                axum::http::StatusCode::from_u16(status).unwrap_or(axum::http::StatusCode::OK),
                Json(json),
            )
        }
        Err(e) => (
            axum::http::StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": format!("Failed to trigger compaction at {}: {}", url, e) })),
        ),
    }
}

async fn handle_query(
    State(state): State<Arc<AppState>>,
    Json(req): Json<QueryRequest>,
) -> impl IntoResponse {
    let target_endpoint = req
        .endpoint
        .unwrap_or_else(|| state.default_cluster_url.clone());
    let cluster_query_url = format!("{}/query", target_endpoint.trim_end_matches('/'));

    let start = std::time::Instant::now();

    // Relay to GDB cluster
    let cluster_res = state
        .http_client
        .post(&cluster_query_url)
        .header("Content-Type", "application/json")
        .body(serde_json::json!({ "query": req.query }).to_string())
        .send()
        .await;

    let elapsed_us = start.elapsed().as_micros();

    match cluster_res {
        Ok(response) => {
            if let Ok(mut json_val) = response.json::<serde_json::Value>().await {
                let status = json_val["status"].as_str().unwrap_or("ok").to_string();
                let message = json_val["message"].as_str().map(String::from);
                let error = json_val["error"].as_str().map(String::from);
                let query_elapsed = json_val["elapsed_us"].as_u64().unwrap_or(elapsed_us as u64) as u128;

                let columns: Vec<String> = json_val["columns"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                let rows: Vec<Vec<serde_json::Value>> = json_val["rows"]
                    .as_array_mut()
                    .map(|arr| {
                        arr.iter_mut()
                            .filter_map(|r| r.as_array_mut().map(|inner| std::mem::take(inner)))
                            .collect()
                    })
                    .unwrap_or_default();

                // Extract Graph Elements
                let mut nodes = Vec::new();
                let mut edges = Vec::new();
                let mut node_set = std::collections::HashSet::new();

                // If columns look like graph traversal (e.g. source, target)
                if columns.len() >= 2 && !columns[1].contains("score") && !columns[1].contains("distance") {
                    for row in &rows {
                        if row.len() >= 2 {
                            let u_id = row[0].to_string().trim_matches('"').to_string();
                            let v_id = row[1].to_string().trim_matches('"').to_string();

                            if node_set.insert(u_id.clone()) {
                                nodes.push(GraphNode {
                                    id: u_id.clone(),
                                    label: u_id.clone(),
                                    properties: serde_json::json!({ "id": u_id }),
                                });
                            }

                            if node_set.insert(v_id.clone()) {
                                nodes.push(GraphNode {
                                    id: v_id.clone(),
                                    label: v_id.clone(),
                                    properties: serde_json::json!({ "id": v_id }),
                                });
                            }

                            edges.push(GraphEdge {
                                source: u_id,
                                target: v_id,
                                label: if row.len() > 2 { row[2].to_string().trim_matches('"').to_string() } else { "REL".to_string() },
                                properties: serde_json::json!({}),
                            });
                        }
                    }
                } else if columns.iter().any(|c| c == "vertex_id") {
                    let id_idx = columns.iter().position(|c| c == "vertex_id").unwrap();
                    let val_idx = if id_idx == 0 && columns.len() > 1 { 1 } else { 0 };
                    let val_name = &columns[val_idx];

                    for row in &rows {
                        if row.len() > id_idx {
                            let vid = row[id_idx].to_string().trim_matches('"').to_string();
                            let val = if row.len() > val_idx { row[val_idx].clone() } else { serde_json::Value::Null };

                            nodes.push(GraphNode {
                                id: vid.clone(),
                                label: format!("{} ({})", vid, val),
                                properties: serde_json::json!({ val_name: val }),
                            });
                        }
                    }
                }

                let num_rows = rows.len();

                Json(StudioQueryResponse {
                    status,
                    message,
                    error,
                    elapsed_us: query_elapsed,
                    num_rows,
                    columns,
                    rows,
                    graph: GraphData { nodes, edges },
                })
            } else {
                Json(StudioQueryResponse {
                    status: "error".to_string(),
                    message: None,
                    error: Some("Invalid JSON response from GDB cluster".to_string()),
                    elapsed_us,
                    num_rows: 0,
                    columns: vec![],
                    rows: vec![],
                    graph: GraphData { nodes: vec![], edges: vec![] },
                })
            }
        }
        Err(e) => Json(StudioQueryResponse {
            status: "error".to_string(),
            message: None,
            error: Some(format!("Failed to connect to GDB cluster at {}: {}", cluster_query_url, e)),
            elapsed_us,
            num_rows: 0,
            columns: vec![],
            rows: vec![],
            graph: GraphData { nodes: vec![], edges: vec![] },
        }),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    println!("\x1b[1;36m");
    println!("  ██████╗ ██████╗ ██████╗       ███████╗████████╗██╗   ██╗██████╗ ██╗ ██████╗ ");
    println!(" ██╔════╝ ██╔══██╗██╔══██╗      ██╔════╝╚══██╔══╝██║   ██║██╔══██╗██║██╔═══██╗");
    println!(" ██║  ███╗██║  ██║██████╔╝█████╗███████╗   ██║   ██║   ██║██║  ██║██║██║   ██║");
    println!(" ██║   ██║██║  ██║██╔══██╗╚════╝╚════██║   ██║   ██║   ██║██║  ██║██║██║   ██║");
    println!(" ╚██████╔╝██████╔╝██████╔╝      ███████║   ██║   ╚██████╔╝██████╔╝██║╚██████╔╝");
    println!("  ╚═════╝ ╚═════╝ ╚═════╝       ╚══════╝   ╚═╝    ╚═════╝ ╚═════╝ ╚═╝ ╚═════╝ ");
    println!("  GDB Studio — Web UI & Interactive Graph Workspace");
    println!("\x1b[0m");

    println!(
        "\x1b[1;32m[+] Target GDB Cluster:\x1b[0m {}",
        args.cluster_url
    );

    let state = Arc::new(AppState {
        default_cluster_url: args.cluster_url,
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?,
    });

    let app = Router::new()
        .route("/", get(handle_index))
        .route("/health", get(handle_health))
        .route("/api/query", post(handle_query))
        .route("/api/cluster/status", get(handle_cluster_status))
        .route("/api/cluster", get(handle_proxy_cluster))
        .route("/api/resources", get(handle_proxy_resources))
        .route("/api/gpu", get(handle_proxy_gpu))
        .route("/api/compact", post(handle_proxy_compact))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let bind_addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    println!(
        "\x1b[1;32m[+] GDB Studio Web UI:\x1b[0m http://localhost:{}",
        args.port
    );
    println!("\x1b[1;33m[ Ready! Open http://localhost:{} in your browser ]\x1b[0m\n", args.port);

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
