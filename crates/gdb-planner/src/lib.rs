pub mod eval;
pub mod executor;
pub mod plan;

pub use eval::{eval_expr, PathRow};
pub use executor::{QueryResult, QueryExecutor};
pub use plan::PhysicalOperator;

#[cfg(test)]
mod tests {
    use super::*;
    use gdb_core::schema::GraphSchema;
    use gdb_parser::parse;
    use gdb_storage::PartitionStorageEngine;
    use parking_lot::RwLock;
    use std::sync::Arc;

    #[test]
    fn test_end_to_end_cypher_execution() {
        let schema = Arc::new(RwLock::new(GraphSchema::new("social")));
        let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
        let executor = QueryExecutor::new(schema, storage.clone());

        // 1. DDL
        let ddl1 = parse("CREATE VERTEX Person (name STRING, age INT64)").unwrap();
        executor.execute(ddl1).unwrap();

        let ddl2 = parse("CREATE EDGE KNOWS ()").unwrap();
        executor.execute(ddl2).unwrap();

        // 2. DML Vertices
        let ins_v1 = parse("INSERT VERTEX Person (id, name, age) VALUES (1, 'Alice', 30)").unwrap();
        executor.execute(ins_v1).unwrap();

        let ins_v2 = parse("INSERT VERTEX Person (id, name, age) VALUES (2, 'Bob', 25)").unwrap();
        executor.execute(ins_v2).unwrap();

        let ins_v3 = parse("INSERT VERTEX Person (id, name, age) VALUES (3, 'Charlie', 35)").unwrap();
        executor.execute(ins_v3).unwrap();

        // 3. DML Edges
        let ins_e1 = parse("INSERT EDGE KNOWS FROM 1 TO 2").unwrap();
        executor.execute(ins_e1).unwrap();

        let ins_e2 = parse("INSERT EDGE KNOWS FROM 2 TO 3").unwrap();
        executor.execute(ins_e2).unwrap();

        // Compact into CSR
        storage.compact();

        // 4. Query 1-hop
        let q1 = parse("MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE a.age > 28 RETURN a.name, b.name").unwrap();
        let res1 = executor.execute(q1).unwrap();
        let batch1 = res1.batch.expect("Batch returned");
        assert_eq!(batch1.num_rows(), 1);

        // 5. Query 2-hop traversal: Alice -> Bob -> Charlie
        let q2 = parse("MATCH (a:Person)-[:KNOWS]->(b:Person)-[:KNOWS]->(c:Person) RETURN a.name, b.name, c.name").unwrap();
        let res2 = executor.execute(q2).unwrap();
        let batch2 = res2.batch.expect("Batch returned");
        assert_eq!(batch2.num_rows(), 1);
        assert_eq!(batch2.num_columns(), 3);

        // 6. Test COUNT(*)
        let q3 = parse("MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN COUNT(*)").unwrap();
        let res3 = executor.execute(q3).unwrap();
        let batch3 = res3.batch.expect("Count batch returned");
        assert_eq!(batch3.num_rows(), 1);
        let count_col = batch3.column(0).as_any().downcast_ref::<arrow::array::Int64Array>().unwrap();
        assert_eq!(count_col.value(0), 2);
    }

    #[test]
    fn test_execute_call_algorithm() {
        let schema = Arc::new(RwLock::new(GraphSchema::new("test")));
        let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
        let executor = QueryExecutor::new(schema, storage.clone());

        // Setup triangle graph
        let _ = executor.execute(parse("CREATE VERTEX User ()").unwrap());
        let _ = executor.execute(parse("CREATE EDGE KNOWS ()").unwrap());
        let _ = executor.execute(parse("INSERT VERTEX User (id) VALUES (1)").unwrap());
        let _ = executor.execute(parse("INSERT VERTEX User (id) VALUES (2)").unwrap());
        let _ = executor.execute(parse("INSERT VERTEX User (id) VALUES (3)").unwrap());
        let _ = executor.execute(parse("INSERT EDGE KNOWS FROM 1 TO 2").unwrap());
        let _ = executor.execute(parse("INSERT EDGE KNOWS FROM 2 TO 3").unwrap());
        let _ = executor.execute(parse("INSERT EDGE KNOWS FROM 3 TO 1").unwrap());
        storage.compact();

        // 1. Run PageRank via CALL
        let call_pr = parse("CALL algo.pageRank({damping: 0.85, max_iter: 10}) YIELD vertex_id, score").unwrap();
        let res_pr = executor.execute(call_pr).unwrap();
        let batch_pr = res_pr.batch.expect("Batch returned");
        assert_eq!(batch_pr.num_rows(), 3);
        assert_eq!(batch_pr.num_columns(), 2);

        // 2. Run WCC via CALL
        let call_wcc = parse("CALL algo.wcc() YIELD vertex_id, component_id").unwrap();
        let res_wcc = executor.execute(call_wcc).unwrap();
        let batch_wcc = res_wcc.batch.expect("Batch returned");
        assert_eq!(batch_wcc.num_rows(), 3);

        // 3. Run Triangles via CALL
        let call_tri = parse("CALL algo.triangleCount() YIELD vertex_id, triangles").unwrap();
        let res_tri = executor.execute(call_tri).unwrap();
        let batch_tri = res_tri.batch.expect("Batch returned");
        assert_eq!(batch_tri.num_rows(), 3);
        assert_eq!(batch_tri.num_columns(), 2);
    }

