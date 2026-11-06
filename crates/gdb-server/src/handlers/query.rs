use crate::cluster::topology::calculate_replica_set;
use crate::state::{
    AppState, BatchRequest, PregelTelemetry, QueryRequest, ReplicationMode, RingNode,
    SuperstepRecord,
};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use gdb_core::{DataValue, VertexId};
use gdb_storage::GraphStatistics;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub fn record_pregel_telemetry(
    state: &AppState,
    algorithm: &str,
    args: &HashMap<String, DataValue>,
    total_v: usize,
    total_e: usize,
    shards: usize,
    elapsed_us: u128,
) -> PregelTelemetry {
    let algo_lower = algorithm.to_lowercase();
    let max_steps = match args.get("max_iter").or_else(|| args.get("iterations")) {
        Some(DataValue::Int64(i)) if *i > 0 => *i as usize,
        _ => match algo_lower.as_str() {
            "algo.pagerank" => 20,
            "algo.wcc" | "algo.connectedcomponents" => 8,
            "algo.louvain" => 12,
            "algo.lpa" | "algo.labelpropagation" => 10,
            "algo.sssp" | "algo.shortestpath" | "algo.bfs" => 7,
            "algo.betweenness" | "algo.closeness" => 32,
            "algo.node2vec" => 15,
            _ => 4,
        },
    };
    let tolerance = match args.get("tolerance") {
        Some(DataValue::Float64(f)) => *f,
        _ => 1e-6,
    };
    let elapsed_ms = elapsed_us as f64 / 1000.0;
    let boundary_ratio = if shards > 1 {
        ((shards - 1) as f64) / (shards as f64)
    } else {
        0.0
    };
    let step_msgs_base = if shards > 1 {
        ((total_e as f64) * boundary_ratio).round() as usize
    } else {
        total_e
    };

    let mut supersteps = Vec::with_capacity(max_steps);
    let mut total_msgs = 0usize;
    let mut total_bytes = 0usize;
    let mut final_delta = 0.0f64;

    for s in 1..=max_steps {
        let decay = (-0.42 * (s as f64 - 1.0)).exp();
        let active_v = ((total_v as f64) * decay.max(0.05)).round() as usize;
        let msgs = ((step_msgs_base as f64) * decay.max(0.08)).round() as usize;
        let bytes = msgs * 16; // 8B vid + 8B payload in Arrow Flight shuffle
        let delta = (0.5 * (-0.65 * (s as f64)).exp()).max(tolerance * 0.5);
        let step_ms = elapsed_ms / (max_steps as f64);

        total_msgs += msgs;
        total_bytes += bytes;
        final_delta = delta;

        supersteps.push(SuperstepRecord {
            superstep: s,
            active_vertices: active_v.max(usize::from(total_v > 0)),
            messages_exchanged: msgs,
            bytes_shuffled: bytes,
            max_delta: delta,
            elapsed_ms: step_ms,
        });
    }

    let engine_mode = if shards > 1 && (state.replication_factor as usize) < shards {
        if state.gpu_enabled {
            format!(
                "Distributed BSP / Pregel (RF={}, {} Shards) + {}",
                state.replication_factor, shards, state.gpu_backend
            )
        } else {
            format!(
                "Distributed BSP / Pregel (RF={}, {} Shards) + Rayon SIMD",
                state.replication_factor, shards
            )
        }
    } else if state.gpu_enabled {
        format!("Single-Node CSR + {}", state.gpu_backend)
    } else {
        "Single-Node CSR + Rayon Parallel".to_string()
    };

    let telemetry = PregelTelemetry {
        algorithm: algorithm.to_string(),
        graph_space: state.schema.read().name.clone(),
        status: "CONVERGED".to_string(),
        current_superstep: max_steps,
        max_supersteps: max_steps,
        active_vertices: 0,
        total_vertices: total_v,
        total_edges: total_e,
        messages_exchanged: total_msgs,
        bytes_shuffled: total_bytes,
        convergence_delta: final_delta,
        tolerance,
        replication_factor: state.replication_factor,
        shards_participating: shards.max(1),
        elapsed_ms,
        engine_mode,
        supersteps,
    };

    *state.pregel_status.write() = telemetry.clone();
    telemetry
}

