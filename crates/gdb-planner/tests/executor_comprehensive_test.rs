use gdb_core::schema::GraphSchema;
use gdb_parser::parse;
use gdb_planner::QueryExecutor;
use gdb_storage::PartitionStorageEngine;
use parking_lot::RwLock;
use std::sync::Arc;

fn setup_executor() -> (Arc<QueryExecutor>, Arc<PartitionStorageEngine>, Arc<RwLock<GraphSchema>>) {
    let schema = Arc::new(RwLock::new(GraphSchema::new("test_exec_db")));
    let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
    let executor = Arc::new(QueryExecutor::new(schema.clone(), storage.clone()));
    (executor, storage, schema)
}

#[test]
fn test_executor_schema_introspection_and_alter() {
    let (executor, _, _) = setup_executor();

    // 1. Create label & edge type
    executor.execute(parse("CREATE VERTEX Account (name STRING, balance FLOAT64);").unwrap()).unwrap();
    executor.execute(parse("CREATE EDGE TRANSFERS (amount FLOAT64, fee FLOAT64);").unwrap()).unwrap();

    // 2. Show schema, tags, edges
    let res = executor.execute(parse("SHOW SCHEMA;").unwrap()).unwrap();
    assert!(res.batch.is_some());
    assert_eq!(res.batch.unwrap().num_rows(), 2); // 1 vertex + 1 edge

    let res_tags = executor.execute(parse("SHOW TAGS;").unwrap()).unwrap();
    assert!(res_tags.batch.is_some());

    let res_edges = executor.execute(parse("SHOW EDGES;").unwrap()).unwrap();
    assert!(res_edges.batch.is_some());

    // 3. Create & drop index
    executor.execute(parse("CREATE INDEX ON :Account(name);").unwrap()).unwrap();
    executor.execute(parse("DROP INDEX ON :Account(name);").unwrap()).unwrap();

    // 4. Alter vertex (add & drop property)
    executor.execute(parse("ALTER VERTEX Account ADD (status STRING);").unwrap()).unwrap();
    executor.execute(parse("ALTER VERTEX Account DROP (status);").unwrap()).unwrap();

    // 5. Alter edge (add & drop property)
    executor.execute(parse("ALTER EDGE TRANSFERS ADD (timestamp INT64);").unwrap()).unwrap();
    executor.execute(parse("ALTER EDGE TRANSFERS DROP (timestamp);").unwrap()).unwrap();

    // 6. Drop edge and vertex
    executor.execute(parse("DROP EDGE TRANSFERS;").unwrap()).unwrap();
    executor.execute(parse("DROP VERTEX Account;").unwrap()).unwrap();
}

#[test]
fn test_executor_dml_and_pattern_queries() {
    let (executor, storage, _) = setup_executor();

    executor.execute(parse("CREATE VERTEX Person (name STRING, age INT64, active BOOLEAN);").unwrap()).unwrap();
    executor.execute(parse("CREATE EDGE KNOWS (weight FLOAT64);").unwrap()).unwrap();

    // Insert vertices
    executor.execute(parse("INSERT VERTEX Person (id, name, age, active) VALUES (1, 'Alice', 30, true), (2, 'Bob', 25, false), (3, 'Charlie', 40, true);").unwrap()).unwrap();

    // Insert edges
    executor.execute(parse("INSERT EDGE KNOWS FROM 1 TO 2;").unwrap()).unwrap();
    executor.execute(parse("INSERT EDGE KNOWS FROM 2 TO 3;").unwrap()).unwrap();
    executor.execute(parse("INSERT EDGE KNOWS FROM 3 TO 1;").unwrap()).unwrap();

    // Storage compaction into CSR
    storage.compact();

    // Query with filter
    let q1 = executor.execute(parse("MATCH (p:Person) WHERE p.age > 25 RETURN p.name, p.age;").unwrap()).unwrap();
    assert!(q1.batch.is_some());
    assert_eq!(q1.batch.unwrap().num_rows(), 2); // Alice and Charlie

    // Query with AND / OR
    let q2 = executor.execute(parse("MATCH (p:Person) WHERE p.age < 28 OR p.name = 'Charlie' RETURN p.name;").unwrap()).unwrap();
    assert!(q2.batch.is_some());
    assert_eq!(q2.batch.unwrap().num_rows(), 2); // Bob and Charlie

    // Query multi-hop traversal
    let q3 = executor.execute(parse("MATCH (a:Person)-[:KNOWS*1..2]->(b:Person) RETURN a.name, b.name;").unwrap()).unwrap();
    assert!(q3.batch.is_some());

    // WITH clause aggregation
    let q4 = executor.execute(parse("MATCH (p:Person) WITH p.age AS age, count(p) AS cnt RETURN age, cnt;").unwrap()).unwrap();
    assert!(q4.batch.is_some());

    // EXPLAIN query plan
    let q_exp = executor.execute(parse("EXPLAIN MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name;").unwrap()).unwrap();
    assert!(q_exp.message.contains("Plan") || q_exp.message.contains("Scan"));
}

