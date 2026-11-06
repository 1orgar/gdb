use clap::Parser;
use gdb_core::schema::GraphSchema;
use gdb_planner::QueryExecutor;
use gdb_storage::PartitionStorageEngine;
use parking_lot::RwLock;
use reedline::{
    ColumnarMenu, Completer, Emacs, KeyCode, KeyModifiers, MenuBuilder, Prompt, PromptEditMode,
    PromptHistorySearch, PromptHistorySearchStatus, Reedline, ReedlineEvent, ReedlineMenu, Signal,
    Span, Suggestion, default_emacs_keybindings,
};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(author, version, about = "GDB CLI: Interactive & Batch Cypher Console", long_about = None)]
struct CliArgs {
    /// GDB Cluster HTTP REST endpoint URL
    #[arg(long, default_value = "http://127.0.0.1:8847")]
    endpoint: String,

    /// Target Graph Space / Namespace
    #[arg(short = 'g', long, default_value = "default")]
    graph: String,

    /// Execute a single query and exit
    #[arg(short = 'e', long = "execute")]
    execute: Option<String>,

    /// Execute queries from a file line-by-line and exit
    #[arg(short, long)]
    file: Option<PathBuf>,

    /// Run in standalone local in-memory mode without connecting to cluster
    #[arg(long)]
    local: bool,
}

struct GdbPrompt {
    active_graph: Arc<RwLock<String>>,
}

impl Prompt for GdbPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Owned(format!("gdb ({})", self.active_graph.read()))
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, _edit_mode: PromptEditMode) -> Cow<'_, str> {
        Cow::Borrowed("> ")
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed("... ")
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<'_, str> {
        let prefix = match history_search.status {
            PromptHistorySearchStatus::Passing => "",
            PromptHistorySearchStatus::Failing => "failing ",
        };
        Cow::Owned(format!("({}reverse-search: {}) ", prefix, history_search.term))
    }
}

pub struct GdbCompleter {
    dynamic_terms: Arc<RwLock<Vec<String>>>,
}

impl GdbCompleter {
    pub fn new(dynamic_terms: Arc<RwLock<Vec<String>>>) -> Self {
        Self { dynamic_terms }
    }

    fn base_keywords() -> &'static [&'static str] {
        &[
            "MATCH",
            "WHERE",
            "RETURN",
            "WITH",
            "ORDER BY",
            "LIMIT",
            "SKIP",
            "DISTINCT",
            "CREATE VERTEX",
            "CREATE EDGE",
            "CREATE GRAPH",
            "CREATE INDEX ON",
            "CREATE VECTOR INDEX ON",
            "USE GRAPH",
            "DROP GRAPH",
            "DROP VERTEX",
            "DROP EDGE",
            "DROP INDEX ON",
            "SHOW GRAPHS",
            "SHOW SCHEMA",
            "SHOW VERTICES",
            "SHOW EDGES",
            "SHOW CLUSTER",
            "SHOW RESOURCES",
            "SHOW GPU",
            "INSERT VERTEX",
            "INSERT EDGE",
            "DELETE EDGE",
            "MERGE VERTEX",
            "ANALYZE GRAPH",
            "EXPLAIN",
            "CALL algo.pageRank",
            "CALL algo.louvain",
            "CALL algo.wcc",
            "CALL algo.scc",
            "CALL algo.triangleCount",
            "CALL algo.kCore",
            "CALL algo.betweenness",
            "CALL algo.closeness",
            "CALL algo.degree",
            "CALL algo.sssp",
            "CALL algo.bfs",
            "CALL algo.lpa",
            "CALL algo.similarity",
            "CALL algo.node2vec",
            "CALL vector.similaritySearch",
            "YIELD",
            "VALUES",
            "FROM",
            "TO",
            "compact",
            "snapshot",
            "help",
            "exit",
            "quit",
        ]
    }
}

impl Completer for GdbCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let slice = &line[..pos.min(line.len())];
        let trimmed_start_idx = slice.len() - slice.trim_start().len();
        let full_prefix = &slice[trimmed_start_idx..];

        let word_start = slice
            .rfind(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ':' | ',' | '[' | ']'))
            .map(|i| i + 1)
            .unwrap_or(0);
        let word_prefix = &slice[word_start..];
        if full_prefix.is_empty() && word_prefix.is_empty() {
            return Vec::new();
        }
        let full_lower = full_prefix.to_lowercase();
        let word_lower = word_prefix.to_lowercase();

        let mut candidates: Vec<String> = Self::base_keywords()
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        candidates.extend(self.dynamic_terms.read().iter().cloned());
        candidates.sort();
        candidates.dedup();

        let mut out = Vec::new();
        for c in candidates {
            let c_lower = c.to_lowercase();
            if !full_lower.is_empty() && c.contains(' ') && c_lower.starts_with(&full_lower) {
                out.push(Suggestion {
                    value: c,
                    description: None,
                    style: None,
                    extra: None,
                    span: Span::new(trimmed_start_idx, pos),
                    append_whitespace: true,
                });
            } else if !word_lower.is_empty() && c_lower.starts_with(&word_lower) {
                out.push(Suggestion {
                    value: c,
                    description: None,
                    style: None,
                    extra: None,
                    span: Span::new(word_start, pos),
                    append_whitespace: true,
                });
            }
        }
        out
    }
}