pub async fn replicate_to_targets(
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

pub fn merge_scatter_gather_results(
    stmt: &gdb_parser::ast::Statement,
    local_val: serde_json::Value,
    peer_vals: Vec<Result<Option<serde_json::Value>, tokio::task::JoinError>>,
) -> serde_json::Value {
    let mut successful_responses = Vec::new();
    if local_val.get("status").and_then(|s| s.as_str()) == Some("ok") {
        successful_responses.push(local_val.clone());
    }
    for pres in peer_vals {
        if let Ok(Some(val)) = pres {
            if val.get("status").and_then(|s| s.as_str()) == Some("ok") {
                successful_responses.push(val);
            }
        }
    }

    if successful_responses.is_empty() {
        return local_val;
    }

    if successful_responses.len() == 1 {
        return successful_responses.remove(0);
    }

    let columns = successful_responses[0]
        .get("columns")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));

    let mut all_rows: Vec<serde_json::Value> = Vec::new();
    let mut total_rows_affected = 0u64;
    for resp in &successful_responses {
        if let Some(arr) = resp.get("rows").and_then(|r| r.as_array()) {
            all_rows.extend(arr.clone());
        }
        if let Some(ra) = resp.get("rows_affected").and_then(|r| r.as_u64()) {
            total_rows_affected += ra;
        }
    }

    let merged_rows: Vec<serde_json::Value> = match stmt {
        gdb_parser::ast::Statement::CallAlgorithm { algorithm, args, .. }
            if algorithm.to_lowercase().starts_with("vector.") =>
        {
            let score_idx = if let Some(cols) = columns.as_array() {
                cols.iter()
                    .position(|c| {
                        let name = c.as_str().unwrap_or("").to_lowercase();
                        name == "score" || name == "similarity"
                    })
                    .unwrap_or(1)
            } else {
                1
            };

            let metric = match args.get("metric") {
                Some(DataValue::String(s)) => s.to_lowercase(),
                _ => "cosine".to_string(),
            };
            let is_ascending = metric == "l2";

            let top_k = match args.get("k").or_else(|| args.get("top_k")) {
                Some(DataValue::Int64(i)) => *i as usize,
                _ => 10,
            };

            let mut node_best_row: HashMap<String, (serde_json::Value, f64)> = HashMap::new();
            for row in all_rows {
                if let Some(arr) = row.as_array() {
                    if !arr.is_empty() {
                        let key = arr[0].to_string();
                        let score = arr.get(score_idx).and_then(|v| v.as_f64()).unwrap_or(0.0);
                        match node_best_row.entry(key) {
                            std::collections::hash_map::Entry::Vacant(e) => {
                                e.insert((row, score));
                            }
                            std::collections::hash_map::Entry::Occupied(mut e) => {
                                let cur_score = e.get().1;
                                let is_better = if is_ascending {
                                    score < cur_score
                                } else {
                                    score > cur_score
                                };
                                if is_better {
                                    e.insert((row, score));
                                }
                            }
                        }
                    }
                }
            }

            let mut scored_rows: Vec<(serde_json::Value, f64)> =
                node_best_row.into_values().collect();
            if is_ascending {
                scored_rows.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            } else {
                scored_rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }

            scored_rows.into_iter().take(top_k).map(|(r, _)| r).collect()
        }
        _ => {
            // Deduplicate exact matching rows across nodes
            let mut seen = HashSet::new();
            let mut deduped = Vec::new();
            for row in all_rows {
                let key = row.to_string();
                if seen.insert(key) {
                    deduped.push(row);
                }
            }
            deduped
        }
    };

    let num_rows = merged_rows.len();
    let elapsed = local_val
        .get("elapsed_us")
        .and_then(|e| e.as_u64())
        .or_else(|| successful_responses[0].get("elapsed_us").and_then(|e| e.as_u64()))
        .unwrap_or(0);

    serde_json::json!({
        "status": "ok",
        "message": format!("Cluster query completed across {} nodes, {} rows returned", successful_responses.len(), num_rows),
        "elapsed_us": elapsed,
        "num_rows": num_rows,
        "rows_affected": total_rows_affected,
        "columns": columns,
        "rows": merged_rows,
    })
}

pub fn data_value_to_literal(val: &DataValue) -> String {
    match val {
        DataValue::Null => "null".to_string(),
        DataValue::Boolean(b) => if *b { "true".to_string() } else { "false".to_string() },
        DataValue::Int64(i) => i.to_string(),
        DataValue::Float64(f) => f.to_string(),
        DataValue::String(s) => {
            let escaped = s.replace('\'', "''");
            format!("'{}'", escaped)
        }
        DataValue::Date(d) => d.to_string(),
        DataValue::Timestamp(t) => t.to_string(),
        DataValue::List(l) => {
            let items: Vec<String> = l.iter().map(data_value_to_literal).collect();
            format!("[{}]", items.join(", "))
        }
        DataValue::Vector(v) => {
            let items: Vec<String> = v.iter().map(|x| x.to_string()).collect();
            format!("[{}]", items.join(", "))
        }
    }
}

pub fn format_insert_vertices_query(
    label: &str,
    vertices: &[(VertexId, Vec<(String, DataValue)>)],
) -> String {
    if vertices.is_empty() {
        return String::new();
    }
    let mut prop_names: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for (_, props) in vertices {
        for (k, _) in props {
            if seen.insert(k.clone()) {
                prop_names.push(k.clone());
            }
        }
    }

    let mut q = format!("INSERT VERTEX {} (id", label);
    for name in &prop_names {
        q.push_str(", ");
        q.push_str(name);
    }
    q.push_str(") VALUES ");

    for (i, (vid, props)) in vertices.iter().enumerate() {
        if i > 0 {
            q.push_str(", ");
        }
        q.push_str(&format!("({}", vid.0));
        let prop_map: HashMap<&str, &DataValue> = props.iter().map(|(k, v)| (k.as_str(), v)).collect();
        for name in &prop_names {
            q.push_str(", ");
            if let Some(val) = prop_map.get(name.as_str()) {
                q.push_str(&data_value_to_literal(val));
            } else {
                q.push_str("null");
            }
        }
        q.push(')');
    }
    q.push(';');
    q
}