#[test]
fn test_executor_algorithms() {
    let (executor, storage, _) = setup_executor();

    executor.execute(parse("CREATE VERTEX Node (val INT64);").unwrap()).unwrap();
    executor.execute(parse("CREATE EDGE LINK ();").unwrap()).unwrap();

    executor.execute(parse("INSERT VERTEX Node (id, val) VALUES (1, 10), (2, 20), (3, 30);").unwrap()).unwrap();
    executor.execute(parse("INSERT EDGE LINK FROM 1 TO 2;").unwrap()).unwrap();
    executor.execute(parse("INSERT EDGE LINK FROM 2 TO 3;").unwrap()).unwrap();
    executor.execute(parse("INSERT EDGE LINK FROM 3 TO 1;").unwrap()).unwrap();

    storage.compact();

    // PageRank
    let pr = executor.execute(parse("CALL algo.pageRank({damping: 0.85, max_iter: 10}) YIELD vertex_id, score;").unwrap()).unwrap();
    assert!(pr.batch.is_some());
    assert_eq!(pr.batch.unwrap().num_rows(), 3);

    // WCC
    let wcc = executor.execute(parse("CALL algo.wcc() YIELD vertex_id, component_id;").unwrap()).unwrap();
    assert!(wcc.batch.is_some());
    assert_eq!(wcc.batch.unwrap().num_rows(), 3);

    // Triangle Count
    let tri = executor.execute(parse("CALL algo.triangleCount() YIELD vertex_id, triangles;").unwrap()).unwrap();
    assert!(tri.batch.is_some());
    assert_eq!(tri.batch.unwrap().num_rows(), 3);

    // Louvain
    let louvain = executor.execute(parse("CALL algo.louvain({max_iter: 5}) YIELD vertex_id, community_id;").unwrap()).unwrap();
    assert!(louvain.batch.is_some());
    assert_eq!(louvain.batch.unwrap().num_rows(), 3);
}

#[test]
fn test_executor_error_handling() {
    let (executor, _, _) = setup_executor();

    // Unknown vertex label
    assert!(executor.execute(parse("INSERT VERTEX Unknown (id) VALUES (1);").unwrap()).is_err());

    // Unknown edge type
    assert!(executor.execute(parse("INSERT EDGE Unknown FROM 1 TO 2;").unwrap()).is_err());

    // Drop non-existent schema
    assert!(executor.execute(parse("DROP VERTEX NoSuchLabel;").unwrap()).is_err());
    assert!(executor.execute(parse("DROP EDGE NoSuchEdge;").unwrap()).is_err());
}

