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
            flight_address: "127.0.0.1".into(),
            flight_port: 8848,
            client_flight_address: "127.0.0.1".into(),
            client_flight_port: 8860,
        },
        RingNode {
            node_id: 2,
            http_url: "http://127.0.0.1:8846".into(),
            flight_address: "127.0.0.1".into(),
            flight_port: 8849,
            client_flight_address: "127.0.0.1".into(),
            client_flight_port: 8861,
        },
    ];

    let json_str = serde_json::to_string(&ring).unwrap();
    let deserialized: Vec<RingNode> = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.len(), 2);
    assert_eq!(deserialized[0].node_id, 1);
    assert_eq!(deserialized[0].flight_address, "127.0.0.1");
    assert_eq!(deserialized[0].client_flight_address, "127.0.0.1");
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
    assert_eq!(body["flight_address"], "127.0.0.1");
    assert_eq!(body["client_flight_address"], "127.0.0.1");

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
    assert_eq!(cluster_body["flight_address"], "127.0.0.1");
    assert_eq!(cluster_body["client_flight_address"], "127.0.0.1");
    assert_eq!(cluster_body["ring_nodes"][0]["flight_address"], "127.0.0.1");

    // 3b. POST /cluster/join
    let join_node = RingNode {
        node_id: 2,
        http_url: "http://10.0.0.2:8846".into(),
        flight_address: "10.0.0.2".into(),
        flight_port: 8849,
        client_flight_address: "10.0.0.2".into(),
        client_flight_port: 8861,
    };
    let join_res = client
        .post(format!("{}/cluster/join", base_url))
        .json(&join_node)
        .send()
        .await
        .unwrap();
    assert!(join_res.status().is_success());
    let join_body: serde_json::Value = join_res.json().await.unwrap();
    assert_eq!(join_body["status"], "ok");
    assert_eq!(join_body["ring_nodes"].as_array().unwrap().len(), 2);

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
        flight_address: "127.0.0.1".into(),
        flight_port: 0,
        client_flight_address: "127.0.0.1".into(),
        client_flight_port: 0,
    };
    let node2 = RingNode {
        node_id: 2,
        http_url: format!("http://{}", addr2),
        flight_address: "127.0.0.1".into(),
        flight_port: 0,
        client_flight_address: "127.0.0.1".into(),
        client_flight_port: 0,
    };

    let ring = vec![node1.clone(), node2.clone()];
    *state1.ring_nodes.write() = ring.clone();
    state1.replication_factor = 2;
    state1.replication_mode = ReplicationMode::Sync;

    *state2.ring_nodes.write() = ring.clone();
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

