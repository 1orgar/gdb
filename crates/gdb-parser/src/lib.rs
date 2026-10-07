pub mod ast;
pub mod parser;

pub use ast::*;
pub use parser::{parse, Lexer, Parser};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_create_and_insert() {
        let sql = "CREATE VERTEX User (name STRING, age INT64)";
        let stmt = parse(sql).unwrap();
        match stmt {
            Statement::CreateVertexLabel { label, properties } => {
                assert_eq!(label, "User");
                assert_eq!(properties.len(), 2);
            }
            _ => panic!("Expected CreateVertexLabel"),
        }

        let insert_sql = "INSERT VERTEX User (id, name, age) VALUES (1001, 'Bob', 28)";
        let stmt = parse(insert_sql).unwrap();
        match stmt {
            Statement::InsertVertex { label, id, properties } => {
                assert_eq!(label, "User");
                assert_eq!(id.0, 1001);
                assert_eq!(properties.len(), 2);
            }
            _ => panic!("Expected InsertVertex"),
        }
    }

    #[test]
    fn test_parse_cypher_match() {
        let cypher = "MATCH (a:User)-[:KNOWS]->(b:User) WHERE a.age > 21 RETURN b.name, b.age LIMIT 5";
        let stmt = parse(cypher).unwrap();
        match stmt {
            Statement::Query(q) => {
                assert_eq!(q.pattern.start_node.variable, Some("a".into()));
                assert_eq!(q.pattern.start_node.label, Some("User".into()));
                assert_eq!(q.pattern.hops.len(), 1);
                assert_eq!(q.pattern.hops[0].0.edge_type, Some("KNOWS".into()));
                assert_eq!(q.pattern.hops[0].1.variable, Some("b".into()));
                assert!(q.where_clause.is_some());
                assert_eq!(q.return_items.len(), 2);
                assert_eq!(q.limit, Some(5));
            }
            _ => panic!("Expected Query statement"),
        }
    }

    #[test]
    fn test_parse_call_algorithm() {
        let sql = "CALL algo.pageRank({damping: 0.85, max_iter: 20}) YIELD vertex_id, score";
        let stmt = parse(sql).unwrap();
        match stmt {
            Statement::CallAlgorithm { algorithm, args, yield_items } => {
                assert_eq!(algorithm, "algo.pageRank");
                assert_eq!(args.len(), 2);
                assert_eq!(yield_items, vec!["vertex_id", "score"]);
            }
            _ => panic!("Expected CallAlgorithm"),
        }
    }

    #[test]
    fn test_parse_batch_insert_vertices_and_edges() {
        let v_sql = "INSERT VERTEX User (id, name, age) VALUES (1, 'Alice', 30), (2, 'Bob', 25)";
        let stmt = parse(v_sql).unwrap();
        match stmt {
            Statement::InsertVertices { label, vertices } => {
                assert_eq!(label, "User");
                assert_eq!(vertices.len(), 2);
                assert_eq!(vertices[0].0 .0, 1);
                assert_eq!(vertices[1].0 .0, 2);
            }
            _ => panic!("Expected InsertVertices"),
        }

        let e_sql = "INSERT EDGE FOLLOWS VALUES (1, 2), (2, 3, 10)";
        let stmt = parse(e_sql).unwrap();
        match stmt {
            Statement::InsertEdges { edge_type, edges } => {
                assert_eq!(edge_type, "FOLLOWS");
                assert_eq!(edges.len(), 2);
                assert_eq!(edges[0].0 .0, 1);
                assert_eq!(edges[0].1 .0, 2);
                assert_eq!(edges[0].2, 0);
                assert_eq!(edges[1].0 .0, 2);
                assert_eq!(edges[1].1 .0, 3);
                assert_eq!(edges[1].2, 10);
            }
            _ => panic!("Expected InsertEdges"),
        }
    }

    #[test]
    fn test_parse_multi_hop_patterns() {
        let q1 = "MATCH (a)-[:KNOWS*1..3]->(b) RETURN b";
        if let Statement::Query(q) = parse(q1).unwrap() {
            assert_eq!(q.pattern.hops[0].0.min_hops, 1);
            assert_eq!(q.pattern.hops[0].0.max_hops, Some(3));
        } else {
            panic!("Expected Query");
        }

        let q2 = "MATCH (a)-[:KNOWS*..5]->(b) RETURN b";
        if let Statement::Query(q) = parse(q2).unwrap() {
            assert_eq!(q.pattern.hops[0].0.min_hops, 1);
            assert_eq!(q.pattern.hops[0].0.max_hops, Some(5));
        } else {
            panic!("Expected Query");
        }

        let q3 = "MATCH (a)-[:KNOWS*2]->(b) RETURN b";
        if let Statement::Query(q) = parse(q3).unwrap() {
            assert_eq!(q.pattern.hops[0].0.min_hops, 2);
            assert_eq!(q.pattern.hops[0].0.max_hops, Some(2));
        } else {
            panic!("Expected Query");
        }

        let q4 = "MATCH (a)-[*]->(b) RETURN b";
        if let Statement::Query(q) = parse(q4).unwrap() {
            assert_eq!(q.pattern.hops[0].0.min_hops, 1);
            assert_eq!(q.pattern.hops[0].0.max_hops, None);
        } else {
            panic!("Expected Query");
        }
    }
}
