use gdb_server::create_test_server;
use serde_json::Value;

#[tokio::test]
async fn test_e2e_http_server_full_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, server_handle, _app_state) = create_test_server(tmp.path().to_path_buf())
        .await
        .expect("Failed to start test server");

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", addr);

    // 1. Health check
    let resp = client.get(format!("{}/health", base_url)).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let health: Value = resp.json().await.unwrap();
    assert_eq!(health["status"], "UP");
    assert_eq!(health["version"], "0.4.2");
    assert_eq!(health["role"], "Peer");

    // 2. Cluster check
    let resp = client.get(format!("{}/cluster", base_url)).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let cluster: Value = resp.json().await.unwrap();
    assert_eq!(cluster["status"], "UP");
    assert_eq!(cluster["cluster_topology"], "leaderless-ring");

    // 3. Clean catalog check (no default entities!)
    let resp = client.get(format!("{}/schema", base_url)).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let schema: Value = resp.json().await.unwrap();
    assert_eq!(schema["status"], "ok");
    let vertices = schema["vertices"].as_array().unwrap();
    let edges = schema["edges"].as_array().unwrap();
    assert!(vertices.is_empty(), "Catalog must start clean with 0 vertex tags");
    assert!(edges.is_empty(), "Catalog must start clean with 0 edge types");

    // 4. DDL - Create Vertex Tag & Edge Type
    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({
            "query": "CREATE VERTEX Person (name STRING, age INT64)"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");

    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({
            "query": "CREATE EDGE FRIEND (since INT64)"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");

    // Verify Schema after DDL
    let resp = client.get(format!("{}/schema", base_url)).send().await.unwrap();
    let schema: Value = resp.json().await.unwrap();
    let vertices = schema["vertices"].as_array().unwrap();
    let edges = schema["edges"].as_array().unwrap();
    assert_eq!(vertices.len(), 1);
    assert_eq!(vertices[0]["label"], "Person");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["edge_type_name"], "FRIEND");

    // 5. Schema Alteration: Add property to Person
    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({
            "query": "ALTER VERTEX Person ADD (city STRING)"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");

    // 6. Secondary Index Creation
    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({
            "query": "CREATE INDEX ON Person(age)"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");

    // 7. DML - Insert Vertices and Edges
    let inserts = [
        "INSERT VERTEX Person (id, name, age, city) VALUES (1, 'Alice', 30, 'Zurich')",
        "INSERT VERTEX Person (id, name, age, city) VALUES (2, 'Bob', 25, 'Geneva')",
        "INSERT VERTEX Person (id, name, age, city) VALUES (3, 'Charlie', 35, 'Basel')",
        "INSERT EDGE FRIEND FROM 1 TO 2",
        "INSERT EDGE FRIEND FROM 2 TO 3",
    ];
    for q in &inserts {
        let res = client
            .post(format!("{}/query", base_url))
            .json(&serde_json::json!({ "query": q }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let json: Value = res.json().await.unwrap();
        assert_eq!(json["status"], "ok", "Failed query: {}", q);
    }

    // 8. Cypher Queries
    // Match simple vertex query
    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({
            "query": "MATCH (p:Person) RETURN p.name, p.age"
        }))
        .send()
        .await
        .unwrap();
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["num_rows"], 3);

    // Multi-hop edge traversal
    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({
            "query": "MATCH (a:Person)-[:FRIEND]->(b:Person) RETURN a.name, b.name"
        }))
        .send()
        .await
        .unwrap();
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["num_rows"], 2);

    // 9. openCypher WITH clause query pipeline
    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({
            "query": "MATCH (a:Person)-[:FRIEND]->(b:Person) WITH b.name AS friend_name, b.age AS friend_age WHERE friend_age > 20 RETURN friend_name, friend_age"
        }))
        .send()
        .await
        .unwrap();
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["num_rows"], 2);

    // 10. Storage Compaction Endpoint
    let res = client.post(format!("{}/compact", base_url)).send().await.unwrap();
    assert_eq!(res.status(), 200);
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["message"], "Compaction completed");

    // 11. Graph Analytics CALL algorithm (WCC and PageRank)
    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({
            "query": "CALL algo.wcc() YIELD vertex_id, component_id"
        }))
        .send()
        .await
        .unwrap();
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["num_rows"], 3);

    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({
            "query": "CALL algo.pageRank({damping: 0.85, max_iter: 10}) YIELD vertex_id, score"
        }))
        .send()
        .await
        .unwrap();
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["num_rows"], 3);

    // 12. Resources & Observability Metrics
    let res = client.get(format!("{}/resources", base_url)).send().await.unwrap();
    assert_eq!(res.status(), 200);
    let res_json: Value = res.json().await.unwrap();
    assert!(res_json["total_vertices"].as_u64().unwrap() >= 3);
    assert_eq!(res_json["compactions_total"], 1);

    let res = client.get(format!("{}/metrics", base_url)).send().await.unwrap();
    assert_eq!(res.status(), 200);
    let metrics_text = res.text().await.unwrap();
    assert!(metrics_text.contains("gdb_queries_total"));
    assert!(metrics_text.contains("gdb_cluster_replication_factor"));

    // 13. Error Handling - Invalid query returns structured error
    let res = client
        .post(format!("{}/query", base_url))
        .json(&serde_json::json!({
            "query": "INVALID SYNTAX QUERY ???"
        }))
        .send()
        .await
        .unwrap();
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "error");
    assert!(json["error"].as_str().unwrap().contains("error") || json["error"].as_str().unwrap().contains("Syntax"));

    server_handle.abort();
}