#[tokio::test]
async fn test_server_mpp_scatter_gather_queries() {
    let client = reqwest::Client::new();
    let tdir1 = tempdir().unwrap();
    let tdir2 = tempdir().unwrap();

    let (addr1, _h1, state1) = create_test_server(tdir1.path().to_path_buf()).await.unwrap();
    let (addr2, _h2, state2) = create_test_server(tdir2.path().to_path_buf()).await.unwrap();

    let url1 = format!("http://{}", addr1);
    let url2 = format!("http://{}", addr2);

    let node1 = RingNode {
        node_id: 1,
        http_url: url1.clone(),
        flight_address: "127.0.0.1".into(),
        flight_port: 0,
        client_flight_address: "127.0.0.1".into(),
        client_flight_port: 0,
    };
    let node2 = RingNode {
        node_id: 2,
        http_url: url2.clone(),
        flight_address: "127.0.0.1".into(),
        flight_port: 0,
        client_flight_address: "127.0.0.1".into(),
        client_flight_port: 0,
    };

    let ring = vec![node1.clone(), node2.clone()];
    *state1.ring_nodes.write() = ring.clone();
    *state2.ring_nodes.write() = ring.clone();

    // DDL on both nodes
    client.post(format!("{}/query", url1))
        .json(&serde_json::json!({ "query": "CREATE VERTEX Product (name STRING, emb VECTOR(2));" }))
        .send().await.unwrap();
    client.post(format!("{}/query", url2))
        .json(&serde_json::json!({ "query": "CREATE VERTEX Product (name STRING, emb VECTOR(2));" }))
        .send().await.unwrap();

    // Insert Product 1 on Node 1 (local_only)
    client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "INSERT VERTEX Product (id, name, emb) VALUES (1, 'Prod1', [1.0, 0.0]);",
            "local_only": true
        }))
        .send().await.unwrap();

    // Insert Product 2 on Node 2 (local_only)
    client.post(format!("{}/query", url2))
        .json(&serde_json::json!({
            "query": "INSERT VERTEX Product (id, name, emb) VALUES (2, 'Prod2', [0.0, 1.0]);",
            "local_only": true
        }))
        .send().await.unwrap();

    // Compact on both nodes
    client.post(format!("{}/compact", url1)).send().await.unwrap();
    client.post(format!("{}/compact", url2)).send().await.unwrap();

    // 2. Query MATCH (p:Product) from Node 1 (scatter-gather)
    let res = client.post(format!("{}/query", url1))
        .json(&serde_json::json!({ "query": "MATCH (p:Product) RETURN p.name;" }))
        .send().await.unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["num_rows"], 2);

    let rows = body["rows"].as_array().unwrap();
    let names: Vec<&str> = rows.iter().map(|r| r[0].as_str().unwrap()).collect();
    assert!(names.contains(&"Prod1"));
    assert!(names.contains(&"Prod2"));

    // 3. Query Vector Similarity Search from Node 1 (scatter-gather Top-K across both nodes)
    let res_vec = client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "CALL vector.similaritySearch('Product', 'emb', [1.0, 0.0], 2, 'cosine') YIELD vertex_id, score;"
        }))
        .send().await.unwrap();
    assert!(res_vec.status().is_success());
    let body_vec: serde_json::Value = res_vec.json().await.unwrap();
    assert_eq!(body_vec["status"], "ok");
    assert_eq!(body_vec["num_rows"], 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_sharded_cross_node_multi_hop_traversal_and_resources() {
    let tdir1 = tempdir().unwrap();
    let tdir2 = tempdir().unwrap();
    let (addr1, _h1, state1) = create_test_server(tdir1.path().to_path_buf()).await.unwrap();
    let (addr2, _h2, state2) = create_test_server(tdir2.path().to_path_buf()).await.unwrap();

    let url1 = format!("http://{}", addr1);
    let url2 = format!("http://{}", addr2);

    let node1 = RingNode {
        node_id: 1,
        http_url: url1.clone(),
        flight_address: "127.0.0.1".into(),
        flight_port: 8848,
        client_flight_address: "127.0.0.1".into(),
        client_flight_port: 8860,
    };
    let node2 = RingNode {
        node_id: 2,
        http_url: url2.clone(),
        flight_address: "127.0.0.1".into(),
        flight_port: 8849,
        client_flight_address: "127.0.0.1".into(),
        client_flight_port: 8861,
    };

    *state1.ring_nodes.write() = vec![node1.clone(), node2.clone()];
    *state2.ring_nodes.write() = vec![node1.clone(), node2.clone()];

    // Explicitly set RF=1 (pure distributed sharding)
    // state1.replication_factor is already 1 by default
    assert_eq!(state1.replication_factor, 1);
    assert_eq!(state2.replication_factor, 1);

    let client = Client::new();

    // 1. Setup Schema on both nodes
    client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "CREATE VERTEX Device (id INT64); CREATE EDGE CONNECTED ();",
            "local_only": true
        }))
        .send().await.unwrap();

    client.post(format!("{}/query", url2))
        .json(&serde_json::json!({
            "query": "CREATE VERTEX Device (id INT64); CREATE EDGE CONNECTED ();",
            "local_only": true
        }))
        .send().await.unwrap();

    // 2. Sharded Ingestion:
    // Node 1 receives: 1 -> 2
    client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "INSERT VERTEX Device (id) VALUES (1); INSERT VERTEX Device (id) VALUES (2); INSERT EDGE CONNECTED FROM 1 TO 2;",
            "local_only": true
        }))
        .send().await.unwrap();

    // Node 2 receives: 2 -> 3
    client.post(format!("{}/query", url2))
        .json(&serde_json::json!({
            "query": "INSERT VERTEX Device (id) VALUES (2); INSERT VERTEX Device (id) VALUES (3); INSERT EDGE CONNECTED FROM 2 TO 3;",
            "local_only": true
        }))
        .send().await.unwrap();

    // Compact both
    client.post(format!("{}/compact", url1)).send().await.unwrap();
    client.post(format!("{}/compact", url2)).send().await.unwrap();

    // 3. Verify /storage/out_edges endpoint on Node 2
    let res_edges = client.post(format!("{}/storage/out_edges", url2))
        .json(&serde_json::json!({
            "vids": [2]
        }))
        .send().await.unwrap();
    assert!(res_edges.status().is_success());
    let edge_map: serde_json::Value = res_edges.json().await.unwrap();
    assert!(edge_map.get("2").is_some());
    let edges_for_2 = edge_map["2"].as_array().unwrap();
    assert_eq!(edges_for_2.len(), 1);
    assert_eq!(edges_for_2[0]["src"], 2);
    assert_eq!(edges_for_2[0]["dst"], 3);

    // 4. Verify /resources aggregation on Node 1 (RF=1)
    let res_res = client.get(format!("{}/resources", url1)).send().await.unwrap();
    assert!(res_res.status().is_success());
    let res_body: serde_json::Value = res_res.json().await.unwrap();
    assert_eq!(res_body["total_nodes"], 2);
    assert_eq!(res_body["local_edges"], 1); // Local Node 1 has 1 edge
    assert_eq!(res_body["cluster_total_edges"], 2); // Cluster total is 1 + 1 = 2
    assert_eq!(res_body["total_edges"], 2);

    // 5. Query 2-hop traversal across shards: MATCH (a:Device)-[:CONNECTED]->(b:Device)-[:CONNECTED]->(c:Device)
    // Hop 1 (1->2) is on Node 1, Hop 2 (2->3) is on Node 2!
    let res_query = client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "MATCH (a:Device)-[:CONNECTED]->(b:Device)-[:CONNECTED]->(c:Device) RETURN a.id, b.id, c.id;"
        }))
        .send().await.unwrap();
    assert!(res_query.status().is_success());
    let query_body: serde_json::Value = res_query.json().await.unwrap();
    assert_eq!(query_body["status"], "ok");
    assert_eq!(query_body["num_rows"], 1);
    let rows = query_body["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0], serde_json::json!([1, 2, 3]));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rf_less_than_n_cross_node_property_resolution_and_ddl_drop() {
    let dir1 = tempdir().unwrap();
    let dir2 = tempdir().unwrap();

    let (addr1, _h1, state1) = create_test_server(dir1.path().to_path_buf()).await.unwrap();
    let (addr2, _h2, state2) = create_test_server(dir2.path().to_path_buf()).await.unwrap();

    let url1 = format!("http://{}", addr1);
    let url2 = format!("http://{}", addr2);

    let ring = vec![
        RingNode {
            node_id: 1,
            http_url: url1.clone(),
            flight_address: "127.0.0.1".into(),
            flight_port: 0,
            client_flight_address: "127.0.0.1".into(),
            client_flight_port: 0,
        },
        RingNode {
            node_id: 2,
            http_url: url2.clone(),
            flight_address: "127.0.0.1".into(),
            flight_port: 0,
            client_flight_address: "127.0.0.1".into(),
            client_flight_port: 0,
        },
    ];

    *state1.ring_nodes.write() = ring.clone();
    *state2.ring_nodes.write() = ring;

    let client = Client::new();

    // 1. DDL: CREATE VERTEX Users and EDGES FOLLOWS
    client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "CREATE VERTEX Users (name STRING, age int64); CREATE EDGES FOLLOWS ();"
        }))
        .send().await.unwrap();

    // 2. Insert sharded data: Node 1 has User 1 (Alice) -> 2; Node 2 has User 2 (Bob)
    client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "INSERT VERTEX Users (id, name, age) VALUES (1, 'Alice', 30); INSERT EDGE FOLLOWS FROM 1 TO 2;",
            "local_only": true
        }))
        .send().await.unwrap();

    client.post(format!("{}/query", url2))
        .json(&serde_json::json!({
            "query": "INSERT VERTEX Users (id, name, age) VALUES (2, 'Bob', 25);",
            "local_only": true
        }))
        .send().await.unwrap();

    // 3. Test /storage/vertex_properties on Node 2 directly
    let res_props = client.post(format!("{}/storage/vertex_properties", url2))
        .json(&serde_json::json!({
            "vids": [2],
            "properties": ["name", "age"]
        }))
        .send().await.unwrap();
    assert!(res_props.status().is_success());
    let props_map: serde_json::Value = res_props.json().await.unwrap();
    assert!(props_map.get("2").is_some());
    assert_eq!(props_map["2"]["name"]["String"], "Bob");
    assert_eq!(props_map["2"]["age"]["Int64"], 25);

    // 4. Test remote property resolution via openCypher query on Node 1:
    // Destination vertex 2 properties must NOT be null!
    let res_match = client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "MATCH (a:User)-[:FOLLOWS]->(b:User) RETURN a.name, b.name, b.age;"
        }))
        .send().await.unwrap();
    let body: serde_json::Value = res_match.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    let rows = body["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0], serde_json::json!(["Alice", "Bob", 25]));

    // 5. Test DDL DROP and Re-create lifecycle
    let res_drop = client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "DROP VERTEX User; DROP EDGES FOLLOWS;"
        }))
        .send().await.unwrap();
    assert_eq!(res_drop.json::<serde_json::Value>().await.unwrap()["status"], "ok");

    let res_recreate = client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "CREATE VERTEX Users (name STRING, age int64); CREATE EDGES FOLLOWS ();"
        }))
        .send().await.unwrap();
    assert_eq!(res_recreate.json::<serde_json::Value>().await.unwrap()["status"], "ok");

    // Ingest new data after drop/recreate
    client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "INSERT VERTEX Users (id, name, age) VALUES (10, 'Carol', 28); INSERT EDGE FOLLOWS FROM 10 TO 20;",
            "local_only": true
        }))
        .send().await.unwrap();

    client.post(format!("{}/query", url2))
        .json(&serde_json::json!({
            "query": "INSERT VERTEX Users (id, name, age) VALUES (20, 'Dave', 35);",
            "local_only": true
        }))
        .send().await.unwrap();

    let res_after_recreate = client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "MATCH (a:User)-[:FOLLOWS]->(b:User) RETURN a.name, b.name, b.age;"
        }))
        .send().await.unwrap();
    let body_after: serde_json::Value = res_after_recreate.json().await.unwrap();
    assert_eq!(body_after["status"], "ok");
    let rows_after = body_after["rows"].as_array().unwrap();
    assert_eq!(rows_after.len(), 1);
    assert_eq!(rows_after[0], serde_json::json!(["Carol", "Dave", 35]));
}