enum ClientMode {
    Cluster {
        endpoint: String,
        client: reqwest::blocking::Client,
        active_graph: Arc<RwLock<String>>,
    },
    Local {
        executor: QueryExecutor,
        storage: Arc<PartitionStorageEngine>,
        active_graph: Arc<RwLock<String>>,
        spaces: HashMap<String, (Arc<RwLock<GraphSchema>>, Arc<PartitionStorageEngine>, QueryExecutor)>,
    },
}

fn print_ascii_table(columns: &[String], rows: &[Vec<Value>]) {
    if columns.is_empty() {
        return;
    }

    let mut col_widths: Vec<usize> = columns.iter().map(|c| c.len().max(4)).collect();

    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < col_widths.len() {
                let cell_str = format_cell(cell);
                if cell_str.len() > col_widths[i] {
                    col_widths[i] = cell_str.len();
                }
            }
        }
    }

    // Top border
    let sep: String = col_widths
        .iter()
        .map(|w| format!("+-{}-", "-".repeat(*w)))
        .collect::<Vec<_>>()
        .join("")
        + "+";
    println!("\x1b[1;30m{}\x1b[0m", sep);

    // Headers
    let header_line: String = columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!("| \x1b[1;36m{:<width$}\x1b[0m ", c, width = col_widths[i]))
        .collect::<Vec<_>>()
        .join("")
        + "|";
    println!("{}", header_line);

    // Header-row separator
    let head_sep: String = col_widths
        .iter()
        .map(|w| format!("+={}=+", "=".repeat(*w)))
        .collect::<Vec<_>>()
        .join("")
        + "+";
    println!("\x1b[1;30m{}\x1b[0m", head_sep);

    // Data rows
    for row in rows {
        let row_line: String = row
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let w = if i < col_widths.len() { col_widths[i] } else { 8 };
                let val = format_cell(cell);
                format!("| {:<width$} ", val, width = w)
            })
            .collect::<Vec<_>>()
            .join("")
            + "|";
        println!("{}", row_line);
    }

    // Bottom border
    println!("\x1b[1;30m{}\x1b[0m", sep);
}

fn format_cell(val: &Value) -> String {
    match val {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            if arr.len() <= 6 {
                let items: Vec<String> = arr
                    .iter()
                    .map(|v| match v {
                        Value::Number(n) => n.to_string(),
                        Value::String(s) => format!("'{}'", s),
                        other => other.to_string(),
                    })
                    .collect();
                format!("[{}]", items.join(", "))
            } else {
                format!("[{} items: {}, {}, ...]", arr.len(), arr[0], arr[1])
            }
        }
        Value::Object(_) => format!("{}", val),
    }
}

fn show_cluster(client: &reqwest::blocking::Client, current_endpoint: &str) {
    println!("\n\x1b[1;36m=== GDB Leaderless Ring Cluster Topology ===\x1b[0m");

    let cluster_url = format!("{}/cluster", current_endpoint.trim_end_matches('/'));
    let mut rf_str = "3".to_string();
    let mut mode_str = "SYNC".to_string();
    let mut ring_nodes_opt: Option<Vec<Value>> = None;

    if let Ok(resp) = client.get(&cluster_url).timeout(Duration::from_millis(600)).send() {
        if let Ok(info) = resp.json::<Value>() {
            rf_str = info["replication_factor"].to_string();
            mode_str = info["replication_mode"].as_str().unwrap_or("SYNC").to_uppercase();
            ring_nodes_opt = info.get("ring_nodes").and_then(|v| v.as_array().cloned());
        }
    }

    println!("  Topology: \x1b[1;32mLeaderless Hash Ring\x1b[0m | Replication Factor: \x1b[1;33mRF={}\x1b[0m | Mode: \x1b[1;35m{}\x1b[0m\n", rf_str, mode_str);

    let candidate_endpoints: Vec<(String, String, u16, u64)> = if let Some(nodes) = ring_nodes_opt {
        nodes.iter().map(|n| {
            let id = n["node_id"].as_u64().unwrap_or(1);
            let url = n["http_url"].as_str().unwrap_or("").to_string();
            let flight = n["flight_port"].as_u64().unwrap_or(8848) as u16;
            (format!("Peer Node #{}", id), url, flight, id)
        }).collect()
    } else {
        vec![
            ("Peer Node #1".to_string(), "http://127.0.0.1:8847".to_string(), 8848, 1),
            ("Peer Node #2".to_string(), "http://127.0.0.1:8846".to_string(), 8849, 2),
            ("Peer Node #3".to_string(), "http://127.0.0.1:8845".to_string(), 8850, 3),
        ]
    };

    let total_nodes = candidate_endpoints.len();

    let cols = vec![
        "Node".to_string(),
        "Role".to_string(),
        "Ring Token Range".to_string(),
        "HTTP Endpoint".to_string(),
        "Flight Port".to_string(),
        "Status".to_string(),
        "Latency".to_string(),
    ];
    let mut rows: Vec<Vec<Value>> = Vec::new();

    for (label, url, flight_port, node_id) in candidate_endpoints {
        let target_url = if (url == "http://127.0.0.1:8847" || url == "http://localhost:8847") && current_endpoint != url {
            current_endpoint.to_string()
        } else {
            url
        };

        let start = std::time::Instant::now();
        let health_url = format!("{}/health", target_url.trim_end_matches('/'));
        let token_desc = format!("u % {} == {}", total_nodes, node_id.saturating_sub(1));

        match client.get(&health_url).timeout(Duration::from_millis(600)).send() {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed();
                let is_current = if target_url == current_endpoint { " (active)" } else { "" };

                rows.push(vec![
                    Value::String(format!("{}{}", label, is_current)),
                    Value::String("Peer".to_string()),
                    Value::String(token_desc),
                    Value::String(target_url),
                    Value::Number(flight_port.into()),
                    Value::String("\x1b[1;32mUP\x1b[0m".to_string()),
                    Value::String(format!("{:.2} ms", elapsed.as_secs_f64() * 1000.0)),
                ]);
            }
            _ => {
                rows.push(vec![
                    Value::String(label),
                    Value::String("Peer".to_string()),
                    Value::String(token_desc),
                    Value::String(target_url),
                    Value::Number(flight_port.into()),
                    Value::String("\x1b[1;31mDOWN\x1b[0m".to_string()),
                    Value::String("timeout".to_string()),
                ]);
            }
        }
    }

    print_ascii_table(&cols, &rows);
    println!();
}