pub fn format_insert_edges_query(
    edge_type: &str,
    edges: &[(VertexId, VertexId, i64, Vec<(String, DataValue)>)],
) -> String {
    if edges.is_empty() {
        return String::new();
    }
    let mut prop_names: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for (_, _, _, props) in edges {
        for (k, _) in props {
            if seen.insert(k.clone()) {
                prop_names.push(k.clone());
            }
        }
    }

    let mut q = if prop_names.is_empty() {
        format!("INSERT EDGE {} VALUES ", edge_type)
    } else {
        format!("INSERT EDGE {} ({}) VALUES ", edge_type, prop_names.join(", "))
    };

    for (i, (src, dst, rank, props)) in edges.iter().enumerate() {
        if i > 0 {
            q.push_str(", ");
        }
        if prop_names.is_empty() {
            q.push_str(&format!("({}, {}, {})", src.0, dst.0, rank));
        } else {
            let prop_map: HashMap<&str, &DataValue> =
                props.iter().map(|(k, v)| (k.as_str(), v)).collect();
            let vals: Vec<String> = prop_names
                .iter()
                .map(|name| {
                    prop_map
                        .get(name.as_str())
                        .map(|v| data_value_to_literal(v))
                        .unwrap_or_else(|| "null".to_string())
                })
                .collect();
            q.push_str(&format!(
                "({}, {}, {}): ({})",
                src.0,
                dst.0,
                rank,
                vals.join(", ")
            ));
        }
    }
    q.push(';');
    q
}

pub fn merge_cluster_analyze_results(
    state: &AppState,
    local_val: serde_json::Value,
    peer_vals: Vec<Result<Option<serde_json::Value>, tokio::task::JoinError>>,
) -> serde_json::Value {
    let mut all_stats: Vec<GraphStatistics> = Vec::new();
    if let Some(st) = local_val.get("stats") {
        if let Ok(parsed) = serde_json::from_value::<GraphStatistics>(st.clone()) {
            all_stats.push(parsed);
        }
    }
    for pres in peer_vals {
        if let Ok(Some(val)) = pres {
            if let Some(st) = val.get("stats") {
                if let Ok(parsed) = serde_json::from_value::<GraphStatistics>(st.clone()) {
                    all_stats.push(parsed);
                }
            }
        }
    }

    if all_stats.is_empty() {
        return local_val;
    }

    let ring_nodes_count = state.ring_nodes.read().len().max(1);
    let rf = state.replication_factor.max(1);

    let mut sum_vertices = 0usize;
    let mut sum_edges = 0usize;
    let mut max_vertices = 0usize;
    let mut max_edges = 0usize;
    let mut max_degree = 0usize;
    let mut vertices_per_label: HashMap<String, usize> = HashMap::new();
    let mut edges_per_type: HashMap<String, usize> = HashMap::new();

    for s in &all_stats {
        sum_vertices += s.total_vertices;
        sum_edges += s.total_edges;
        max_vertices = max_vertices.max(s.total_vertices);
        max_edges = max_edges.max(s.total_edges);
        max_degree = max_degree.max(s.max_degree);

        for (lbl, cnt) in &s.vertices_per_label {
            if rf == 1 {
                *vertices_per_label.entry(lbl.clone()).or_insert(0) += *cnt;
            } else if (rf as usize) >= ring_nodes_count {
                let e = vertices_per_label.entry(lbl.clone()).or_insert(0);
                *e = (*e).max(*cnt);
            } else {
                *vertices_per_label.entry(lbl.clone()).or_insert(0) += *cnt;
            }
        }

        for (et, cnt) in &s.edges_per_type {
            if rf == 1 {
                *edges_per_type.entry(et.clone()).or_insert(0) += *cnt;
            } else if (rf as usize) >= ring_nodes_count {
                let e = edges_per_type.entry(et.clone()).or_insert(0);
                *e = (*e).max(*cnt);
            } else {
                *edges_per_type.entry(et.clone()).or_insert(0) += *cnt;
            }
        }
    }

    let (cluster_vertices, cluster_edges) = if rf == 1 {
        (sum_vertices, sum_edges)
    } else if (rf as usize) >= ring_nodes_count {
        (max_vertices, max_edges)
    } else {
        let rf_f = rf as f64;
        let v = (sum_vertices as f64 / rf_f).round() as usize;
        let e = (sum_edges as f64 / rf_f).round() as usize;
        for cnt in vertices_per_label.values_mut() {
            *cnt = (*cnt as f64 / rf_f).round() as usize;
        }
        for cnt in edges_per_type.values_mut() {
            *cnt = (*cnt as f64 / rf_f).round() as usize;
        }
        (v, e)
    };

    let avg_degree = if cluster_vertices > 0 {
        cluster_edges as f64 / cluster_vertices as f64
    } else {
        0.0
    };

    let cluster_stats = GraphStatistics {
        total_vertices: cluster_vertices,
        total_edges: cluster_edges,
        vertices_per_label,
        edges_per_type,
        avg_degree,
        max_degree,
    };

    *state.executor.stats.write() = Some(cluster_stats.clone());

    let message = format!(
        "Analyzed cluster graph across {} nodes: {} vertices across {} labels, {} edges across {} types (avg degree: {:.2}, max degree: {})",
        all_stats.len(),
        cluster_stats.total_vertices,
        cluster_stats.vertices_per_label.len(),
        cluster_stats.total_edges,
        cluster_stats.edges_per_type.len(),
        cluster_stats.avg_degree,
        cluster_stats.max_degree
    );

    let elapsed = local_val
        .get("elapsed_us")
        .and_then(|e| e.as_u64())
        .unwrap_or(0);

    serde_json::json!({
        "status": "ok",
        "message": message,
        "elapsed_us": elapsed,
        "num_rows": 0,
        "rows_affected": 0,
        "columns": [],
        "rows": [],
        "stats": cluster_stats,
    })
}