#[tokio::test]
async fn test_cluster_analyze_graph_and_sharded_batch_insert() {
    let tdir1 = tempdir().unwrap();
    let (addr1, _h1, state1) = create_test_server(tdir1.path().to_path_buf()).await.unwrap();
    let url1 = format!("http://{}", addr1);

    let tdir2 = tempdir().unwrap();
    let (addr2, _h2, state2) = create_test_server(tdir2.path().to_path_buf()).await.unwrap();
    let url2 = format!("http://{}", addr2);

    let ring = vec![
        RingNode {
            node_id: 1,
            http_url: url1.clone(),
            flight_address: "127.0.0.1".into(),
            flight_port: 0,
            client_flight_address: "127.0.0.1".into(),
            client_flight_port: 0,
        },
        RingNode {
            node_id: 2,
            http_url: url2.clone(),
            flight_address: "127.0.0.1".into(),
            flight_port: 0,
            client_flight_address: "127.0.0.1".into(),
            client_flight_port: 0,
        },
    ];

    *state1.ring_nodes.write() = ring.clone();
    *state2.ring_nodes.write() = ring;

    let client = Client::new();

    // 1. Create Schema on cluster
    let ddl_res = client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "CREATE VERTEX Person (name STRING, age INT64); CREATE EDGE KNOWS ();"
        }))
        .send().await.unwrap();
    assert_eq!(ddl_res.json::<serde_json::Value>().await.unwrap()["status"], "ok");

    // Replicate schema to node 2 so both have schema
    client.post(format!("{}/query", url2))
        .json(&serde_json::json!({
            "query": "CREATE VERTEX Person (name STRING, age INT64); CREATE EDGE KNOWS ();",
            "local_only": true
        }))
        .send().await.unwrap();

    // 2. Batch insert 4 vertices through coordinator node 1 with RF=1
    let insert_v = client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "INSERT VERTEX Person (id, name, age) VALUES (1, 'Alice', 30), (2, 'Bob', 25), (3, 'Charlie', 35), (4, 'Dave', 40);"
        }))
        .send().await.unwrap();
    let insert_v_json: serde_json::Value = insert_v.json().await.unwrap();
    assert_eq!(insert_v_json["status"], "ok");
    assert_eq!(insert_v_json["rows_affected"], 4);

    // 3. Verify /resources reports total_vertices = 4 (no duplicates)
    let res_metrics = client.get(format!("{}/resources", url1)).send().await.unwrap();
    let metrics_json: serde_json::Value = res_metrics.json().await.unwrap();
    assert_eq!(metrics_json["cluster_total_vertices"], 4);
    assert_eq!(metrics_json["total_vertices"], 4);

    // 4. Batch insert 2 edges through coordinator node 1 with RF=1
    let insert_e = client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "INSERT EDGE KNOWS VALUES (1, 2, 0), (2, 3, 0);"
        }))
        .send().await.unwrap();
    let insert_e_json: serde_json::Value = insert_e.json().await.unwrap();
    assert_eq!(insert_e_json["status"], "ok");
    assert_eq!(insert_e_json["rows_affected"], 2);

    let res_metrics2 = client.get(format!("{}/resources", url1)).send().await.unwrap();
    let metrics_json2: serde_json::Value = res_metrics2.json().await.unwrap();
    assert_eq!(metrics_json2["cluster_total_edges"], 2);
    assert_eq!(metrics_json2["total_edges"], 2);

    // 5. Run ANALYZE GRAPH on coordinator node 1
    let analyze_res = client.post(format!("{}/query", url1))
        .json(&serde_json::json!({
            "query": "ANALYZE GRAPH;"
        }))
        .send().await.unwrap();
    let analyze_json: serde_json::Value = analyze_res.json().await.unwrap();
    assert_eq!(analyze_json["status"], "ok");
    assert!(analyze_json["message"].as_str().unwrap().contains("4 vertices across 1 labels"));
    assert!(analyze_json["message"].as_str().unwrap().contains("2 edges across 1 types"));

    let stats = &analyze_json["stats"];
    assert_eq!(stats["total_vertices"], 4);
    assert_eq!(stats["total_edges"], 2);
    assert_eq!(stats["vertices_per_label"]["Person"], 4);
    assert_eq!(stats["edges_per_type"]["KNOWS"], 2);
}