fn show_resources(client: &reqwest::blocking::Client, endpoint: &str) {
    let url = format!("{}/resources", endpoint.trim_end_matches('/'));
    println!("\n\x1b[1;36m=== GDB Resource & Storage Consumption ===\x1b[0m");

    match client.get(&url).timeout(Duration::from_secs(2)).send() {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(data) = resp.json::<Value>() {
                let cols = vec!["Resource Metric".to_string(), "Value".to_string()];
                let mem_mb = data["estimated_memory_bytes"].as_f64().unwrap_or(0.0) / 1024.0 / 1024.0;
                let uptime_sec = data["uptime_seconds"].as_u64().unwrap_or(0);
                let uptime_fmt = format!("{}m {}s", uptime_sec / 60, uptime_sec % 60);
                let arch_str = data["cluster_mode"].as_str().unwrap_or("Leaderless Ring");
                let rf_val = data["replication_factor"].to_string();
                let mode_val = data["replication_mode"].as_str().unwrap_or("SYNC");
                let s3_str = if data["s3_configured"].as_bool().unwrap_or(false) {
                    format!("Enabled (bucket: {})", data["s3_bucket"].as_str().unwrap_or(""))
                } else {
                    "Disabled".into()
                };

                let rows = vec![
                    vec![Value::String("Node Role".into()), Value::String(format!("Node #{} (Peer)", data["node_id"]))],
                    vec![Value::String("Cluster Architecture".into()), Value::String(arch_str.to_string())],
                    vec![Value::String("Replication Factor".into()), Value::String(format!("RF = {}", rf_val))],
                    vec![Value::String("Replication Mode".into()), Value::String(mode_val.to_string())],
                    vec![Value::String("Total Vertices (CSR)".into()), Value::String(data["total_vertices"].to_string())],
                    vec![Value::String("Total Active Edges".into()), Value::String(data["total_edges"].to_string())],
                    vec![Value::String("Chunked-CSR Edges".into()), Value::String(data["csr_edges"].to_string())],
                    vec![Value::String("Delta MemTable Edges".into()), Value::String(data["memtable_edges"].to_string())],
                    vec![Value::String("Background Compactions".into()), Value::String(data["compactions_total"].to_string())],
                    vec![Value::String("Total Queries Executed".into()), Value::String(format!("{} (ok: {}, err: {})", data["queries_total"], data["queries_ok"], data["queries_error"]))],
                    vec![Value::String("Process Uptime".into()), Value::String(uptime_fmt)],
                    vec![Value::String("Estimated In-Memory RAM".into()), Value::String(format!("{:.2} MB", mem_mb))],
                    vec![Value::String("S3 Tiered Storage".into()), Value::String(s3_str)],
                ];

                print_ascii_table(&cols, &rows);
                println!();
                return;
            }
        }
        _ => {}
    }
    eprintln!("\x1b[1;31m[!] Failed to retrieve resource usage from {}\x1b[0m\n", endpoint);
}

