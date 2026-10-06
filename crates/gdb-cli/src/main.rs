use clap::Parser;
use gdb_core::schema::GraphSchema;
use gdb_planner::QueryExecutor;
use gdb_storage::PartitionStorageEngine;
use parking_lot::RwLock;
use reedline::{DefaultPrompt, Reedline, Signal};
use serde_json::Value;
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

enum ClientMode {
    Cluster {
        endpoint: String,
        client: reqwest::blocking::Client,
    },
    Local {
        executor: QueryExecutor,
        storage: Arc<PartitionStorageEngine>,
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
        Value::Array(arr) => format!("[{} items]", arr.len()),
        Value::Object(_) => format!("{}", val),
    }
}

fn show_cluster(client: &reqwest::blocking::Client, current_endpoint: &str) {
    println!("\n\x1b[1;36m=== GDB Cluster Topology & Node Status ===\x1b[0m");

    // Standard ports for local 3-node cluster or custom endpoint
    let candidate_endpoints = vec![
        ("Node 1 (Leader)", "http://127.0.0.1:8847", 8848),
        ("Node 2 (Follower)", "http://127.0.0.1:8846", 8849),
        ("Node 3 (Follower)", "http://127.0.0.1:8845", 8850),
    ];

    let cols = vec![
        "Node".to_string(),
        "Role".to_string(),
        "HTTP Endpoint".to_string(),
        "Flight Port".to_string(),
        "Status".to_string(),
        "Latency".to_string(),
    ];
    let mut rows: Vec<Vec<Value>> = Vec::new();

    for (label, url, flight_port) in candidate_endpoints {
        let target_url = if url == "http://127.0.0.1:8847" && current_endpoint != url {
            current_endpoint
        } else {
            url
        };

        let start = std::time::Instant::now();
        let health_url = format!("{}/health", target_url.trim_end_matches('/'));

        match client.get(&health_url).timeout(Duration::from_millis(600)).send() {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed();
                let is_current = if target_url == current_endpoint { " (active)" } else { "" };
                let role = if label.contains("Leader") { "Leader" } else { "Follower" };

                rows.push(vec![
                    Value::String(format!("{}{}", label, is_current)),
                    Value::String(role.to_string()),
                    Value::String(target_url.to_string()),
                    Value::Number(flight_port.into()),
                    Value::String("\x1b[1;32mUP\x1b[0m".to_string()),
                    Value::String(format!("{:.2} ms", elapsed.as_secs_f64() * 1000.0)),
                ]);
            }
            _ => {
                rows.push(vec![
                    Value::String(label.to_string()),
                    Value::String("-".to_string()),
                    Value::String(target_url.to_string()),
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

                let rows = vec![
                    vec![Value::String("Node Role".into()), Value::String(format!("Node #{} ({})", data["node_id"], data["role"].as_str().unwrap_or("Leader")))],
                    vec![Value::String("Total Vertices (CSR)".into()), Value::String(data["total_vertices"].to_string())],
                    vec![Value::String("Total Active Edges".into()), Value::String(data["total_edges"].to_string())],
                    vec![Value::String("Chunked-CSR Edges".into()), Value::String(data["csr_edges"].to_string())],
                    vec![Value::String("Delta MemTable Edges".into()), Value::String(data["memtable_edges"].to_string())],
                    vec![Value::String("Background Compactions".into()), Value::String(data["compactions_total"].to_string())],
                    vec![Value::String("Total Queries Executed".into()), Value::String(format!("{} (ok: {}, err: {})", data["queries_total"], data["queries_ok"], data["queries_error"]))],
                    vec![Value::String("Process Uptime".into()), Value::String(uptime_fmt)],
                    vec![Value::String("Estimated In-Memory RAM".into()), Value::String(format!("{:.2} MB", mem_mb))],
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
                let cols = vec!["GPU Attribute".to_string(), "Status / Value".to_string()];
                let rows = vec![
                    vec![Value::String("Compute Accelerator".into()), Value::String(data["backend"].as_str().unwrap_or("Apple Metal").into())],
                    vec![Value::String("Memory Architecture".into()), Value::String(data["memory_model"].as_str().unwrap_or("UMA Zero-Copy").into())],
                    vec![Value::String("Offload Threshold".into()), Value::String(format!("{} edges", data["threshold_edges"]))],
                    vec![Value::String("Kernel Status".into()), Value::String("\x1b[1;32mActive & Ready\x1b[0m".into())],
                    vec![Value::String("Supported Kernels".into()), Value::String("Parallel BFS Frontier, PageRank MSL, Similarity".into())],
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
            ClientMode::Cluster { client, endpoint } => show_cluster(client, endpoint),
            ClientMode::Local { .. } => {
                println!("\x1b[1;33mCurrently in local standalone mode. Use ':connect <url>' to attach to a cluster.\x1b[0m");
            }
        }
        return Ok(());
    }

    if upper == "SHOW RESOURCES" || upper == ":RESOURCES" || upper == "RESOURCES" {
        match mode {
            ClientMode::Cluster { client, endpoint } => show_resources(client, endpoint),
            ClientMode::Local { storage, .. } => {
                println!("\n\x1b[1;36m=== Local Storage Resources ===\x1b[0m");
                println!("  CSR Edges:       {}", storage.csr_edges());
                println!("  MemTable Edges:  {}", storage.delta_edges());
                println!("  Total Edges:     {}\n", storage.total_edges());
            }
        }
        return Ok(());
    }

    if upper == "SHOW GPU" || upper == ":GPU" || upper == "GPU" {
        match mode {
            ClientMode::Cluster { client, endpoint } => show_gpu(client, endpoint),
            ClientMode::Local { .. } => {
                let disp = gdb_gpu::GpuDispatcher::new();
                println!("\n\x1b[1;36m=== Local Hardware Acceleration ===\x1b[0m");
                println!("  Backend:    {}", disp.backend_name());
                println!("  Threshold:  {} edges\n", disp.threshold_edges);
            }
        }
        return Ok(());
    }

    if input.starts_with(":connect ") {
        let mut raw_url = input.trim_start_matches(":connect ").trim().to_string();
        // Auto-fix double colon typo (e.g. "localhost::8847" -> "localhost:8847")
        if raw_url.contains("::") && !raw_url.contains('[') {
            println!("\x1b[1;33m[*] Notice: Detected '::' typo in URL, normalizing to single ':'\x1b[0m");
            raw_url = raw_url.replace("::", ":");
        }
        // Auto-prepend http:// if protocol scheme is omitted
        if !raw_url.starts_with("http://") && !raw_url.starts_with("https://") {
            raw_url = format!("http://{}", raw_url);
        }
        let clean_url = raw_url.trim_end_matches('/').to_string();
        let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(3)).build()?;
        let health_url = format!("{}/health", clean_url);

        match client.get(&health_url).send() {
            Ok(resp) if resp.status().is_success() => {
                println!("\x1b[1;32m[✓] Successfully connected to GDB Cluster at {}\x1b[0m", clean_url);
                *mode = ClientMode::Cluster {
                    endpoint: clean_url,
                    client,
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
        ClientMode::Cluster { client, endpoint } => {
            let start = std::time::Instant::now();
            let query_url = format!("{}/query", endpoint.trim_end_matches('/'));
            let payload = serde_json::json!({ "query": input });

            let resp = client
                .post(&query_url)
                .json(&payload)
                .send()?;

            let data: Value = resp.json()?;
            let client_elapsed = start.elapsed();

            if data["status"] == "ok" {
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
        ClientMode::Local { executor, storage } => {
            if input.eq_ignore_ascii_case("compact") || input.eq_ignore_ascii_case("compact;") {
                let start = std::time::Instant::now();
                storage.compact();
                println!("\x1b[1;32m[✓] Local compaction completed in {:?}\x1b[0m", start.elapsed());
                return Ok(());
            }

            let start = std::time::Instant::now();
            let stmt = gdb_parser::parse(input)?;
            let res = executor.execute(stmt)?;
            let elapsed = start.elapsed();

            if let Some(batch) = res.batch {
                arrow::util::pretty::print_batches(&[batch])?;
            }
            println!("\x1b[1;32m{} (took {:?})\x1b[0m", res.message, elapsed);
        }
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = CliArgs::parse();

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

    let mut mode = if args.local {
        println!("\x1b[1;33m[*] Starting in local standalone in-memory mode (--local)\x1b[0m\n");
        let schema = Arc::new(RwLock::new(GraphSchema::new("social")));
        let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
        let executor = QueryExecutor::new(schema, storage.clone());
        ClientMode::Local { executor, storage }
    } else {
        // Probe cluster health
        let health_url = format!("{}/health", args.endpoint.trim_end_matches('/'));
        match client.get(&health_url).send() {
            Ok(resp) if resp.status().is_success() => {
                let info: Value = resp.json().unwrap_or(Value::Null);
                let node_id = info["node_id"].as_u64().unwrap_or(1);
                let role = info["role"].as_str().unwrap_or("Leader");

                println!(
                    "\x1b[1;32m[✓] Connected to GDB Cluster:\x1b[0m \x1b[1m{}\x1b[0m",
                    args.endpoint
                );
                println!(
                    "    \x1b[1;32m[+]\x1b[0m Active Node: \x1b[1;33mNode #{} ({})\x1b[0m | Status: \x1b[1;32mUP\x1b[0m\n",
                    node_id, role
                );

                ClientMode::Cluster {
                    endpoint: args.endpoint.clone(),
                    client,
                }
            }
            _ => {
                println!(
                    "\x1b[1;33m[!] Warning: Could not connect to GDB Cluster at {}\x1b[0m",
                    args.endpoint
                );
                println!("    Falling back to local in-memory standalone mode.");
                println!("    Use \x1b[1;36m:connect <url>\x1b[0m or \x1b[1;36mSHOW CLUSTER\x1b[0m to connect when cluster is online.\n");

                let schema = Arc::new(RwLock::new(GraphSchema::new("social")));
                let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
                let executor = QueryExecutor::new(schema, storage.clone());
                ClientMode::Local { executor, storage }
            }
        }
    };

    // Non-interactive: Execute from string
    if let Some(query) = args.execute {
        execute_query(&mut mode, &query)?;
        return Ok(());
    }

    // Non-interactive: Execute from file
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

    // Interactive REPL Mode
    println!("Type \x1b[1;33m'help'\x1b[0m for instructions, \x1b[1;33m'SHOW CLUSTER'\x1b[0m for node status, \x1b[1;33m'exit'\x1b[0m to quit.\n");

    let mut line_editor = Reedline::create();
    let prompt = DefaultPrompt::default();

    loop {
        let sig = line_editor.read_line(&prompt);
        match sig {
            Ok(Signal::Success(buffer)) => {
                let input = buffer.trim();
                if input.is_empty() {
                    continue;
                }
                if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
                    println!("Goodbye!");
                    break;
                }
                if input.eq_ignore_ascii_case("help") {
                    println!("\x1b[1;33mSystem & Cluster Commands:\x1b[0m");
                    println!("  SHOW CLUSTER              - Display cluster topology, roles & node latencies");
                    println!("  SHOW RESOURCES            - Display in-memory usage (CSR, MemTable, Vertices, Edges)");
                    println!("  SHOW GPU                  - Display hardware GPU acceleration status & kernels");
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
                    continue;
                }

                if let Err(e) = execute_query(&mut mode, input) {
                    eprintln!("\x1b[1;31mError: {}\x1b[0m\n", e);
                } else {
                    println!();
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
