use gdb_server::{create_test_server, ReplicationMode, RingNode};
use reqwest::Client;
use std::str::FromStr;
use tempfile::tempdir;

#[test]
fn test_replication_mode_and_ring_helpers() {
    assert_eq!(ReplicationMode::from_str("sync").unwrap(), ReplicationMode::Sync);
    assert_eq!(ReplicationMode::from_str("SYNC").unwrap(), ReplicationMode::Sync);
    assert_eq!(ReplicationMode::from_str("synchronous").unwrap(), ReplicationMode::Sync);
    assert_eq!(ReplicationMode::from_str("async").unwrap(), ReplicationMode::Async);
    assert_eq!(ReplicationMode::from_str("asynchronous").unwrap(), ReplicationMode::Async);
    assert!(ReplicationMode::from_str("invalid_mode").is_err());

    assert_eq!(ReplicationMode::Sync.to_string(), "SYNC");
    assert_eq!(ReplicationMode::Async.to_string(), "ASYNC");

    let ring = vec![
        RingNode {
            node_id: 1,
            http_url: "http://127.0.0.1:8847".into(),
            flight_port: 8848,
            client_flight_port: 8860,
        },
        RingNode {
            node_id: 2,
            http_url: "http://127.0.0.1:8846".into(),
            flight_port: 8849,
            client_flight_port: 8861,
        },
    ];

    let json_str = serde_json::to_string(&ring).unwrap();
    let deserialized: Vec<RingNode> = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.len(), 2);
    assert_eq!(deserialized[0].node_id, 1);
}