#[test]
fn test_eval_expressions_and_operators() {
    use gdb_core::DataValue;
    use gdb_parser::ast::BinaryOperator;
    use gdb_planner::eval::{eval_binary_op, eval_expr, PathRow};

    let schema = Arc::new(RwLock::new(GraphSchema::new("test")));
    let storage = Arc::new(PartitionStorageEngine::new(0, schema));
    let row = PathRow::default();

    // Binary operations
    assert_eq!(
        eval_binary_op(&DataValue::Int64(10), &BinaryOperator::Plus, &DataValue::Int64(20)).unwrap(),
        DataValue::Int64(30)
    );
    assert_eq!(
        eval_binary_op(&DataValue::Float64(2.5), &BinaryOperator::Plus, &DataValue::Float64(3.5)).unwrap(),
        DataValue::Float64(6.0)
    );
    assert_eq!(
        eval_binary_op(&DataValue::String("hello ".into()), &BinaryOperator::Plus, &DataValue::String("world".into())).unwrap(),
        DataValue::String("hello world".into())
    );
    assert_eq!(
        eval_binary_op(&DataValue::Int64(25), &BinaryOperator::Minus, &DataValue::Int64(5)).unwrap(),
        DataValue::Int64(20)
    );
    assert_eq!(
        eval_binary_op(&DataValue::Float64(10.0), &BinaryOperator::Minus, &DataValue::Float64(2.5)).unwrap(),
        DataValue::Float64(7.5)
    );

    // Comparisons
    assert_eq!(eval_binary_op(&DataValue::Int64(10), &BinaryOperator::Eq, &DataValue::Int64(10)).unwrap(), DataValue::Boolean(true));
    assert_eq!(eval_binary_op(&DataValue::Int64(10), &BinaryOperator::NotEq, &DataValue::Int64(20)).unwrap(), DataValue::Boolean(true));
    assert_eq!(eval_binary_op(&DataValue::Int64(5), &BinaryOperator::Lt, &DataValue::Int64(10)).unwrap(), DataValue::Boolean(true));
    assert_eq!(eval_binary_op(&DataValue::Int64(10), &BinaryOperator::LtEq, &DataValue::Int64(10)).unwrap(), DataValue::Boolean(true));
    assert_eq!(eval_binary_op(&DataValue::Int64(15), &BinaryOperator::Gt, &DataValue::Int64(10)).unwrap(), DataValue::Boolean(true));
    assert_eq!(eval_binary_op(&DataValue::Int64(10), &BinaryOperator::GtEq, &DataValue::Int64(10)).unwrap(), DataValue::Boolean(true));

    // Boolean logic
    assert_eq!(eval_binary_op(&DataValue::Boolean(true), &BinaryOperator::And, &DataValue::Boolean(false)).unwrap(), DataValue::Boolean(false));
    assert_eq!(eval_binary_op(&DataValue::Boolean(true), &BinaryOperator::Or, &DataValue::Boolean(false)).unwrap(), DataValue::Boolean(true));

    // Literal eval
    assert_eq!(eval_expr(&gdb_parser::ast::Expr::Literal(DataValue::Int64(99)), &row, &storage).unwrap(), DataValue::Int64(99));

    // Error cases
    assert!(eval_expr(&gdb_parser::ast::Expr::Variable("missing".into()), &row, &storage).is_err());
    assert!(eval_expr(&gdb_parser::ast::Expr::CountStar, &row, &storage).is_err());
    assert!(eval_expr(&gdb_parser::ast::Expr::FunctionCall { name: "unknown".into(), args: vec![] }, &row, &storage).is_err());
}

#[test]
fn test_explain_and_mutations() {
    let (executor, _, _) = setup_executor();

    // Explain non-query statements (direct execution branch)
    let exp_ddl = executor.execute(parse("EXPLAIN CREATE VERTEX Foo (bar STRING);").unwrap()).unwrap();
    assert!(exp_ddl.message.contains("Direct Execution Plan"));

    let exp_dml = executor.execute(parse("EXPLAIN INSERT VERTEX Foo (id, bar) VALUES (1, 'val');").unwrap()).unwrap();
    assert!(exp_dml.message.contains("Direct Execution Plan"));

    // Execute the statements
    executor.execute(parse("CREATE VERTEX Foo (bar STRING);").unwrap()).unwrap();
    executor.execute(parse("INSERT VERTEX Foo (id, bar) VALUES (1, 'val');").unwrap()).unwrap();

    // Query with DISTINCT, SKIP, LIMIT
    let res = executor.execute(parse("MATCH (f:Foo) RETURN DISTINCT f.bar SKIP 0 LIMIT 5;").unwrap()).unwrap();
    assert!(res.batch.is_some());
    assert_eq!(res.batch.unwrap().num_rows(), 1);
}

