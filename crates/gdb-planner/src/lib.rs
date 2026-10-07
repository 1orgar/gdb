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
}