    #[test]
    fn test_bulk_insert_vertices_and_edges() {
        let schema = Arc::new(RwLock::new(GraphSchema::new("bulk")));
        let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
        let executor = QueryExecutor::new(schema, storage.clone());

        executor.execute(parse("CREATE VERTEX User (name STRING, age INT64)").unwrap()).unwrap();
        executor.execute(parse("CREATE EDGE FOLLOWS ()").unwrap()).unwrap();

        // Multi-tuple VERTEX insert
        let ins_v = parse("INSERT VERTEX User (id, name, age) VALUES (1, 'Alice', 30), (2, 'Bob', 25), (3, 'Charlie', 35)").unwrap();
        let res_v = executor.execute(ins_v).unwrap();
        assert_eq!(res_v.rows_affected, 3);

        // Multi-tuple EDGE insert
        let ins_e = parse("INSERT EDGE FOLLOWS VALUES (1, 2), (2, 3)").unwrap();
        let res_e = executor.execute(ins_e).unwrap();
        assert_eq!(res_e.rows_affected, 2);

        storage.compact();

        let q = parse("MATCH (a:User)-[:FOLLOWS]->(b:User) RETURN a.name, b.name").unwrap();
        let res = executor.execute(q).unwrap();
        let batch = res.batch.unwrap();
        assert_eq!(batch.num_rows(), 2);
    }

    #[test]
    fn test_multi_hop_variable_length_traversal() {
        let schema = Arc::new(RwLock::new(GraphSchema::new("multihop")));
        let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
        let executor = QueryExecutor::new(schema, storage.clone());

        // Linear chain: 1 -> 2 -> 3 -> 4
        executor.execute(parse("CREATE VERTEX Person (name STRING)").unwrap()).unwrap();
        executor.execute(parse("CREATE EDGE KNOWS ()").unwrap()).unwrap();

        executor.execute(parse("INSERT VERTEX Person (id, name) VALUES (1, 'A'), (2, 'B'), (3, 'C'), (4, 'D')").unwrap()).unwrap();
        executor.execute(parse("INSERT EDGE KNOWS VALUES (1, 2), (2, 3), (3, 4)").unwrap()).unwrap();
        storage.compact();

        // 1..2 hops from 1
        let q1 = parse("MATCH (a:Person)-[:KNOWS*1..2]->(b:Person) WHERE a.name = 'A' RETURN b.name").unwrap();
        let res1 = executor.execute(q1).unwrap();
        let b1 = res1.batch.unwrap();
        // Should find 2 (1 hop) and 3 (2 hops)
        assert_eq!(b1.num_rows(), 2);

        // Exact 2 hops from 1
        let q2 = parse("MATCH (a:Person)-[:KNOWS*2]->(b:Person) WHERE a.name = 'A' RETURN b.name").unwrap();
        let res2 = executor.execute(q2).unwrap();
        let b2 = res2.batch.unwrap();
        // Should find only 3 (2 hops)
        assert_eq!(b2.num_rows(), 1);

        // 1..3 hops from 1
        let q3 = parse("MATCH (a:Person)-[:KNOWS*1..3]->(b:Person) WHERE a.name = 'A' RETURN b.name").unwrap();
        let res3 = executor.execute(q3).unwrap();
        let b3 = res3.batch.unwrap();
        // Should find 2, 3, 4 (3 rows)
        assert_eq!(b3.num_rows(), 3);
    }