fn show_gpu(client: &reqwest::blocking::Client, endpoint: &str) {
    let url = format!("{}/gpu", endpoint.trim_end_matches('/'));
    println!("\n\x1b[1;36m=== GDB Hardware Acceleration & GPU Status ===\x1b[0m");

    match client.get(&url).timeout(Duration::from_secs(2)).send() {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(data) = resp.json::<Value>() {
                let is_enabled = data["enabled"].as_bool().unwrap_or(false);
                let status_str = if is_enabled {
                    "\x1b[1;32mActive & Ready\x1b[0m"
                } else {
                    "\x1b[1;33mDisabled (pass --enable-gpu to activate)\x1b[0m"
                };
                let device_id = data["device_id"].as_u64().unwrap_or(0);
                let backend = data["backend"].as_str().unwrap_or("Apple Metal");
                let threshold = data["threshold_edges"].as_u64().unwrap_or(10_000);
                let mem_arch = data["memory_model"].as_str().unwrap_or("UMA Zero-Copy");

                let cols = vec!["GPU Attribute".to_string(), "Status / Value".to_string()];
                let rows = vec![
                    vec![Value::String("Hardware Status".into()), Value::String(status_str.into())],
                    vec![Value::String("Compute Accelerator".into()), Value::String(backend.into())],
                    vec![Value::String("Selected Device ID".into()), Value::String(format!("#{}", device_id))],
                    vec![Value::String("Offload Threshold".into()), Value::String(format!("{} edges", threshold))],
                    vec![Value::String("Memory Architecture".into()), Value::String(mem_arch.into())],
                    vec![Value::String("Supported Kernels".into()), Value::String("Parallel BFS Frontier, PageRank MSL/CUDA, Similarity".into())],
                ];

                print_ascii_table(&cols, &rows);
                println!();
                return;
            }
        }
        _ => {}
    }
    eprintln!("\x1b[1;31m[!] Failed to retrieve GPU status from {}\x1b[0m\n", endpoint);
}