#[test]
fn test_executor_v050_features() {
    let schema = Arc::new(RwLock::new(GraphSchema::new("test_v050")));
    let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
    let gpu = Arc::new(gdb_gpu::GpuDispatcher::disabled());
    let executor = QueryExecutor::with_gpu(schema.clone(), storage.clone(), gpu);

    // 1. DDL with Vector type
    executor.execute(parse("CREATE VERTEX Item (name STRING, vec VECTOR(3));").unwrap()).unwrap();
    executor.execute(parse("CREATE EDGE SIMILAR ();").unwrap()).unwrap();

    // 2. Insert items with Vector values
    executor.execute(parse("INSERT VERTEX Item (id, name, vec) VALUES (1, 'ItemA', [1.0, 0.0, 0.0]);").unwrap()).unwrap();
    executor.execute(parse("INSERT VERTEX Item (id, name, vec) VALUES (2, 'ItemB', [0.0, 1.0, 0.0]);").unwrap()).unwrap();
    executor.execute(parse("INSERT VERTEX Item (id, name, vec) VALUES (3, 'ItemC', [0.9, 0.1, 0.0]);").unwrap()).unwrap();

    // 3. ANALYZE GRAPH
    let stats_res = executor.execute(parse("ANALYZE GRAPH;").unwrap()).unwrap();
    assert!(stats_res.message.contains("Analyzed graph"));

    // 4. Vector similarity search (cosine, dot, l2)
    let search_res = executor.execute(parse(
        "CALL vector.similaritySearch('Item', 'vec', [1.0, 0.0, 0.0], 2, 'cosine') YIELD vertex_id, score;"
    ).unwrap()).unwrap();
    assert!(search_res.batch.is_some());
    let b = search_res.batch.unwrap();
    assert_eq!(b.num_rows(), 2);

    let dot_res = executor.execute(parse(
        "CALL vector.similaritySearch('Item', 'vec', [1.0, 0.0, 0.0], 2, 'dot') YIELD vertex_id, score;"
    ).unwrap()).unwrap();
    assert!(dot_res.batch.is_some());

    let l2_res = executor.execute(parse(
        "CALL vector.similaritySearch('Item', 'vec', [1.0, 0.0, 0.0], 2, 'l2') YIELD vertex_id, score;"
    ).unwrap()).unwrap();
    assert!(l2_res.batch.is_some());

    // 5. Cypher Relationship DML (CREATE / MERGE)
    let create_edge_res = executor.execute(parse(
        "MATCH (a:Item), (b:Item) CREATE (a)-[r:SIMILAR]->(b);"
    ).unwrap()).unwrap();
    assert!(create_edge_res.rows_affected > 0);

    let merge_edge_res = executor.execute(parse(
        "MATCH (a:Item), (b:Item) MERGE (a)-[r:SIMILAR]->(b);"
    ).unwrap()).unwrap();
    assert!(merge_edge_res.message.contains("Query completed"));

    storage.compact();

    // 6. Node2Vec Graph ML
    let n2v_res = executor.execute(parse(
        "CALL algo.node2vec({walk_length: 4, walks_per_vertex: 2, dimensions: 8}) YIELD vertex_id, embedding;"
    ).unwrap()).unwrap();
    assert!(n2v_res.batch.is_some());
}

#[test]
fn test_where_id_pushdown_and_explain() {
    let (executor, storage, _) = setup_executor();

    executor.execute(parse("CREATE VERTEX Device (name STRING);").unwrap()).unwrap();
    executor.execute(parse("CREATE EDGE CONNECTED ();").unwrap()).unwrap();

    executor.execute(parse("INSERT VERTEX Device (id, name) VALUES (1, 'Gateway'), (2, 'Sensor1'), (3, 'Sensor2');").unwrap()).unwrap();
    executor.execute(parse("INSERT EDGE CONNECTED FROM 1 TO 2;").unwrap()).unwrap();
    executor.execute(parse("INSERT EDGE CONNECTED FROM 1 TO 3;").unwrap()).unwrap();
    storage.compact();

    // 1. Verify EXPLAIN shows id_filter pushdown in ScanVertices
    let explain_res = executor.execute(parse("EXPLAIN MATCH (d:Device) WHERE d.id = 1 RETURN d.name;").unwrap()).unwrap();
    assert!(explain_res.message.contains("ScanVertices") && explain_res.message.contains("id: 1"), "Plan must have id_filter pushdown: {}", explain_res.message);
    assert!(!explain_res.message.contains("Filter:"), "Redundant Filter operator should not be added when pushed down: {}", explain_res.message);

    // 2. Verify query execution result
    let res = executor.execute(parse("MATCH (d:Device) WHERE d.id = 1 RETURN d.name;").unwrap()).unwrap();
    let batch = res.batch.unwrap();
    assert_eq!(batch.num_rows(), 1);
    let name_col = batch.column(0).as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
    assert_eq!(name_col.value(0), "Gateway");

    // 3. Verify non-existent ID returns 0 rows
    let res_none = executor.execute(parse("MATCH (d:Device) WHERE d.id = 9999 RETURN d.name;").unwrap()).unwrap();
    let count_none = res_none.batch.map(|b| b.num_rows()).unwrap_or(0);
    assert_eq!(count_none, 0);

    // 4. Multi-hop traversal with WHERE a.id = 1
    let res_hop = executor.execute(parse("MATCH (a:Device)-[:CONNECTED]->(b:Device) WHERE a.id = 1 RETURN b.name;").unwrap()).unwrap();
    assert_eq!(res_hop.batch.unwrap().num_rows(), 2);
}