#[tokio::test]
async fn test_e2e_ddl_and_dml_modifications() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, server_handle, _app_state) = create_test_server(tmp.path().to_path_buf())
        .await
        .expect("Failed to start test server");

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", addr);

    // Create DDL
    client.post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": "CREATE VERTEX Account (balance INT64, status STRING)" }))
        .send().await.unwrap();

    client.post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": "CREATE EDGE TRANSFERS (amount INT64)" }))
        .send().await.unwrap();

    client.post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": "CREATE INDEX ON Account(balance)" }))
        .send().await.unwrap();

    // Verify index exists
    let res = client.get(format!("{}/schema", base_url)).send().await.unwrap();
    let schema: Value = res.json().await.unwrap();
    assert_eq!(schema["vertices"].as_array().unwrap().len(), 1);

    // Insert accounts
    for i in 1..=5 {
        client.post(format!("{}/query", base_url))
            .json(&serde_json::json!({
                "query": format!("INSERT VERTEX Account (id, balance, status) VALUES ({}, {}, 'ACTIVE')", i, i * 100)
            }))
            .send().await.unwrap();
    }

    // Insert transfers
    client.post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": "INSERT EDGE TRANSFERS FROM 1 TO 2" }))
        .send().await.unwrap();
    client.post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": "INSERT EDGE TRANSFERS FROM 2 TO 3" }))
        .send().await.unwrap();

    // Aggregations test
    let res = client.post(format!("{}/query", base_url))
        .json(&serde_json::json!({
            "query": "MATCH (a:Account) RETURN count(a) AS total_accs, sum(a.balance) AS total_balance"
        }))
        .send().await.unwrap();
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["num_rows"], 1);

    // openCypher EXPLAIN query
    let res = client.post(format!("{}/query", base_url))
        .json(&serde_json::json!({
            "query": "EXPLAIN MATCH (a:Account)-[:TRANSFERS]->(b:Account) RETURN a.balance, b.balance"
        }))
        .send().await.unwrap();
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");
    assert!(json["rows"].as_array().unwrap().len() >= 1);

    // Delete edge
    let res = client.post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": "DELETE EDGE TRANSFERS FROM 1 TO 2" }))
        .send().await.unwrap();
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");

    // Drop index & DDL drop
    let res = client.post(format!("{}/query", base_url))
        .json(&serde_json::json!({ "query": "DROP INDEX ON Account(balance)" }))
        .send().await.unwrap();
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");

    server_handle.abort();
}

#[tokio::test]
async fn test_e2e_replicate_endpoint() {
    let tmp = tempfile::tempdir().unwrap();
    let (addr, server_handle, _app_state) = create_test_server(tmp.path().to_path_buf())
        .await
        .expect("Failed to start test server");

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", addr);

    // Replicate DDL
    let res = client.post(format!("{}/replicate", base_url))
        .json(&serde_json::json!({
            "query": "CREATE VERTEX Device (mac STRING)"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");

    // Replicate DML
    let res = client.post(format!("{}/replicate", base_url))
        .json(&serde_json::json!({
            "query": "INSERT VERTEX Device (id, mac) VALUES (10, '00:1A:2B:3C:4D:5E')"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");

    // Replicate Compaction
    let res = client.post(format!("{}/replicate", base_url))
        .json(&serde_json::json!({
            "query": "compact;"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");

    // Verify Query sees replicated data
    let res = client.post(format!("{}/query", base_url))
        .json(&serde_json::json!({
            "query": "MATCH (d:Device) RETURN d.mac"
        }))
        .send().await.unwrap();
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["num_rows"], 1);

    server_handle.abort();
}

