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
}