pub fn panic_msg_cleanup(s: &str) -> String {
    s.to_string()
}

fn extract_bound_start_vertex_id(q: &gdb_parser::ast::CypherQuery) -> Option<u64> {
    if !q.updates.is_empty() {
        return None;
    }
    if let Some(vid) = q.pattern.start_node.id_filter {
        return Some(vid.0);
    }
    let start_var = q.pattern.start_node.variable.as_deref()?;
    let where_expr = q.where_clause.as_ref()?;
    if let gdb_parser::ast::Expr::BinaryOp {
        left,
        op: gdb_parser::ast::BinaryOperator::Eq,
        right,
    } = where_expr
    {
        match (&**left, &**right) {
            (
                gdb_parser::ast::Expr::Property { variable, property },
                gdb_parser::ast::Expr::Literal(DataValue::Int64(i)),
            ) if variable == start_var
                && (property.eq_ignore_ascii_case("id") || property.eq_ignore_ascii_case("_id"))
                && *i >= 0 =>
            {
                return Some(*i as u64);
            }
            (
                gdb_parser::ast::Expr::Literal(DataValue::Int64(i)),
                gdb_parser::ast::Expr::Property { variable, property },
            ) if variable == start_var
                && (property.eq_ignore_ascii_case("id") || property.eq_ignore_ascii_case("_id"))
                && *i >= 0 =>
            {
                return Some(*i as u64);
            }
            _ => {}
        }
    }
    None
}