#[tokio::test]
async fn test_server_rest_endpoints_comprehensive() {
    let tdir = tempdir().unwrap();
    let wal_dir = tdir.path().to_path_buf();
    let (addr, _handle, _app_state) = create_test_server(wal_dir).await.unwrap();
    let base_url = format!("http://{}", addr);
    let client = Client::new();

    // 1. GET /health
    let res = client.get(format!("{}/health", base_url)).send().await.unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "UP");

    // 2. GET /schema (empty at boot)
    let res = client.get(format!("{}/schema", base_url)).send().await.unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["vertices"].as_array().unwrap().len(), 0);

    // 3. GET /cluster
    let res = client.get(format!("{}/cluster", base_url)).send().await.unwrap();
    assert!(res.status().is_success());
    let cluster_body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(cluster_body["gpu_max_vram_mb"], 2048);

    // 4. GET /resources
    let res = client.get(format!("{}/resources", base_url)).send().await.unwrap();
    assert!(res.status().is_success());
    let res_body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(res_body["gpu_max_vram_mb"], 2048);

    // 5. GET /gpu
    let res = client.get(format!("{}/gpu", base_url)).send().await.unwrap();
    assert!(res.status().is_success());
    let gpu_body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(gpu_body["max_vram_mb"], 2048);
    assert!(gpu_body["paging_strategy"].is_string());

    // 6. GET /metrics
    let res = client.get(format!("{}/metrics", base_url)).send().await.unwrap();
    assert!(res.status().is_success());

    // 7. POST /query (error cases)
    // 7a. Invalid Cypher syntax
    let res = client
        .post(format!("{}/query", base_url))
        .body("INVALID QUERY SYNTAX;")
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "error");

    // 7b. Empty query
    let res = client
        .post(format!("{}/query", base_url))
        .body("")
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "error");

    // 7c. JSON query payload with valid DDL
    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": "CREATE VERTEX Machine (ip STRING, ram INT64);" }))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "ok");

    // 7d. DML Insert
    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": "INSERT VERTEX Machine (id, ip, ram) VALUES (1, '192.168.1.1', 64);" }))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());

    // 8. POST /compact
    let res = client.post(format!("{}/compact", base_url)).send().await.unwrap();
    assert!(res.status().is_success());

    // 9. POST /replicate (compact & query)
    let res = client
        .post(format!("{}/replicate", base_url))
        .json(&serde_json::json!({ "query": "compact" }))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());

    let res = client
        .post(format!("{}/replicate", base_url))
        .json(&serde_json::json!({ "query": "INSERT VERTEX Machine (id, ip, ram) VALUES (2, '192.168.1.2', 32);" }))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());

    // 10. POST /snapshot (not configured)
    let res = client.post(format!("{}/snapshot", base_url)).send().await.unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "not_configured");

    // 11. POST /query (compact; & snapshot;)
    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": "compact;" }))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());

    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": "snapshot;" }))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());

    // 12. Cypher Read & Schema queries
    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": "SHOW SCHEMA;" }))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());

    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": "MATCH (m:Machine) RETURN m.ip, m.ram;" }))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "ok");

    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": "EXPLAIN MATCH (m:Machine) RETURN m.ip;" }))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());

    // 13. POST /raft/replicate
    let res = client
        .post(format!("{}/raft/replicate", base_url))
        .json(&serde_json::json!({ "query": "compact" }))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());

    // 14. Error on replicate
    let res_err = client
        .post(format!("{}/replicate", base_url))
        .json(&serde_json::json!({ "query": "INVALID SYNTAX;" }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = res_err.json().await.unwrap();
    assert_eq!(body["status"], "error");

    let res_exec_err = client
        .post(format!("{}/replicate", base_url))
        .json(&serde_json::json!({ "query": "DROP INDEX non_existent_idx;" }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = res_exec_err.json().await.unwrap();
    assert_eq!(body["status"], "error");

    // 15. POST /query backup alias
    let res_backup = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": "backup;" }))
        .send()
        .await
        .unwrap();
    assert!(res_backup.status().is_success());
}

#[tokio::test]
async fn test_server_replication_multinode_and_async() {
    let tdir1 = tempdir().unwrap();
    let tdir2 = tempdir().unwrap();

    let (addr1, _h1, mut state1) = create_test_server(tdir1.path().to_path_buf()).await.unwrap();
    let (addr2, _h2, mut state2) = create_test_server(tdir2.path().to_path_buf()).await.unwrap();

    let node1 = RingNode {
        node_id: 1,
        http_url: format!("http://{}", addr1),
        flight_port: 0,
        client_flight_port: 0,
    };
    let node2 = RingNode {
        node_id: 2,
        http_url: format!("http://{}", addr2),
        flight_port: 0,
        client_flight_port: 0,
    };

    let ring = vec![node1.clone(), node2.clone()];
    state1.ring_nodes = ring.clone();
    state1.replication_factor = 2;
    state1.replication_mode = ReplicationMode::Sync;

    state2.ring_nodes = ring.clone();
    state2.replication_factor = 2;
    state2.replication_mode = ReplicationMode::Sync;

    // Create a 2-node router app with updated state
    let app1 = gdb_server::create_router(state1.clone());
    let listener1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let multi_addr1 = listener1.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener1, app1).await.unwrap();
    });

    let client = Client::new();
    let base_url1 = format!("http://{}", multi_addr1);

    // 1. DDL broadcast across multi-node ring
    let ddl_res = client
        .post(format!("{}/query", base_url1))
        .json(&serde_json::json!({ "query": "CREATE VERTEX Node (ip STRING);" }))
        .send()
        .await
        .unwrap();
    assert!(ddl_res.status().is_success());

    let ddl_edge = client
        .post(format!("{}/query", base_url1))
        .json(&serde_json::json!({ "query": "CREATE EDGE LINK ();" }))
        .send()
        .await
        .unwrap();
    assert!(ddl_edge.status().is_success());

    // 2. Batch insert vertices with multi-node replication
    let ins_res = client
        .post(format!("{}/query", base_url1))
        .json(&serde_json::json!({
            "query": "INSERT VERTEX Node (id, ip) VALUES (101, '10.0.0.1'), (102, '10.0.0.2');"
        }))
        .send()
        .await
        .unwrap();
    assert!(ins_res.status().is_success());

    // 3. Batch insert edges with multi-node replication
    let ins_edges = client
        .post(format!("{}/query", base_url1))
        .json(&serde_json::json!({
            "query": "INSERT EDGE LINK FROM 101 TO 102;"
        }))
        .send()
        .await
        .unwrap();
    assert!(ins_edges.status().is_success());

    // 4. Merge vertex with multi-node replication
    let merge_res = client
        .post(format!("{}/query", base_url1))
        .json(&serde_json::json!({
            "query": "MERGE VERTEX Node (id, ip) VALUES (101, '10.0.0.100');"
        }))
        .send()
        .await
        .unwrap();
    assert!(merge_res.status().is_success());

    // 5. Delete edge with multi-node replication
    let del_edge = client
        .post(format!("{}/query", base_url1))
        .json(&serde_json::json!({
            "query": "DELETE EDGE LINK FROM 101 TO 102;"
        }))
        .send()
        .await
        .unwrap();
    assert!(del_edge.status().is_success());

    // 6. Test async replication mode
    let mut async_state = state1.clone();
    async_state.replication_mode = ReplicationMode::Async;
    let async_app = gdb_server::create_router(async_state);
    let async_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let async_addr = async_listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(async_listener, async_app).await.unwrap();
    });

    let async_url = format!("http://{}", async_addr);
    let res_async = client
        .post(format!("{}/query", async_url))
        .json(&serde_json::json!({
            "query": "INSERT VERTEX Node (id, ip) VALUES (200, '10.0.0.200');"
        }))
        .send()
        .await
        .unwrap();
    assert!(res_async.status().is_success());
}

