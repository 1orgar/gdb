use arrow::array::AsArray;
use gdb_core::schema::GraphSchema;
use gdb_parser::parse;
use gdb_planner::QueryExecutor;
use gdb_storage::PartitionStorageEngine;
use parking_lot::RwLock;
use std::sync::Arc;

fn create_planner_environment() -> (Arc<RwLock<GraphSchema>>, Arc<PartitionStorageEngine>, QueryExecutor) {
    let schema = Arc::new(RwLock::new(GraphSchema::new("planner_test")));
    let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
    let executor = QueryExecutor::new(schema.clone(), storage.clone());
    (schema, storage, executor)
}

#[test]
fn test_complex_with_clause_pipeline() {
    let (_, storage, executor) = create_planner_environment();

    executor.execute(parse("CREATE VERTEX Employee (name STRING, department STRING, salary INT64)").unwrap()).unwrap();
    executor.execute(parse("CREATE EDGE MANAGES ()").unwrap()).unwrap();

    // Insert employees across departments
    let emps = [
        (1, "Alice", "Engineering", 120_000),
        (2, "Bob", "Engineering", 95_000),
        (3, "Charlie", "Engineering", 110_000),
        (4, "Diana", "Marketing", 85_000),
        (5, "Evan", "Marketing", 90_000),
        (6, "Frank", "Sales", 75_000),
    ];
    for (id, name, dept, sal) in emps {
        executor.execute(parse(&format!(
            "INSERT VERTEX Employee (id, name, department, salary) VALUES ({}, '{}', '{}', {})",
            id, name, dept, sal
        )).unwrap()).unwrap();
    }

    storage.compact();

    // WITH pipeline with grouping, aggregation, filtering, and ordering
    let q = "MATCH (e:Employee) WITH e.department AS dept, count(e) AS head_count, avg(e.salary) AS avg_sal WHERE head_count > 1 RETURN dept, head_count, avg_sal ORDER BY dept ASC";
    let res = executor.execute(parse(q).unwrap()).unwrap();
    let batch = res.batch.expect("Batch should be returned");
    assert_eq!(batch.num_rows(), 2); // Engineering and Marketing

    let depts = batch.column(0).as_string::<i32>();
    assert_eq!(depts.value(0), "Engineering");
    assert_eq!(depts.value(1), "Marketing");

    let counts = batch.column(1).as_primitive::<arrow::datatypes::Int64Type>();
    assert_eq!(counts.value(0), 3);
    assert_eq!(counts.value(1), 2);
}

#[test]
fn test_variable_length_multi_hop_traversal() {
    let (_, storage, executor) = create_planner_environment();

    executor.execute(parse("CREATE VERTEX Node (name STRING)").unwrap()).unwrap();
    executor.execute(parse("CREATE EDGE LINKS ()").unwrap()).unwrap();

    // Create a chain 1 -> 2 -> 3 -> 4 -> 5
    for i in 1..=5 {
        executor.execute(parse(&format!("INSERT VERTEX Node (id, name) VALUES ({}, 'N{}')", i, i)).unwrap()).unwrap();
    }
    for i in 1..4 {
        executor.execute(parse(&format!("INSERT EDGE LINKS FROM {} TO {}", i, i + 1)).unwrap()).unwrap();
    }
    executor.execute(parse("INSERT EDGE LINKS FROM 4 TO 5").unwrap()).unwrap();

    storage.compact();

    // 1..2 hops from Node 1
    let q2 = "MATCH (a:Node)-[:LINKS*1..2]->(b:Node) WHERE a.id = 1 RETURN b.name";
    let res2 = executor.execute(parse(q2).unwrap()).unwrap();
    let b2 = res2.batch.expect("Batch returned");
    // Should reach node 2 and node 3
    assert_eq!(b2.num_rows(), 2);

    // 1..4 hops from Node 1
    let q4 = "MATCH (a:Node)-[:LINKS*1..4]->(b:Node) WHERE a.id = 1 RETURN b.name";
    let res4 = executor.execute(parse(q4).unwrap()).unwrap();
    let b4 = res4.batch.expect("Batch returned");
    // Should reach 2, 3, 4, 5
    assert_eq!(b4.num_rows(), 4);
}

