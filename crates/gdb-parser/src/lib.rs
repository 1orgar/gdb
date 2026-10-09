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
    fn test_parse_indexes() {
        let create_idx = "CREATE INDEX ON :User(email)";
        let stmt = parse(create_idx).unwrap();
        match stmt {
            Statement::CreateIndex { label, property } => {
                assert_eq!(label, "User");
                assert_eq!(property, "email");
            }
            _ => panic!("Expected CreateIndex"),
        }

        let drop_idx = "DROP INDEX ON :User(email)";
        let stmt2 = parse(drop_idx).unwrap();
        match stmt2 {
            Statement::DropIndex { label, property } => {
                assert_eq!(label, "User");
                assert_eq!(property, "email");
            }
            _ => panic!("Expected DropIndex"),
        }
    }

    #[test]
    fn test_parse_explain() {
        let sql = "EXPLAIN MATCH (n:User) RETURN n.name";
        let stmt = parse(sql).unwrap();
        match stmt {
            Statement::Explain(inner) => match *inner {
                Statement::Query(q) => {
                    assert_eq!(q.pattern.start_node.variable, Some("n".into()));
                    assert_eq!(q.return_items.len(), 1);
                }
                _ => panic!("Expected Query inside Explain"),
            },
            _ => panic!("Expected Explain"),
        }
    }

    #[test]
    fn test_parse_mutations_and_merge() {
        let set_sql = "MATCH (n:User) WHERE n.id = 1001 SET n.age = 29, n.status = 'active'";
        let stmt = parse(set_sql).unwrap();
        match stmt {
            Statement::Query(q) => {
                assert_eq!(q.updates.len(), 2);
                match &q.updates[0] {
                    UpdateClause::Set { variable, property, .. } => {
                        assert_eq!(variable, "n");
                        assert_eq!(property, "age");
                    }
                    _ => panic!("Expected Set clause"),
                }
            }
            _ => panic!("Expected Query"),
        }

        let delete_sql = "MATCH (n:User) WHERE n.id = 1001 DETACH DELETE n";
        let stmt = parse(delete_sql).unwrap();
        match stmt {
            Statement::Query(q) => {
                assert_eq!(q.updates.len(), 1);
                match &q.updates[0] {
                    UpdateClause::Delete { variable, detach } => {
                        assert_eq!(variable, "n");
                        assert!(*detach);
                    }
                    _ => panic!("Expected Delete clause"),
                }
            }
            _ => panic!("Expected Query"),
        }

        let merge_sql = "MERGE VERTEX User (id, name) VALUES (1001, 'Alice')";
        let stmt = parse(merge_sql).unwrap();
        match stmt {
            Statement::MergeVertex { label, id, properties } => {
                assert_eq!(label, "User");
                assert_eq!(id.0, 1001);
                assert_eq!(properties.len(), 1);
            }
            _ => panic!("Expected MergeVertex"),
        }

        let merge_pattern = "MERGE (n:User {id: 1002, name: 'Charlie'})";
        let stmt = parse(merge_pattern).unwrap();
        match stmt {
            Statement::MergeVertex { label, id, properties } => {
                assert_eq!(label, "User");
                assert_eq!(id.0, 1002);
                assert_eq!(properties.len(), 1);
            }
            _ => panic!("Expected MergeVertex"),
        }
    }

    #[test]
    fn test_parse_distinct_order_by_skip_limit() {
        let sql = "MATCH (n:User) RETURN DISTINCT n.name ORDER BY n.age DESC SKIP 10 LIMIT 5";
        let stmt = parse(sql).unwrap();
        match stmt {
            Statement::Query(q) => {
                assert!(q.distinct);
                assert_eq!(q.return_items.len(), 1);
                assert_eq!(q.order_by.len(), 1);
                assert!(!q.order_by[0].ascending);
                assert_eq!(q.skip, Some(10));
                assert_eq!(q.limit, Some(5));
            }
            _ => panic!("Expected Query"),
        }
    }

    #[test]
    fn test_parse_aggregations() {
        let sql = "MATCH (n:User) RETURN COUNT(*), SUM(n.age), AVG(n.salary), MIN(n.score), MAX(n.score)";
        let stmt = parse(sql).unwrap();
        match stmt {
            Statement::Query(q) => {
                assert_eq!(q.return_items.len(), 5);
                assert!(q.return_items[0].expr.is_aggregate());
                assert!(q.return_items[1].expr.is_aggregate());
                assert!(q.return_items[2].expr.is_aggregate());
                assert!(q.return_items[3].expr.is_aggregate());
                assert!(q.return_items[4].expr.is_aggregate());
            }
            _ => panic!("Expected Query"),
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

    #[test]
    fn test_parse_ddl_drop_alter_show() {
        assert_eq!(
            parse("DROP VERTEX Person;").unwrap(),
            Statement::DropVertexLabel {
                label: "Person".into()
            }
        );
        assert_eq!(
            parse("DROP TAG Person;").unwrap(),
            Statement::DropVertexLabel {
                label: "Person".into()
            }
        );
        assert_eq!(
            parse("DROP EDGE KNOWS;").unwrap(),
            Statement::DropEdgeType {
                edge_type: "KNOWS".into()
            }
        );

        let alter_v = parse("ALTER VERTEX Person ADD (email STRING, score FLOAT64);").unwrap();
        match alter_v {
            Statement::AlterVertexLabel {
                label,
                add_properties,
                drop_properties,
            } => {
                assert_eq!(label, "Person");
                assert_eq!(add_properties.len(), 2);
                assert!(drop_properties.is_empty());
            }
            _ => panic!("Expected AlterVertexLabel"),
        }

        let alter_v_drop = parse("ALTER VERTEX Person DROP (email);").unwrap();
        match alter_v_drop {
            Statement::AlterVertexLabel {
                label,
                add_properties,
                drop_properties,
            } => {
                assert_eq!(label, "Person");
                assert!(add_properties.is_empty());
                assert_eq!(drop_properties, vec!["email".to_string()]);
            }
            _ => panic!("Expected AlterVertexLabel"),
        }

        assert_eq!(parse("SHOW SCHEMA;").unwrap(), Statement::ShowSchema);
        assert_eq!(parse("SHOW TAGS;").unwrap(), Statement::ShowVertexLabels);
        assert_eq!(parse("SHOW EDGES;").unwrap(), Statement::ShowEdgeTypes);
    }

    #[test]
    fn test_parse_with_clause() {
        let q1 = "MATCH (a:Person) WITH a, a.age AS age WHERE age > 20 RETURN a.name, age;";
        match parse(q1).unwrap() {
            Statement::Query(q) => {
                let with = q.with_clause.expect("Expected with_clause");
                assert_eq!(with.items.len(), 2);
                assert_eq!(with.items[1].alias, Some("age".into()));
                assert!(with.where_clause.is_some());
                assert_eq!(q.return_items.len(), 2);
            }
            _ => panic!("Expected Query"),
        }

        let q2 = "MATCH (a:Person)-[:KNOWS]->(b:Person) WITH a, count(b) AS friends ORDER BY friends DESC LIMIT 10 MATCH (a)-[:WORKS_AT]->(c) RETURN a.name, friends, c.name;";
        match parse(q2).unwrap() {
            Statement::Query(q) => {
                let with = q.with_clause.expect("Expected with_clause");
                assert_eq!(with.items.len(), 2);
                assert_eq!(with.limit, Some(10));
                assert_eq!(with.order_by.len(), 1);
                assert!(!with.order_by[0].ascending);
                assert!(q.next_match.is_some());
                assert_eq!(q.return_items.len(), 3);
            }
            _ => panic!("Expected Query"),
        }
    }
}