fn execute_query(
    mode: &mut ClientMode,
    input: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let input = input.trim();
    if input.is_empty() || input.starts_with("//") || input.starts_with("#") {
        return Ok(());
    }

    // System inspection commands
    let upper = input.trim_end_matches(';').trim().to_uppercase();
    if upper == "SHOW CLUSTER" || upper == ":CLUSTER" || upper == "CLUSTER" {
        match mode {
            ClientMode::Cluster { client, endpoint, .. } => show_cluster(client, endpoint),
            ClientMode::Local { .. } => {
                println!("\x1b[1;33mCurrently in local standalone mode. Use ':connect <url>' to attach to a cluster.\x1b[0m");
            }
        }
        return Ok(());
    }

    if upper == "SHOW RESOURCES" || upper == ":RESOURCES" || upper == "RESOURCES" {
        match mode {
            ClientMode::Cluster { client, endpoint, .. } => show_resources(client, endpoint),
            ClientMode::Local { storage, active_graph, .. } => {
                println!("\n\x1b[1;36m=== Local Storage Resources (Graph: {}) ===\x1b[0m", active_graph.read());
                println!("  CSR Edges:       {}", storage.csr_edges());
                println!("  MemTable Edges:  {}", storage.delta_edges());
                println!("  Total Edges:     {}\n", storage.total_edges());
            }
        }
        return Ok(());
    }

    if upper == "SHOW GPU" || upper == ":GPU" || upper == "GPU" {
        match mode {
            ClientMode::Cluster { client, endpoint, .. } => show_gpu(client, endpoint),
            ClientMode::Local { .. } => {
                let disp = gdb_gpu::GpuDispatcher::disabled();
                println!("\n\x1b[1;36m=== Local Hardware Acceleration ===\x1b[0m");
                println!("  Status:     Disabled (default)");
                println!("  Backend:    {}", disp.backend_name());
                println!("  Device:     #{}", disp.device_id);
                println!("  Threshold:  {} edges\n", disp.threshold_edges);
            }
        }
        return Ok(());
    }

    if input.starts_with(":connect ") {
        let mut raw_url = input.trim_start_matches(":connect ").trim().to_string();
        if raw_url.contains("::") && !raw_url.contains('[') {
            println!("\x1b[1;33m[*] Notice: Detected '::' typo in URL, normalizing to single ':'\x1b[0m");
            raw_url = raw_url.replace("::", ":");
        }
        if !raw_url.starts_with("http://") && !raw_url.starts_with("https://") {
            raw_url = format!("http://{}", raw_url);
        }
        let clean_url = raw_url.trim_end_matches('/').to_string();
        let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(3)).build()?;
        let health_url = format!("{}/health", clean_url);

        match client.get(&health_url).send() {
            Ok(resp) if resp.status().is_success() => {
                println!("\x1b[1;32m[✓] Successfully connected to GDB Cluster at {}\x1b[0m", clean_url);
                let prev_graph = match mode {
                    ClientMode::Cluster { active_graph, .. } => active_graph.clone(),
                    ClientMode::Local { active_graph, .. } => active_graph.clone(),
                };
                *mode = ClientMode::Cluster {
                    endpoint: clean_url,
                    client,
                    active_graph: prev_graph,
                };
            }
            Ok(resp) => {
                eprintln!("\x1b[1;31m[!] Failed to connect to cluster at {}: HTTP {}\x1b[0m", clean_url, resp.status());
            }
            Err(e) => {
                eprintln!("\x1b[1;31m[!] Failed to connect to cluster at {}: {}\x1b[0m", clean_url, e);
            }
        }
        return Ok(());
    }

    match mode {
        ClientMode::Cluster { client, endpoint, active_graph } => {
            let start = std::time::Instant::now();
            let query_url = format!("{}/query", endpoint.trim_end_matches('/'));
            let cur_graph = active_graph.read().clone();
            let payload = serde_json::json!({
                "query": input,
                "graph": cur_graph,
            });

            let resp = client
                .post(&query_url)
                .header("X-GDB-Graph", &cur_graph)
                .json(&payload)
                .send()?;

            let data: Value = resp.json()?;
            let client_elapsed = start.elapsed();

            if data["status"] == "ok" {
                if let Some(new_active) = data.get("active_graph").and_then(|v| v.as_str()) {
                    if !new_active.is_empty() {
                        *active_graph.write() = new_active.to_string();
                    }
                }
                let server_us = data["elapsed_us"].as_u64().unwrap_or(0);
                let columns: Vec<String> = data["columns"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();

                let rows: Vec<Vec<Value>> = data["rows"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|r| r.as_array().cloned())
                            .collect()
                    })
                    .unwrap_or_default();

                if !columns.is_empty() && !rows.is_empty() {
                    print_ascii_table(&columns, &rows);
                }

                let num_rows = data["num_rows"].as_u64().unwrap_or(rows.len() as u64);
                let msg = data["message"].as_str().unwrap_or("Query executed");
                println!(
                    "\x1b[1;32m[✓] {} ({} rows in set, server: {:.2}ms, roundtrip: {:?})\x1b[0m",
                    msg,
                    num_rows,
                    server_us as f64 / 1000.0,
                    client_elapsed
                );
            } else {
                let err = data["error"].as_str().unwrap_or("Unknown server error");
                eprintln!("\x1b[1;31mError: {}\x1b[0m", err);
            }
        }
        ClientMode::Local { executor, storage, active_graph, spaces } => {
            if input.eq_ignore_ascii_case("compact") || input.eq_ignore_ascii_case("compact;") {
                let start = std::time::Instant::now();
                storage.compact();
                println!("\x1b[1;32m[✓] Local compaction completed in {:?}\x1b[0m", start.elapsed());
                return Ok(());
            }

            let start = std::time::Instant::now();
            let stmts = gdb_parser::split_statements(input);
            for stmt_str in stmts {
                let stmt = gdb_parser::parse(&stmt_str)?;
                match &stmt {
                    gdb_parser::ast::Statement::CreateGraph { name, .. } => {
                        if !spaces.contains_key(name) {
                            let sc = Arc::new(RwLock::new(GraphSchema::new(name)));
                            let st = Arc::new(PartitionStorageEngine::new(0, sc.clone()));
                            let ex = QueryExecutor::new(sc.clone(), st.clone());
                            spaces.insert(name.clone(), (sc, st, ex));
                        }
                        println!("\x1b[1;32mCreated graph space '{}' (took {:?})\x1b[0m", name, start.elapsed());
                        continue;
                    }
                    gdb_parser::ast::Statement::UseGraph { name } => {
                        if let Some((sc, st, _)) = spaces.get(name) {
                            *active_graph.write() = name.clone();
                            *storage = st.clone();
                            *executor = QueryExecutor::new(sc.clone(), st.clone());
                            println!("\x1b[1;32mSwitched active graph space to '{}' (took {:?})\x1b[0m", name, start.elapsed());
                            continue;
                        } else {
                            return Err(format!("Graph space '{}' does not exist", name).into());
                        }
                    }
                    gdb_parser::ast::Statement::DropGraph { name, .. } => {
                        if name == "default" {
                            return Err("Cannot drop default graph space".into());
                        }
                        spaces.remove(name);
                        if *active_graph.read() == *name {
                            *active_graph.write() = "default".to_string();
                        }
                        println!("\x1b[1;32mDropped graph space '{}' (took {:?})\x1b[0m", name, start.elapsed());
                        continue;
                    }
                    _ => {}
                }
                let res = executor.execute(stmt)?;
                if let Some(batch) = res.batch {
                    arrow::util::pretty::print_batches(&[batch])?;
                }
                println!("\x1b[1;32m{} (took {:?})\x1b[0m", res.message, start.elapsed());
            }
        }
    }

    Ok(())
}