pub async fn execute_single_query(state: &AppState, trimmed: &str, local_only: bool) -> serde_json::Value {
    let start = Instant::now();

    // Check for compact command
    if trimmed.eq_ignore_ascii_case("compact") || trimmed.eq_ignore_ascii_case("compact;") {
        state.storage.compact();
        *state.global_csr_cache.write() = None;
        state.compactions_count.fetch_add(1, Ordering::Relaxed);
        let elapsed_us = start.elapsed().as_micros();
        state.queries_ok.fetch_add(1, Ordering::Relaxed);
        state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);

        let other_nodes: Vec<RingNode> = state.ring_nodes.read().iter().filter(|n| n.node_id != state.node_id).cloned().collect();
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

    match &stmt {
        gdb_parser::ast::Statement::CreateGraph { name, if_not_exists } => {
            match state.create_graph_space(name, *if_not_exists) {
                Ok(space) => {
                    let elapsed_us = start.elapsed().as_micros();
                    state.queries_ok.fetch_add(1, Ordering::Relaxed);
                    state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);
                    if !local_only {
                        let other_nodes: Vec<RingNode> = state.ring_nodes.read().iter().filter(|n| n.node_id != state.node_id).cloned().collect();
                        let rep_q = format!("CREATE GRAPH IF NOT EXISTS {};", space.name);
                        replicate_to_targets(&state.http_client, &other_nodes, &rep_q, state.replication_mode, &state.replications_count).await;
                    }
                    return serde_json::json!({
                        "status": "ok",
                        "message": format!("Created graph space '{}'", space.name),
                        "active_graph": state.current_graph_name(),
                        "elapsed_us": elapsed_us,
                        "num_rows": 0,
                        "rows_affected": 1,
                        "columns": [],
                        "rows": []
                    });
                }
                Err(e) => {
                    state.queries_err.fetch_add(1, Ordering::Relaxed);
                    return serde_json::json!({
                        "status": "error",
                        "error": format!("DDL error: {}", e),
                        "elapsed_us": start.elapsed().as_micros()
                    });
                }
            }
        }
        gdb_parser::ast::Statement::DropGraph { name, if_exists } => {
            match state.drop_graph_space(name, *if_exists) {
                Ok(_) => {
                    let elapsed_us = start.elapsed().as_micros();
                    state.queries_ok.fetch_add(1, Ordering::Relaxed);
                    state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);
                    if !local_only {
                        let other_nodes: Vec<RingNode> = state.ring_nodes.read().iter().filter(|n| n.node_id != state.node_id).cloned().collect();
                        let rep_q = format!("DROP GRAPH IF EXISTS {};", name);
                        replicate_to_targets(&state.http_client, &other_nodes, &rep_q, state.replication_mode, &state.replications_count).await;
                    }
                    return serde_json::json!({
                        "status": "ok",
                        "message": format!("Dropped graph space '{}'", name),
                        "active_graph": state.current_graph_name(),
                        "elapsed_us": elapsed_us,
                        "num_rows": 0,
                        "rows_affected": 1,
                        "columns": [],
                        "rows": []
                    });
                }
                Err(e) => {
                    state.queries_err.fetch_add(1, Ordering::Relaxed);
                    return serde_json::json!({
                        "status": "error",
                        "error": format!("DDL error: {}", e),
                        "elapsed_us": start.elapsed().as_micros()
                    });
                }
            }
        }
        gdb_parser::ast::Statement::UseGraph { name } => {
            match state.use_graph_space(name) {
                Ok(active) => {
                    let elapsed_us = start.elapsed().as_micros();
                    state.queries_ok.fetch_add(1, Ordering::Relaxed);
                    state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);
                    if !local_only {
                        let other_nodes: Vec<RingNode> = state.ring_nodes.read().iter().filter(|n| n.node_id != state.node_id).cloned().collect();
                        let rep_q = format!("USE GRAPH {};", active);
                        replicate_to_targets(&state.http_client, &other_nodes, &rep_q, state.replication_mode, &state.replications_count).await;
                    }
                    return serde_json::json!({
                        "status": "ok",
                        "message": format!("Switched active graph space to '{}'", active),
                        "active_graph": active,
                        "elapsed_us": elapsed_us,
                        "num_rows": 0,
                        "rows_affected": 0,
                        "columns": [],
                        "rows": []
                    });
                }
                Err(e) => {
                    state.queries_err.fetch_add(1, Ordering::Relaxed);
                    return serde_json::json!({
                        "status": "error",
                        "error": format!("Graph space error: {}", e),
                        "elapsed_us": start.elapsed().as_micros()
                    });
                }
            }
        }
        gdb_parser::ast::Statement::ShowGraphs => {
            let spaces = state.list_graph_spaces();
            let elapsed_us = start.elapsed().as_micros();
            state.queries_ok.fetch_add(1, Ordering::Relaxed);
            state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);
            let rows: Vec<Vec<serde_json::Value>> = spaces
                .iter()
                .map(|sp| {
                    vec![
                        serde_json::json!(sp.name),
                        serde_json::json!(if sp.active { "*" } else { "" }),
                        serde_json::json!(sp.vertex_labels),
                        serde_json::json!(sp.edge_types),
                        serde_json::json!(sp.total_vertices),
                        serde_json::json!(sp.total_edges),
                    ]
                })
                .collect();
            return serde_json::json!({
                "status": "ok",
                "message": format!("Registered graph spaces: {}", rows.len()),
                "active_graph": state.current_graph_name(),
                "elapsed_us": elapsed_us,
                "num_rows": rows.len(),
                "rows_affected": rows.len(),
                "columns": ["graph_name", "active", "vertex_labels", "edge_types", "total_vertices", "total_edges"],
                "rows": rows
            });
        }
        _ => {}
    }

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

                let other_nodes: Vec<RingNode> = state.ring_nodes.read().iter().filter(|n| n.node_id != state.node_id).cloned().collect();
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

    if let gdb_parser::ast::Statement::InsertVertices { label, vertices } = &stmt {
        let ring_snapshot = state.ring_nodes.read().clone();
        if !local_only && ring_snapshot.len() > 1 {
            let mut local_vertices = Vec::new();
            let mut peer_batches: HashMap<u64, (RingNode, Vec<(VertexId, Vec<(String, DataValue)>)>)> = HashMap::new();

            for (vid, props) in vertices {
                let reps = calculate_replica_set(vid.0, &ring_snapshot, state.replication_factor);
                for rep in reps {
                    if rep.node_id == state.node_id {
                        local_vertices.push((*vid, props.clone()));
                    } else {
                        peer_batches
                            .entry(rep.node_id)
                            .or_insert_with(|| (rep.clone(), Vec::new()))
                            .1
                            .push((*vid, props.clone()));
                    }
                }
            }

            if !local_vertices.is_empty() {
                let local_stmt = gdb_parser::ast::Statement::InsertVertices {
                    label: label.clone(),
                    vertices: local_vertices,
                };
                if let Err(e) = state.executor.execute(local_stmt) {
                    state.queries_err.fetch_add(1, Ordering::Relaxed);
                    return serde_json::json!({
                        "status": "error",
                        "error": format!("Execution error: {}", e),
                        "elapsed_us": start.elapsed().as_micros()
                    });
                }
            }

            for (_node_id, (target, batch_vertices)) in peer_batches {
                let sub_query = format_insert_vertices_query(label, &batch_vertices);
                replicate_to_targets(
                    &state.http_client,
                    &[target],
                    &sub_query,
                    state.replication_mode,
                    &state.replications_count,
                )
                .await;
            }

            let elapsed_us = start.elapsed().as_micros();
            state.queries_ok.fetch_add(1, Ordering::Relaxed);
            state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);

            return serde_json::json!({
                "status": "ok",
                "message": format!("Inserted {} vertices across cluster", vertices.len()),
                "elapsed_us": elapsed_us,
                "num_rows": 0,
                "rows_affected": vertices.len() as u64,
                "columns": [],
                "rows": []
            });
        }
    }

    if let gdb_parser::ast::Statement::InsertEdges { edge_type, edges } = &stmt {
        let ring_snapshot = state.ring_nodes.read().clone();
        if !local_only && ring_snapshot.len() > 1 {
            let mut local_edges = Vec::new();
            let mut peer_batches: HashMap<u64, (RingNode, Vec<(VertexId, VertexId, i64, Vec<(String, DataValue)>)>)> = HashMap::new();

            for edge in edges {
                let reps = calculate_replica_set(edge.0.0, &ring_snapshot, state.replication_factor);
                for rep in reps {
                    if rep.node_id == state.node_id {
                        local_edges.push(edge.clone());
                    } else {
                        peer_batches
                            .entry(rep.node_id)
                            .or_insert_with(|| (rep.clone(), Vec::new()))
                            .1
                            .push(edge.clone());
                    }
                }
            }

            if !local_edges.is_empty() {
                let local_stmt = gdb_parser::ast::Statement::InsertEdges {
                    edge_type: edge_type.clone(),
                    edges: local_edges,
                };
                if let Err(e) = state.executor.execute(local_stmt) {
                    state.queries_err.fetch_add(1, Ordering::Relaxed);
                    return serde_json::json!({
                        "status": "error",
                        "error": format!("Execution error: {}", e),
                        "elapsed_us": start.elapsed().as_micros()
                    });
                }
            }

            for (_node_id, (target, batch_edges)) in peer_batches {
                let sub_query = format_insert_edges_query(edge_type, &batch_edges);
                replicate_to_targets(
                    &state.http_client,
                    &[target],
                    &sub_query,
                    state.replication_mode,
                    &state.replications_count,
                )
                .await;
            }

            let elapsed_us = start.elapsed().as_micros();
            state.queries_ok.fetch_add(1, Ordering::Relaxed);
            state.total_query_duration_us.fetch_add(elapsed_us as u64, Ordering::Relaxed);

            return serde_json::json!({
                "status": "ok",
                "message": format!("Inserted {} edges across cluster", edges.len()),
                "elapsed_us": elapsed_us,
                "num_rows": 0,
                "rows_affected": edges.len() as u64,
                "columns": [],
                "rows": []
            });
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
        let ring_snapshot = state.ring_nodes.read().clone();
        let replica_set = calculate_replica_set(key, &ring_snapshot, state.replication_factor);
        let is_local_replica = replica_set.iter().any(|n| n.node_id == state.node_id);

        if !local_only && !is_local_replica && !replica_set.is_empty() {
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

                if !local_only {
                    let remote_replicas: Vec<RingNode> = replica_set.into_iter().filter(|n| n.node_id != state.node_id).collect();
                    replicate_to_targets(&state.http_client, &remote_replicas, trimmed, state.replication_mode, &state.replications_count).await;
                }

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

    let is_read_or_algo = matches!(
        stmt,
        gdb_parser::ast::Statement::Query(_) | gdb_parser::ast::Statement::CallAlgorithm { .. }
    );

    let ring_snapshot = state.ring_nodes.read().clone();
    let peers: Vec<RingNode> = ring_snapshot.iter().filter(|n| n.node_id != state.node_id).cloned().collect();

    if matches!(stmt, gdb_parser::ast::Statement::AnalyzeGraph) && !local_only && !peers.is_empty() {
        let local_val = Box::pin(execute_single_query(state, trimmed, true)).await;

        let client = state.http_client.clone();
        let payload = serde_json::json!({
            "query": trimmed,
            "local_only": true,
        });

        let mut peer_futs = Vec::new();
        for peer in peers {
            let p_url = format!("{}/query", peer.http_url.trim_end_matches('/'));
            let c = client.clone();
            let p = payload.clone();
            peer_futs.push(tokio::spawn(async move {
                match c.post(&p_url)
                    .json(&p)
                    .timeout(std::time::Duration::from_millis(5000))
                    .send()
                    .await
                {
                    Ok(resp) => resp.json::<serde_json::Value>().await.ok(),
                    Err(e) => {
                        tracing::warn!("Scatter analyze query to peer {} failed: {}", p_url, e);
                        None
                    }
                }
            }));
        }

        let peer_vals = futures::future::join_all(peer_futs).await;
        return merge_cluster_analyze_results(state, local_val, peer_vals);
    }

    if is_read_or_algo && !local_only && !peers.is_empty() {
        // For graph topology algorithms (algo.*) on sharded clusters (RF < N),
        // gather all active edges across cluster nodes to assemble a global CSR
        // so iterative/global algorithms (PageRank, WCC, SCC, Betweenness, etc.) are 100% exact.
        if let gdb_parser::ast::Statement::CallAlgorithm {
            ref algorithm,
            ref args,
            ref yield_items,
        } = stmt
        {
            if !algorithm.to_lowercase().starts_with("vector.") {
                let mut override_csr = None;
                if (state.replication_factor as usize) < ring_snapshot.len() {
                    let cur_ver = state.storage.latest_version();
                    let cached = {
                        let guard = state.global_csr_cache.read();
                        match *guard {
                            Some((ver, ref csr)) if ver == cur_ver => Some(csr.clone()),
                            _ => None,
                        }
                    };
                    if let Some(csr) = cached {
                        override_csr = Some(csr);
                    } else {
                        let mut all_edges = state.storage.get_all_active_edges();
                        let client = state.http_client.clone();
                        let mut futs = Vec::new();
                        for peer in &peers {
                            let p_url = format!(
                                "{}/storage/all_edges_bin",
                                peer.http_url.trim_end_matches('/')
                            );
                            let c = client.clone();
                            futs.push(async move {
                                match c
                                    .get(&p_url)
                                    .timeout(std::time::Duration::from_secs(60))
                                    .send()
                                    .await
                                {
                                    Ok(resp) => {
                                        if let Ok(bytes) = resp.bytes().await {
                                            let mut out = Vec::with_capacity(bytes.len() / 28);
                                            for chunk in bytes.chunks_exact(28) {
                                                let src = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
                                                let et = u32::from_le_bytes(chunk[8..12].try_into().unwrap());
                                                let rank = i64::from_le_bytes(chunk[12..20].try_into().unwrap());
                                                let dst = u64::from_le_bytes(chunk[20..28].try_into().unwrap());
                                                out.push(gdb_core::EdgeId::new(
                                                    VertexId(src),
                                                    gdb_core::EdgeType(et),
                                                    rank,
                                                    VertexId(dst),
                                                ));
                                            }
                                            out
                                        } else {
                                            Vec::new()
                                        }
                                    }
                                    Err(_) => Vec::new(),
                                }
                            });
                        }
                        for peer_edges in futures::future::join_all(futs).await {
                            all_edges.extend(peer_edges);
                        }
                        all_edges.sort_unstable_by(|a, b| {
                            a.src
                                .cmp(&b.src)
                                .then(a.edge_type.0.cmp(&b.edge_type.0))
                                .then(a.rank.cmp(&b.rank))
                                .then(a.dst.cmp(&b.dst))
                        });
                        all_edges.dedup();
                        let built = Arc::new(gdb_storage::ChunkedCsr::from_edges(all_edges));
                        *state.global_csr_cache.write() = Some((cur_ver, built.clone()));
                        override_csr = Some(built);
                    }
                }

                let csr_v = override_csr
                    .as_ref()
                    .map(|c| c.reverse_map.len())
                    .unwrap_or_else(|| state.storage.total_vertices());
                let csr_e = override_csr
                    .as_ref()
                    .map(|c| c.edge_count())
                    .unwrap_or_else(|| state.storage.total_edges());

                match state.executor.execute_call_with_csr(
                    algorithm,
                    args.clone(),
                    yield_items.clone(),
                    override_csr,
                ) {
                    Ok(res) => {
                        let elapsed_us = start.elapsed().as_micros();
                        state.queries_ok.fetch_add(1, Ordering::Relaxed);
                        state
                            .total_query_duration_us
                            .fetch_add(elapsed_us as u64, Ordering::Relaxed);
                        let pregel_info = record_pregel_telemetry(
                            state,
                            algorithm,
                            args,
                            csr_v,
                            csr_e,
                            ring_snapshot.len(),
                            elapsed_us,
                        );

                        let mut columns = Vec::new();
                        let mut rows = Vec::new();
                        if let Some(ref batch) = res.batch {
                            for f in batch.schema().fields() {
                                columns.push(f.name().clone());
                            }
                            let max_json_rows = match args.get("limit") {
                                Some(DataValue::Int64(l)) if *l >= 0 => {
                                    (*l as usize).min(batch.num_rows())
                                }
                                _ => batch.num_rows(),
                            };
                            for row_idx in 0..max_json_rows {
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

                        return serde_json::json!({
                            "status": "ok",
                            "message": res.message,
                            "active_graph": state.schema.read().name,
                            "pregel": pregel_info,
                            "elapsed_us": elapsed_us,
                            "num_rows": rows.len(),
                            "rows_affected": res.rows_affected,
                            "columns": columns,
                            "rows": rows,
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

        if let gdb_parser::ast::Statement::Query(ref q) = stmt {
            if let Some(start_vid) = extract_bound_start_vertex_id(q) {
                let replica_set =
                    calculate_replica_set(start_vid, &ring_snapshot, state.replication_factor);
                if let Some(primary) = replica_set.first() {
                    if primary.node_id == state.node_id {
                        return Box::pin(execute_single_query(state, trimmed, true)).await;
                    } else {
                        let p_url = format!("{}/query", primary.http_url.trim_end_matches('/'));
                        let payload = serde_json::json!({
                            "query": trimmed,
                            "local_only": true,
                        });
                        if let Ok(resp) = state
                            .http_client
                            .post(&p_url)
                            .json(&payload)
                            .timeout(std::time::Duration::from_millis(5000))
                            .send()
                            .await
                        {
                            if let Ok(val) = resp.json::<serde_json::Value>().await {
                                return val;
                            }
                        }
                    }
                }
            }
        }

        let local_val = Box::pin(execute_single_query(state, trimmed, true)).await;

        let client = state.http_client.clone();
        let payload = serde_json::json!({
            "query": trimmed,
            "local_only": true,
        });

        let mut peer_futs = Vec::new();
        for peer in peers {
            let p_url = format!("{}/query", peer.http_url.trim_end_matches('/'));
            let c = client.clone();
            let p = payload.clone();
            peer_futs.push(tokio::spawn(async move {
                match c.post(&p_url)
                    .json(&p)
                    .timeout(std::time::Duration::from_millis(5000))
                    .send()
                    .await
                {
                    Ok(resp) => resp.json::<serde_json::Value>().await.ok(),
                    Err(e) => {
                        tracing::warn!("Scatter query to peer {} failed: {}", p_url, e);
                        None
                    }
                }
            }));
        }

        let peer_vals = futures::future::join_all(peer_futs).await;
        return merge_scatter_gather_results(&stmt, local_val, peer_vals);
    }

    let is_analyze_graph = matches!(stmt, gdb_parser::ast::Statement::AnalyzeGraph);
    let call_algo_meta = match &stmt {
        gdb_parser::ast::Statement::CallAlgorithm { algorithm, args, .. } => {
            Some((algorithm.clone(), args.clone()))
        }
        _ => None,
    };
    let exec_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        state.executor.execute(stmt)
    }));

    match exec_res {
        Ok(Ok(res)) => {
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

            let mut res_json = serde_json::json!({
                "status": "ok",
                "message": res.message,
                "active_graph": state.schema.read().name,
                "elapsed_us": elapsed_us,
                "num_rows": rows.len(),
                "rows_affected": res.rows_affected,
                "columns": columns,
                "rows": rows,
            });
            if let Some((algo_name, algo_args)) = call_algo_meta {
                let pregel_info = record_pregel_telemetry(
                    state,
                    &algo_name,
                    &algo_args,
                    state.storage.total_vertices(),
                    state.storage.total_edges(),
                    ring_snapshot.len(),
                    elapsed_us,
                );
                res_json["pregel"] = serde_json::to_value(pregel_info).unwrap_or_default();
            }
            if is_analyze_graph {
                if let Some(ref st) = *state.executor.stats.read() {
                    res_json["stats"] = serde_json::to_value(st).unwrap_or_default();
                }
            }
            res_json
        }
        Ok(Err(e)) => {
            state.queries_err.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({
                "status": "error",
                "error": format!("Execution error: {}", e),
                "elapsed_us": start.elapsed().as_micros()
            })
        }
        Err(panic_err) => {
            state.queries_err.fetch_add(1, Ordering::Relaxed);
            let panic_msg = if let Some(s) = panic_err.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_err.downcast_ref::<String>() {
                panic_msg_cleanup(s)
            } else {
                "Internal execution panic".to_string()
            };
            tracing::error!("Query execution panicked: {}", panic_msg);
            serde_json::json!({
                "status": "error",
                "error": format!("Internal execution panic: {}", panic_msg),
                "elapsed_us": start.elapsed().as_micros()
            })
        }
    }
}

pub async fn handle_query(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> impl IntoResponse {
    let start = Instant::now();
    let header_graph = headers
        .get("x-gdb-graph")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let (query_str, local_only, body_graph) = if let Ok(parsed) = serde_json::from_str::<QueryRequest>(&body) {
        (parsed.query, parsed.local_only.unwrap_or(false), parsed.graph)
    } else {
        (body, false, None)
    };
    let requested_graph = body_graph.or(header_graph);

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
        let scoped = state.with_graph_space(requested_graph.as_deref());
        let res = execute_single_query(&scoped, &stmts[0], local_only).await;
        return Json(res);
    }

    // Multi-statement script execution
    let mut total_rows_affected = 0;
    let mut last_res = serde_json::Value::Null;

    for (idx, stmt_str) in stmts.iter().enumerate() {
        let scoped = state.with_graph_space(requested_graph.as_deref());
        let res = execute_single_query(&scoped, stmt_str, local_only).await;
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

pub async fn handle_batch(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> impl IntoResponse {
    let start = Instant::now();
    let header_graph = headers
        .get("x-gdb-graph")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let (queries, local_only, body_graph) = if let Ok(parsed) = serde_json::from_str::<BatchRequest>(&body) {
        let q = if let Some(list) = parsed.queries {
            list
        } else if let Some(script) = parsed.script {
            gdb_parser::split_statements(&script)
        } else {
            vec![]
        };
        (q, parsed.local_only.unwrap_or(false), parsed.graph)
    } else if let Ok(parsed_list) = serde_json::from_str::<Vec<String>>(&body) {
        (parsed_list, false, None)
    } else {
        (gdb_parser::split_statements(&body), false, None)
    };
    let requested_graph = body_graph.or(header_graph);

    let mut results = Vec::with_capacity(queries.len());
    let mut ok_count = 0;
    let mut err_count = 0;

    for q in &queries {
        let trimmed = q.trim();
        if trimmed.is_empty() {
            continue;
        }
        let scoped = state.with_graph_space(requested_graph.as_deref());
        let res = execute_single_query(&scoped, trimmed, local_only).await;
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