    #[test]
    fn test_secondary_index_and_explain() {
        let schema = Arc::new(RwLock::new(GraphSchema::new("indexed")));
        let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
        let executor = QueryExecutor::new(schema, storage.clone());

        executor.execute(parse("CREATE VERTEX User (name STRING, email STRING)").unwrap()).unwrap();
        executor.execute(parse("CREATE INDEX ON :User(email)").unwrap()).unwrap();

        executor.execute(parse("INSERT VERTEX User (id, name, email) VALUES (1, 'Alice', 'alice@test.com'), (2, 'Bob', 'bob@test.com')").unwrap()).unwrap();

        // Query with indexed lookup
        let q = parse("MATCH (u:User) WHERE u.email = 'alice@test.com' RETURN u.name").unwrap();
        let res = executor.execute(q).unwrap();
        let batch = res.batch.unwrap();
        assert_eq!(batch.num_rows(), 1);

        // Explain query
        let explain_q = parse("EXPLAIN MATCH (u:User) WHERE u.email = 'alice@test.com' RETURN u.name").unwrap();
        let explain_res = executor.execute(explain_q).unwrap();
        assert!(explain_res.message.contains("IndexScan"));
        let explain_batch = explain_res.batch.unwrap();
        assert!(explain_batch.num_rows() >= 1);

        // Drop index
        let drop_res = executor.execute(parse("DROP INDEX ON :User(email)").unwrap()).unwrap();
        assert!(drop_res.message.contains("Dropped"));
    }

    #[test]
    fn test_cypher_dml_set_and_delete() {
        let schema = Arc::new(RwLock::new(GraphSchema::new("dml")));
        let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
        let executor = QueryExecutor::new(schema, storage.clone());

        executor.execute(parse("CREATE VERTEX User (name STRING, age INT64)").unwrap()).unwrap();
        executor.execute(parse("INSERT VERTEX User (id, name, age) VALUES (10, 'Eve', 22)").unwrap()).unwrap();

        // 1. SET
        let set_q = parse("MATCH (u:User) WHERE u.id = 10 SET u.age = 23 RETURN u.name, u.age").unwrap();
        let set_res = executor.execute(set_q).unwrap();
        let set_b = set_res.batch.unwrap();
        assert_eq!(set_b.num_rows(), 1);

        // 2. MERGE
        let merge_q = parse("MERGE VERTEX User (id, name, age) VALUES (11, 'Mallory', 30)").unwrap();
        let merge_res = executor.execute(merge_q).unwrap();
        assert_eq!(merge_res.rows_affected, 1);

        // 3. DELETE
        let del_q = parse("MATCH (u:User) WHERE u.id = 10 DETACH DELETE u").unwrap();
        let del_res = executor.execute(del_q).unwrap();
        assert_eq!(del_res.rows_affected, 1);

        // Confirm deleted
        let check_q = parse("MATCH (u:User) WHERE u.id = 10 RETURN u.name").unwrap();
        let check_res = executor.execute(check_q).unwrap();
        assert_eq!(check_res.rows_affected, 0);
    }

    #[test]
    fn test_ordering_pagination_aggregations() {
        let schema = Arc::new(RwLock::new(GraphSchema::new("agg")));
        let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
        let executor = QueryExecutor::new(schema, storage.clone());

        executor.execute(parse("CREATE VERTEX Emp (city STRING, salary INT64)").unwrap()).unwrap();
        executor.execute(parse("INSERT VERTEX Emp (id, city, salary) VALUES (1, 'SF', 100), (2, 'SF', 200), (3, 'NY', 150), (4, 'NY', 250)").unwrap()).unwrap();

        // 1. ORDER BY & SKIP/LIMIT
        let sort_q = parse("MATCH (e:Emp) RETURN e.salary ORDER BY e.salary DESC SKIP 1 LIMIT 2").unwrap();
        let sort_res = executor.execute(sort_q).unwrap();
        let sort_b = sort_res.batch.unwrap();
        assert_eq!(sort_b.num_rows(), 2);

        // 2. Aggregations with GROUP BY: city, COUNT(*), SUM(salary), AVG(salary), MIN(salary), MAX(salary)
        let agg_q = parse("MATCH (e:Emp) RETURN e.city, COUNT(*), SUM(e.salary), AVG(e.salary), MIN(e.salary), MAX(e.salary)").unwrap();
        let agg_res = executor.execute(agg_q).unwrap();
        let agg_b = agg_res.batch.unwrap();
        assert_eq!(agg_b.num_rows(), 2); // 2 cities: SF, NY
        assert_eq!(agg_b.num_columns(), 6);
    }