pub(crate) fn handle_repl_input(
    mode: &mut ClientMode,
    input: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(true);
    }
    if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
        println!("Goodbye!");
        return Ok(false);
    }
    if input.eq_ignore_ascii_case("help") {
        println!("\x1b[1;33mSystem & Cluster Commands:\x1b[0m");
        println!("  SHOW CLUSTER              - Display cluster topology, roles & node latencies");
        println!("  SHOW RESOURCES            - Display in-memory usage (CSR, MemTable, Vertices, Edges)");
        println!("  SHOW GPU                  - Display hardware GPU acceleration status & kernels");
        println!("  SHOW GRAPHS               - List all Graph Spaces / Namespaces");
        println!("  CREATE GRAPH <name>       - Create isolated Graph Space");
        println!("  USE GRAPH <name>          - Switch active Graph Space in session");
        println!("  DROP GRAPH <name>         - Drop Graph Space");
        println!("  :connect <http://host:port> - Switch target cluster endpoint");
        println!("  compact                   - Force CSR compaction on target");
        println!("  exit / quit               - Exit console");
        println!("\n\x1b[1;33mGraph DDL & DML Commands:\x1b[0m");
        println!("  CREATE VERTEX <Label> (<prop> <type>, ...)");
        println!("  CREATE EDGE <Type> ()");
        println!("  INSERT VERTEX <Label> (id, <props>...) VALUES (<id>, <values>...)");
        println!("  INSERT EDGE <Type> FROM <src> TO <dst>");
        println!("  MATCH (a)-[:TYPE]->(b) WHERE ... RETURN ...");
        println!("\n\x1b[1;33mGraph Analytics (Nebula Enterprise Suite):\x1b[0m");
        println!("  CALL algo.pageRank({{damping: 0.85}}) YIELD vertex_id, score");
        println!("  CALL algo.louvain() YIELD vertex_id, community_id");
        println!("  CALL algo.wcc() YIELD vertex_id, component_id");
        println!("  CALL algo.scc() YIELD vertex_id, component_id");
        println!("  CALL algo.triangleCount() YIELD vertex_id, triangles");
        println!("  CALL algo.kCore() YIELD vertex_id, coreness");
        println!("  CALL algo.betweenness() YIELD vertex_id, betweenness");
        println!("  CALL algo.closeness() YIELD vertex_id, closeness");
        println!("  CALL algo.degree() YIELD vertex_id, in_degree, out_degree");
        println!("  CALL algo.sssp({{source: 1}}) YIELD vertex_id, distance");
        println!("  CALL algo.similarity({{node1: 1, node2: 2}}) YIELD jaccard, common_neighbors\n");
        return Ok(true);
    }

    if let Err(e) = execute_query(mode, input) {
        eprintln!("\x1b[1;31mError: {}\x1b[0m\n", e);
    } else {
        println!();
    }
    Ok(true)
}

fn new_local_mode(graph_name: &str) -> (ClientMode, Arc<RwLock<String>>) {
    let active_graph = Arc::new(RwLock::new(graph_name.to_string()));
    let schema = Arc::new(RwLock::new(GraphSchema::new(graph_name)));
    let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
    let executor = QueryExecutor::new(schema.clone(), storage.clone());
    let mut spaces = HashMap::new();
    spaces.insert(
        graph_name.to_string(),
        (
            schema.clone(),
            storage.clone(),
            QueryExecutor::new(schema, storage.clone()),
        ),
    );
    (
        ClientMode::Local {
            executor,
            storage,
            active_graph: active_graph.clone(),
            spaces,
        },
        active_graph,
    )
}

