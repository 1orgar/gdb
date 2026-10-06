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
