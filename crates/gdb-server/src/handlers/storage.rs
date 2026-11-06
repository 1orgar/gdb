use crate::handlers::query::replicate_to_targets;
use crate::state::{AppState, EdgeDto, ReplicationPayload, RingNode, StorageOutEdgesRequest, StorageVertexPropertiesRequest};
use axum::extract::State;
use axum::Json;
use gdb_core::{DataValue, EdgeType, LabelId, VertexId};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::time::Instant;

pub async fn handle_storage_out_edges(
    State(state): State<AppState>,
    Json(payload): Json<StorageOutEdgesRequest>,
) -> Json<HashMap<u64, Vec<EdgeDto>>> {
    let state = state.with_graph_space(payload.graph.as_deref());
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

pub async fn handle_storage_all_edges(
    State(state): State<AppState>,
) -> Json<Vec<EdgeDto>> {
    let state = state.with_graph_space(None);
    let edges = state.storage.get_all_active_edges();
    Json(edges.into_iter().map(EdgeDto::from).collect())
}

pub async fn handle_storage_all_edges_bin(
    State(state): State<AppState>,
) -> Vec<u8> {
    let state = state.with_graph_space(None);
    let edges = state.storage.get_all_active_edges();
    let mut buf = Vec::with_capacity(edges.len() * 28);
    for e in edges {
        buf.extend_from_slice(&e.src.0.to_le_bytes());
        buf.extend_from_slice(&e.edge_type.0.to_le_bytes());
        buf.extend_from_slice(&e.rank.to_le_bytes());
        buf.extend_from_slice(&e.dst.0.to_le_bytes());
    }
    buf
}

pub async fn handle_storage_vertex_properties(
    State(state): State<AppState>,
    Json(payload): Json<StorageVertexPropertiesRequest>,
) -> Json<HashMap<u64, HashMap<String, DataValue>>> {
    let state = state.with_graph_space(payload.graph.as_deref());
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
) -> Json<serde_json::Value> {
    let state = state.with_graph_space(payload.graph.as_deref());
    let q = payload.query.trim();
    if q.eq_ignore_ascii_case("compact") || q.eq_ignore_ascii_case("compact;") {
        state.storage.compact();
        *state.global_csr_cache.write() = None;
        if let Some(ref remote) = state.executor.remote_expander {
            remote.clear_cache();
        }
        state.compactions_count.fetch_add(1, Ordering::Relaxed);
        return Json(serde_json::json!({ "status": "ok", "message": "Replicated compaction applied" }));
    }

    match gdb_parser::parse(q) {
        Ok(stmt) => {
            match &stmt {
                gdb_parser::ast::Statement::CreateGraph { name, if_not_exists } => {
                    let _ = state.create_graph_space(name, *if_not_exists);
                    return Json(serde_json::json!({ "status": "ok", "message": format!("Replicated CREATE GRAPH '{}'", name) }));
                }
                gdb_parser::ast::Statement::DropGraph { name, if_exists } => {
                    let _ = state.drop_graph_space(name, *if_exists);
                    return Json(serde_json::json!({ "status": "ok", "message": format!("Replicated DROP GRAPH '{}'", name) }));
                }
                gdb_parser::ast::Statement::UseGraph { name } => {
                    let _ = state.use_graph_space(name);
                    return Json(serde_json::json!({ "status": "ok", "message": format!("Replicated USE GRAPH '{}'", name) }));
                }
                _ => {}
            }
            match state.executor.execute(stmt) {
                Ok(res) => {
                    *state.global_csr_cache.write() = None;
                    if let Some(ref remote) = state.executor.remote_expander {
                        remote.clear_cache();
                    }
                    Json(serde_json::json!({ "status": "ok", "message": res.message }))
                }
                Err(e) => Json(serde_json::json!({ "status": "error", "error": e.to_string() })),
            }
        }
        Err(e) => Json(serde_json::json!({ "status": "error", "error": e.to_string() })),
    }
}

pub async fn handle_compact(State(state): State<AppState>) -> Json<serde_json::Value> {
    let state = state.with_graph_space(None);
    let start = Instant::now();
    state.storage.compact();
    *state.global_csr_cache.write() = None;
    if let Some(ref remote) = state.executor.remote_expander {
        remote.clear_cache();
    }
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

pub async fn handle_snapshot(State(state): State<AppState>) -> Json<serde_json::Value> {
    let state = state.with_graph_space(None);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create_test_server;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_storage_handlers() {
        let temp_dir = tempdir().unwrap();
        let (_addr, handle, state) = create_test_server(temp_dir.path().to_path_buf()).await.unwrap();

        // 1. handle_storage_out_edges with empty vids
        let req = StorageOutEdgesRequest { vids: vec![], edge_type: None, graph: None };
        let resp = handle_storage_out_edges(State(state.clone()), Json(req)).await;
        let json_val = serde_json::to_value(resp.0).unwrap();
        assert!(json_val.as_object().unwrap().is_empty());

        // 2. handle_storage_vertex_properties with empty vids
        let req = StorageVertexPropertiesRequest { vids: vec![], label_id: None, properties: None, graph: None };
        let resp = handle_storage_vertex_properties(State(state.clone()), Json(req)).await;
        let json_val = serde_json::to_value(resp.0).unwrap();
        assert!(json_val.as_object().unwrap().is_empty());

        // 3. handle_replicate compaction
        let req = ReplicationPayload { query: "compact".to_string(), graph: None };
        let resp = handle_replicate(State(state.clone()), Json(req)).await;
        assert_eq!(resp.0["status"], "ok");

        // 4. handle_replicate invalid query
        let req = ReplicationPayload { query: "INVALID QUERY SYNTAX".to_string(), graph: None };
        let resp = handle_replicate(State(state.clone()), Json(req)).await;
        assert_eq!(resp.0["status"], "error");

        // 5. handle_snapshot unconfigured
        let resp = handle_snapshot(State(state.clone())).await;
        assert_eq!(resp.0["status"], "not_configured");

        handle.abort();
    }
}