#[test]
fn test_ddl_and_dml_full_lifecycle() {
    let (schema, storage, executor) = create_planner_environment();

    // 1. DDL Create
    executor.execute(parse("CREATE VERTEX Customer (name STRING, credit INT64)").unwrap()).unwrap();
    executor.execute(parse("CREATE EDGE OWES (interest FLOAT64)").unwrap()).unwrap();

    // 2. DDL Index
    executor.execute(parse("CREATE INDEX ON Customer(credit)").unwrap()).unwrap();
    {
        let s = schema.read();
        assert!(s.has_index("Customer", "credit"));
    }

    // 3. Insert and Query
    executor.execute(parse("INSERT VERTEX Customer (id, name, credit) VALUES (1, 'Alice', 5000)").unwrap()).unwrap();
    executor.execute(parse("INSERT VERTEX Customer (id, name, credit) VALUES (2, 'Bob', 2000)").unwrap()).unwrap();
    executor.execute(parse("INSERT EDGE OWES FROM 2 TO 1").unwrap()).unwrap();

    // 4. Cypher DML SET property mutation
    executor.execute(parse("MATCH (c:Customer) WHERE c.id = 2 SET c.credit = 2500").unwrap()).unwrap();

    // Verify index lookup after update
    let res = executor.execute(parse("MATCH (c:Customer) WHERE c.credit = 2500 RETURN c.name").unwrap()).unwrap();
    let b = res.batch.unwrap();
    assert_eq!(b.num_rows(), 1);

    // 5. DDL Alter
    executor.execute(parse("ALTER VERTEX Customer ADD (tier STRING)").unwrap()).unwrap();
    {
        let s = schema.read();
        let vs = s.get_vertex_schema("Customer").unwrap();
        assert!(vs.properties.iter().any(|p| p.name == "tier"));
    }

    // 6. Cypher DML DELETE
    executor.execute(parse("DELETE EDGE OWES FROM 2 TO 1").unwrap()).unwrap();
    storage.compact();

    let res_e = executor.execute(parse("MATCH (a:Customer)-[:OWES]->(b:Customer) RETURN a.name, b.name").unwrap()).unwrap();
    assert_eq!(res_e.rows_affected, 0);
    assert!(res_e.batch.is_none() || res_e.batch.unwrap().num_rows() == 0);

    // 7. DDL Drop Index and Drop Vertex
    executor.execute(parse("DROP INDEX ON Customer(credit)").unwrap()).unwrap();
    executor.execute(parse("DROP EDGE OWES").unwrap()).unwrap();
    executor.execute(parse("DROP VERTEX Customer").unwrap()).unwrap();

    {
        let s = schema.read();
        assert!(s.get_vertex_schema("Customer").is_none());
        assert!(s.get_edge_schema("OWES").is_none());
    }
}

#[test]
fn test_query_explain_execution() {
    let (_, storage, executor) = create_planner_environment();

    executor.execute(parse("CREATE VERTEX User (name STRING, score INT64)").unwrap()).unwrap();
    executor.execute(parse("CREATE EDGE FOLLOWS ()").unwrap()).unwrap();
    executor.execute(parse("CREATE INDEX ON User(score)").unwrap()).unwrap();

    executor.execute(parse("INSERT VERTEX User (id, name, score) VALUES (1, 'A', 10)").unwrap()).unwrap();
    executor.execute(parse("INSERT VERTEX User (id, name, score) VALUES (2, 'B', 20)").unwrap()).unwrap();
    executor.execute(parse("INSERT EDGE FOLLOWS FROM 1 TO 2").unwrap()).unwrap();
    storage.compact();

    // Explain query with index scan
    let res = executor.execute(parse("EXPLAIN MATCH (u:User) WHERE u.score = 20 RETURN u.name").unwrap()).unwrap();
    assert!(res.batch.is_some());
    let b = res.batch.unwrap();
    assert!(b.num_rows() >= 1);
    assert_eq!(b.num_columns(), 4);
    let op_name = b.column(1).as_string::<i32>().value(0);
    assert!(!op_name.is_empty());
    assert!(res.message.contains("IndexScan") || res.message.contains("Scan"));
}