pub(crate) fn run_cli(args: CliArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("\x1b[1;36m");
    println!("   ______ ____   ____   ");
    println!("  / ____// __ \\ / __ ) ");
    println!(" / / __ / / / // __  | ");
    println!("/ /_/ // /_/ // /_/ /  ");
    println!("\\____//_____//_____/   ");
    println!(" Interactive Graph Database Console (openCypher / GQL)");
    println!("\x1b[0m");

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let active_graph = Arc::new(RwLock::new(args.graph.clone()));

    let mut mode = if args.local {
        println!("\x1b[1;33m[*] Starting in local standalone in-memory mode (--local)\x1b[0m\n");
        let (m, _) = new_local_mode(&args.graph);
        m
    } else {
        let health_url = format!("{}/health", args.endpoint.trim_end_matches('/'));
        match client.get(&health_url).send() {
            Ok(resp) if resp.status().is_success() => {
                let info: Value = resp.json().unwrap_or(Value::Null);
                let node_id = info["node_id"].as_u64().unwrap_or(1);
                let role = info["role"].as_str().unwrap_or("Peer");

                println!(
                    "\x1b[1;32m[✓] Connected to GDB Cluster:\x1b[0m \x1b[1m{}\x1b[0m",
                    args.endpoint
                );
                println!(
                    "    \x1b[1;32m[+]\x1b[0m Active Node: \x1b[1;33mNode #{} ({})\x1b[0m | Graph Space: \x1b[1;36m{}\x1b[0m | Status: \x1b[1;32mUP\x1b[0m\n",
                    node_id, role, args.graph
                );

                ClientMode::Cluster {
                    endpoint: args.endpoint.clone(),
                    client,
                    active_graph: active_graph.clone(),
                }
            }
            _ => {
                println!(
                    "\x1b[1;33m[!] Warning: Could not connect to GDB Cluster at {}\x1b[0m",
                    args.endpoint
                );
                println!("    Falling back to local in-memory standalone mode.");
                println!("    Use \x1b[1;36m:connect <url>\x1b[0m or \x1b[1;36mSHOW CLUSTER\x1b[0m to connect when cluster is online.\n");

                let (m, _) = new_local_mode(&args.graph);
                m
            }
        }
    };

    if let Some(query) = args.execute {
        execute_query(&mut mode, &query)?;
        return Ok(());
    }

    if let Some(file_path) = args.file {
        let f = File::open(&file_path)?;
        let reader = BufReader::new(f);
        let start_total = std::time::Instant::now();
        let mut count = 0;

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with("//") && !trimmed.starts_with("#") {
                execute_query(&mut mode, trimmed)?;
                count += 1;
            }
        }
        println!(
            "\n\x1b[1;36mExecuted {} statements from {:?} in {:?}\x1b[0m",
            count,
            file_path,
            start_total.elapsed()
        );
        return Ok(());
    }

    // Interactive REPL Mode with Tab-Completion & Active Graph Prompt
    println!("Type \x1b[1;33m'help'\x1b[0m for instructions, \x1b[1;33m'Tab'\x1b[0m for completion, \x1b[1;33m'USE GRAPH <name>'\x1b[0m to switch spaces, \x1b[1;33m'exit'\x1b[0m to quit.\n");

    let dynamic_terms = Arc::new(RwLock::new(vec![
        "default".to_string(),
        "User".to_string(),
        "Person".to_string(),
        "Document".to_string(),
        "FOLLOWS".to_string(),
        "KNOWS".to_string(),
    ]));
    let completer = Box::new(GdbCompleter::new(dynamic_terms));
    let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));
    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );
    let edit_mode = Box::new(Emacs::new(keybindings));

    let mut line_editor = Reedline::create()
        .with_completer(completer)
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_edit_mode(edit_mode);

    let prompt_graph_ref = match &mode {
        ClientMode::Cluster { active_graph, .. } => active_graph.clone(),
        ClientMode::Local { active_graph, .. } => active_graph.clone(),
    };
    let prompt = GdbPrompt {
        active_graph: prompt_graph_ref,
    };

    loop {
        let sig = line_editor.read_line(&prompt);
        match sig {
            Ok(Signal::Success(buffer)) => {
                if !handle_repl_input(&mut mode, &buffer)? {
                    break;
                }
            }
            Ok(Signal::CtrlC) => {
                println!("^C");
            }
            Ok(Signal::CtrlD) => {
                println!("Goodbye!");
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = CliArgs::parse();
    run_cli(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_format_cell_and_ascii_table() {
        assert_eq!(format_cell(&Value::Null), "NULL");
        assert_eq!(format_cell(&Value::Bool(true)), "true");
        assert_eq!(format_cell(&serde_json::json!(42)), "42");
        assert_eq!(format_cell(&Value::String("test".into())), "test");
        assert_eq!(format_cell(&serde_json::json!([1, 2, 3])), "[1, 2, 3]");
        assert_eq!(format_cell(&serde_json::json!([1, 2, 3, 4, 5, 6, 7])), "[7 items: 1, 2, ...]");
        assert!(format_cell(&serde_json::json!({"a": 1})).contains("1"));

        let cols = vec!["Col1".to_string(), "Col2".to_string()];
        let rows = vec![
            vec![Value::String("v1".into()), serde_json::json!(10)],
            vec![Value::String("v2".into()), serde_json::json!(20)],
        ];
        print_ascii_table(&cols, &rows);
        print_ascii_table(&[], &[]);
    }

    #[test]
    fn test_local_query_execution() {
        let (mut mode, active_graph) = new_local_mode("default");

        // Empty and comments
        assert!(execute_query(&mut mode, "").is_ok());
        assert!(execute_query(&mut mode, "// comment").is_ok());
        assert!(execute_query(&mut mode, "# comment").is_ok());

        // SHOW commands
        assert!(execute_query(&mut mode, "SHOW RESOURCES").is_ok());
        assert!(execute_query(&mut mode, "SHOW GPU").is_ok());
        assert!(execute_query(&mut mode, "SHOW CLUSTER").is_ok());

        // Graph Spaces in Local Mode
        assert!(execute_query(&mut mode, "CREATE GRAPH epinions;").is_ok());
        assert!(execute_query(&mut mode, "USE GRAPH epinions;").is_ok());
        assert_eq!(*active_graph.read(), "epinions");
        assert!(execute_query(&mut mode, "SHOW GRAPHS;").is_ok());

        // DDL
        assert!(execute_query(&mut mode, "CREATE VERTEX Node (val INT64);").is_ok());

        // DML
        assert!(execute_query(&mut mode, "INSERT VERTEX Node (id, val) VALUES (1, 100);").is_ok());

        // Query
        assert!(execute_query(&mut mode, "MATCH (n:Node) RETURN n.val;").is_ok());

        // Compact
        assert!(execute_query(&mut mode, "compact").is_ok());

        // Error handling
        assert!(execute_query(&mut mode, "INVALID SQL").is_err());

        // Completer test
        let mut comp = GdbCompleter::new(Arc::new(RwLock::new(vec!["epinions".to_string()])));
        let suggestions = comp.complete("CALL algo.pa", 12);
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].value, "CALL algo.pageRank");
    }

    #[test]
    fn test_file_batch_execution() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "// GDB Batch Script").unwrap();
        writeln!(file, "CREATE VERTEX Item (cost FLOAT64);").unwrap();
        writeln!(file, "INSERT VERTEX Item (id, cost) VALUES (1, 9.99);").unwrap();
        writeln!(file, "MATCH (i:Item) RETURN i.cost;").unwrap();
        file.flush().unwrap();

        let (mut mode, _) = new_local_mode("test");

        let f = File::open(file.path()).unwrap();
        let reader = BufReader::new(f);
        for line in reader.lines() {
            let line = line.unwrap();
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with("//") {
                execute_query(&mut mode, trimmed).unwrap();
            }
        }
    }

    #[test]
    fn test_cluster_mode_and_show_commands() {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();

        show_cluster(&client, "http://127.0.0.1:19999");
        show_resources(&client, "http://127.0.0.1:19999");
        show_gpu(&client, "http://127.0.0.1:19999");

        let mut mode = ClientMode::Cluster {
            endpoint: "http://127.0.0.1:19999".to_string(),
            client,
            active_graph: Arc::new(RwLock::new("default".to_string())),
        };

        assert!(execute_query(&mut mode, ":CLUSTER").is_ok());
        assert!(execute_query(&mut mode, ":RESOURCES").is_ok());
        assert!(execute_query(&mut mode, ":GPU").is_ok());

        assert!(execute_query(&mut mode, ":connect http://127.0.0.1::8847").is_ok());
        assert!(execute_query(&mut mode, ":connect 127.0.0.1:8847").is_ok());

        let _ = execute_query(&mut mode, "compact");
        let _ = execute_query(&mut mode, "MATCH (n) RETURN n;");
    }

    #[test]
    fn test_online_cluster_mode() {
        let (tx, rx) = std::sync::mpsc::channel();
        let _server_thread = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let wal_dir = tempfile::tempdir().unwrap();
                let (addr, _handle, _state) = gdb_server::create_test_server(wal_dir.path().to_path_buf()).await.unwrap();
                tx.send(addr).unwrap();
                tokio::time::sleep(Duration::from_secs(10)).await;
            });
        });

        let addr = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let endpoint = format!("http://{}", addr);

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();

        show_cluster(&client, &endpoint);
        show_resources(&client, &endpoint);
        show_gpu(&client, &endpoint);

        let active_graph = Arc::new(RwLock::new("default".to_string()));
        let mut mode = ClientMode::Cluster {
            endpoint: endpoint.clone(),
            client,
            active_graph: active_graph.clone(),
        };

        assert!(execute_query(&mut mode, "SHOW SCHEMA;").is_ok());
        assert!(execute_query(&mut mode, "CREATE GRAPH analytics_space;").is_ok());
        assert!(execute_query(&mut mode, "USE GRAPH analytics_space;").is_ok());
        assert_eq!(*active_graph.read(), "analytics_space");
        assert!(execute_query(&mut mode, "SHOW GRAPHS;").is_ok());
        assert!(execute_query(&mut mode, "compact").is_ok());
        assert!(execute_query(&mut mode, ":CLUSTER").is_ok());
        assert!(execute_query(&mut mode, ":RESOURCES").is_ok());
        assert!(execute_query(&mut mode, ":GPU").is_ok());
    }

    #[test]
    fn test_cli_args_parsing() {
        let args = CliArgs::parse_from(&["gdb-cli", "--local", "-e", "SHOW RESOURCES"]);
        assert!(args.local);
        assert_eq!(args.execute.unwrap(), "SHOW RESOURCES");

        let args2 = CliArgs::parse_from(&["gdb-cli", "--endpoint", "http://127.0.0.1:9000", "--graph", "epinions"]);
        assert_eq!(args2.endpoint, "http://127.0.0.1:9000");
        assert_eq!(args2.graph, "epinions");
    }

    #[test]
    fn test_handle_repl_input() {
        let (mut mode, _) = new_local_mode("test");

        assert_eq!(handle_repl_input(&mut mode, "").unwrap(), true);
        assert_eq!(handle_repl_input(&mut mode, "   ").unwrap(), true);
        assert_eq!(handle_repl_input(&mut mode, "help").unwrap(), true);
        assert_eq!(handle_repl_input(&mut mode, "exit").unwrap(), false);
        assert_eq!(handle_repl_input(&mut mode, "quit").unwrap(), false);
        assert_eq!(handle_repl_input(&mut mode, "SHOW RESOURCES").unwrap(), true);
        assert_eq!(handle_repl_input(&mut mode, "INVALID SQL").unwrap(), true);
    }

    #[test]
    fn test_run_cli_executions() {
        let args_exec = CliArgs {
            endpoint: "http://127.0.0.1:8847".into(),
            graph: "default".into(),
            local: true,
            execute: Some("SHOW RESOURCES".into()),
            file: None,
        };
        assert!(run_cli(args_exec).is_ok());

        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "CREATE VERTEX Test (v INT64);").unwrap();
        writeln!(file, "INSERT VERTEX Test (id, v) VALUES (1, 1);").unwrap();
        file.flush().unwrap();

        let args_file = CliArgs {
            endpoint: "http://127.0.0.1:8847".into(),
            graph: "default".into(),
            local: true,
            execute: None,
            file: Some(file.path().to_path_buf()),
        };
        assert!(run_cli(args_file).is_ok());

        let args_fallback = CliArgs {
            endpoint: "http://127.0.0.1:19999".into(),
            graph: "default".into(),
            local: false,
            execute: Some("SHOW GPU".into()),
            file: None,
        };
        assert!(run_cli(args_fallback).is_ok());
    }
}