#[tokio::test]
async fn test_server_multi_statement_and_batch_endpoints() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, _handle, _state) = create_test_server(tmp.path().to_path_buf()).await.unwrap();
    let client = reqwest::Client::new();
    let base_url = format!("http://{}", addr);

    // 1. Multi-statement script via /query
    let script = r#"
        CREATE VERTEX Item (sku STRING, emb VECTOR(3));
        INSERT VERTEX Item (id, sku, emb) VALUES (1, 'item-A', [1.0, 0.0, 0.0]);
        INSERT VERTEX Item (id, sku, emb) VALUES (2, 'item-B', [0.0, 1.0, 0.0]);
        MATCH (i:Item) RETURN i.sku;
    "#;
    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": script }))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["statements_executed"], 4);
    assert_eq!(body["rows_affected"], 4);
    assert_eq!(body["num_rows"], 2);

    // 2. Multi-statement error handling
    let bad_script = r#"
        INSERT VERTEX Item (id, sku) VALUES (3, 'item-C');
        THIS IS INVALID SYNTAX;
        INSERT VERTEX Item (id, sku) VALUES (4, 'item-D');
    "#;
    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": bad_script }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "error");
    assert_eq!(body["statement_index"], 1);

    // 3. Batch API with queries array
    let batch_req = serde_json::json!({
        "queries": [
            "CREATE VERTEX User (name STRING);",
            "INSERT VERTEX User (id, name) VALUES (10, 'Alice');",
            "INSERT VERTEX User (id, name) VALUES (20, 'Bob');",
            "MATCH (u:User) RETURN u.name;"
        ]
    });
    let res = client
        .post(format!("{}/batch", base_url))
        .json(&batch_req)
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["total"], 4);
    assert_eq!(body["succeeded"], 4);
    assert_eq!(body["failed"], 0);

    // 4. Batch API with script field
    let batch_script = serde_json::json!({
        "script": "INSERT VERTEX User (id, name) VALUES (30, 'Charlie'); INSERT VERTEX User (id, name) VALUES (40, 'Dave');"
    });
    let res = client
        .post(format!("{}/batch", base_url))
        .json(&batch_script)
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["total"], 2);
    assert_eq!(body["succeeded"], 2);

    // 5. Batch API with raw text
    let res = client
        .post(format!("{}/batch", base_url))
        .body("INSERT VERTEX User (id, name) VALUES (50, 'Eve'); MATCH (u:User) RETURN count(u.name);")
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["total"], 2);

    // 6. Vector DDL, Insert, and Similarity Search via /query JSON serialization
    let vec_script = r#"
        CREATE VERTEX Doc (title STRING, emb VECTOR(2));
        INSERT VERTEX Doc (id, title, emb) VALUES (1, 'Doc1', [1.0, 0.0]), (2, 'Doc2', [0.0, 1.0]);
        ANALYZE GRAPH;
        CALL vector.similaritySearch('Doc', 'emb', [1.0, 0.0], 1, 'cosine') YIELD vertex_id, score;
    "#;
    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": vec_script }))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["statements_executed"], 4);

    // Verify /resources endpoint returns valid JSON with metrics
    let res = client.get(format!("{}/resources", base_url)).send().await.unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["total_vertices"].as_u64().unwrap() > 0);
}


