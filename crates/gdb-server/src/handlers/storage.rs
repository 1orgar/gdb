use crate::handlers::query::replicate_to_targets;
use crate::state::{AppState, EdgeDto, ReplicationPayload, RingNode, StorageOutEdgesRequest, StorageVertexPropertiesRequest};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use gdb_core::{DataValue, EdgeType, LabelId, VertexId};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::time::Instant;

pub async fn handle_storage_out_edges(
    State(state): State<AppState>,
    Json(payload): Json<StorageOutEdgesRequest>,
) -> impl IntoResponse {
    let et = payload.edge_type.map(EdgeType);
    let snapshot = state.storage.latest_version();
    let mut results: HashMap<u64, Vec<EdgeDto>> = HashMap::new();
    for vid_u64 in payload.vids {
        let vid = VertexId(vid_u64);
        let edges = state.storage.get_out_edges(vid, et, snapshot);
        if !edges.is_empty() {
            results.insert(vid_u64, edges.into_iter().map(EdgeDto::from).collect());
        }
    }
    Json(results)
}

pub async fn handle_storage_vertex_properties(
    State(state): State<AppState>,
    Json(payload): Json<StorageVertexPropertiesRequest>,
) -> impl IntoResponse {
    let mut results: HashMap<u64, HashMap<String, DataValue>> = HashMap::new();
    let prop_filter: Option<HashSet<String>> = payload.properties.map(|p| p.into_iter().collect());

    for vid_u64 in payload.vids {
        let vid = VertexId(vid_u64);
        let lid = payload.label_id.map(LabelId);
        if let Some(mut props) = state.storage.get_all_vertex_properties(vid, lid) {
            if let Some(ref filter) = prop_filter {
                props.retain(|k, _| filter.contains(k));
            }
            if !props.is_empty() {
                results.insert(vid_u64, props);
            }
        }
    }
    Json(results)
}

pub async fn handle_replicate(
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

pub async fn handle_compact(State(state): State<AppState>) -> impl IntoResponse {
    let start = Instant::now();
    state.storage.compact();
    state.compactions_count.fetch_add(1, Ordering::Relaxed);

    // Replicate compaction to all other peers in the ring
    let other_nodes: Vec<RingNode> = state.ring_nodes.read().iter().filter(|n| n.node_id != state.node_id).cloned().collect();
    replicate_to_targets(&state.http_client, &other_nodes, "compact;", state.replication_mode, &state.replications_count).await;

    Json(serde_json::json!({
        "status": "ok",
        "message": "Compaction completed",
        "elapsed_us": start.elapsed().as_micros()
    }))
}

pub async fn handle_snapshot(State(state): State<AppState>) -> impl IntoResponse {
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