    #[test]
    fn test_ddl_and_schema_management() {
        let schema = Arc::new(RwLock::new(GraphSchema::new("test_ddl")));
        let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
        let executor = QueryExecutor::new(schema.clone(), storage.clone());

        // 1. Create schemas
        executor.execute(parse("CREATE VERTEX Person (name STRING, age INT64)").unwrap()).unwrap();
        executor.execute(parse("CREATE EDGE KNOWS ()").unwrap()).unwrap();

        // 2. SHOW SCHEMA
        let show_res = executor.execute(parse("SHOW SCHEMA").unwrap()).unwrap();
        assert_eq!(show_res.rows_affected, 2);

        // 3. SHOW TAGS
        let show_tags = executor.execute(parse("SHOW TAGS").unwrap()).unwrap();
        assert_eq!(show_tags.rows_affected, 1);

        // 4. ALTER VERTEX ADD
        let alter_res = executor.execute(parse("ALTER VERTEX Person ADD (email STRING)").unwrap()).unwrap();
        assert!(alter_res.message.contains("Altered"));
        assert!(schema.read().get_vertex_schema("Person").unwrap().properties.iter().any(|p| p.name == "email"));

        // 5. ALTER VERTEX DROP
        let alter_drop = executor.execute(parse("ALTER VERTEX Person DROP (email)").unwrap()).unwrap();
        assert!(alter_drop.message.contains("Altered"));
        assert!(!schema.read().get_vertex_schema("Person").unwrap().properties.iter().any(|p| p.name == "email"));

        // 6. DROP VERTEX & DROP EDGE
        let drop_v = executor.execute(parse("DROP VERTEX Person").unwrap()).unwrap();
        assert!(drop_v.message.contains("Dropped"));
        assert!(schema.read().get_vertex_schema("Person").is_none());

        let drop_e = executor.execute(parse("DROP EDGE KNOWS").unwrap()).unwrap();
        assert!(drop_e.message.contains("Dropped"));
        assert!(schema.read().get_edge_schema("KNOWS").is_none());
    }

    #[test]
    fn test_cypher_with_clause() {
        let schema = Arc::new(RwLock::new(GraphSchema::new("with_test")));
        let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
        let executor = QueryExecutor::new(schema, storage.clone());

        executor.execute(parse("CREATE VERTEX Person (name STRING, age INT64)").unwrap()).unwrap();
        executor.execute(parse("CREATE EDGE KNOWS ()").unwrap()).unwrap();

        executor.execute(parse("INSERT VERTEX Person (id, name, age) VALUES (1, 'Alice', 30)").unwrap()).unwrap();
        executor.execute(parse("INSERT VERTEX Person (id, name, age) VALUES (2, 'Bob', 25)").unwrap()).unwrap();
        executor.execute(parse("INSERT VERTEX Person (id, name, age) VALUES (3, 'Charlie', 35)").unwrap()).unwrap();
        executor.execute(parse("INSERT EDGE KNOWS FROM 1 TO 2").unwrap()).unwrap();
        executor.execute(parse("INSERT EDGE KNOWS FROM 1 TO 3").unwrap()).unwrap();
        executor.execute(parse("INSERT EDGE KNOWS FROM 2 TO 3").unwrap()).unwrap();

        // 1. Simple WITH projection and filtering
        let q1 = parse("MATCH (p:Person) WITH p, p.age AS age WHERE age > 28 RETURN p.name, age ORDER BY age ASC").unwrap();
        let res1 = executor.execute(q1).unwrap();
        let b1 = res1.batch.unwrap();
        assert_eq!(b1.num_rows(), 2); // Alice (30) and Charlie (35)

        // 2. WITH aggregation (friends count)
        let q2 = parse("MATCH (a:Person)-[:KNOWS]->(b:Person) WITH a, count(b) AS friends WHERE friends > 1 RETURN a.name, friends").unwrap();
        let res2 = executor.execute(q2).unwrap();
        let b2 = res2.batch.unwrap();
        assert_eq!(b2.num_rows(), 1); // Only Alice has 2 friends (> 1)

        // 3. WITH chained into next MATCH
        let q3 = parse("MATCH (a:Person) WITH a WHERE a.age > 28 MATCH (a)-[:KNOWS]->(b:Person) RETURN a.name, b.name").unwrap();
        let res3 = executor.execute(q3).unwrap();
        let b3 = res3.batch.unwrap();
        assert_eq!(b3.num_rows(), 2); // Alice->Bob and Alice->Charlie
    }
}
