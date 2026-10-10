use gdb_parser::ast::*;
use gdb_parser::{parse, parse_script, split_statements};

#[test]
fn test_parser_ddl_and_indexes() {
    // 1. CREATE VERTEX
    let stmt = parse("CREATE VERTEX User (name STRING, age INT64, active BOOLEAN, salary FLOAT64);").unwrap();
    assert!(matches!(stmt, Statement::CreateVertexLabel { .. }));

    // 2. ALTER VERTEX ADD & DROP
    let stmt_add = parse("ALTER VERTEX User ADD (email STRING);").unwrap();
    if let Statement::AlterVertexLabel { label, add_properties, drop_properties } = stmt_add {
        assert_eq!(label, "User");
        assert_eq!(add_properties.len(), 1);
        assert!(drop_properties.is_empty());
    } else {
        panic!("Expected AlterVertexLabel");
    }

    let stmt_drop = parse("ALTER VERTEX User DROP (salary);").unwrap();
    if let Statement::AlterVertexLabel { label, add_properties, drop_properties } = stmt_drop {
        assert_eq!(label, "User");
        assert!(add_properties.is_empty());
        assert_eq!(drop_properties, vec!["salary"]);
    } else {
        panic!("Expected AlterVertexLabel");
    }

    // 3. DROP VERTEX
    let stmt = parse("DROP VERTEX User;").unwrap();
    assert!(matches!(stmt, Statement::DropVertexLabel { .. }));

    // 4. CREATE EDGE
    let stmt = parse("CREATE EDGE FRIENDS_WITH (since INT64, strength FLOAT64);").unwrap();
    assert!(matches!(stmt, Statement::CreateEdgeType { .. }));

    // 5. ALTER EDGE
    let stmt = parse("ALTER EDGE FRIENDS_WITH ADD (verified BOOLEAN);").unwrap();
    assert!(matches!(stmt, Statement::AlterEdgeType { .. }));

    let stmt = parse("ALTER EDGE FRIENDS_WITH DROP (strength);").unwrap();
    assert!(matches!(stmt, Statement::AlterEdgeType { .. }));

    // 6. DROP EDGE
    let stmt = parse("DROP EDGE FRIENDS_WITH;").unwrap();
    assert!(matches!(stmt, Statement::DropEdgeType { .. }));

    // 7. CREATE INDEX
    let stmt = parse("CREATE INDEX ON :User(name);").unwrap();
    assert!(matches!(stmt, Statement::CreateIndex { .. }));

    // 8. DROP INDEX
    let stmt = parse("DROP INDEX ON :User(name);").unwrap();
    assert!(matches!(stmt, Statement::DropIndex { .. }));

    // 9. SHOW SCHEMA, TAGS, EDGES
    assert_eq!(parse("SHOW SCHEMA;").unwrap(), Statement::ShowSchema);
    assert_eq!(parse("SHOW TAGS;").unwrap(), Statement::ShowVertexLabels);
    assert_eq!(parse("SHOW EDGES;").unwrap(), Statement::ShowEdgeTypes);
    assert_eq!(parse("SHOW VERTEX LABELS;").unwrap(), Statement::ShowVertexLabels);
    assert_eq!(parse("SHOW EDGE TYPES;").unwrap(), Statement::ShowEdgeTypes);
}

#[test]
fn test_parser_dml_mutations() {
    // Insert single vertex
    let stmt = parse("INSERT VERTEX Person (id, name, age) VALUES (10, 'Alice', 30);").unwrap();
    assert!(matches!(stmt, Statement::InsertVertex { .. } | Statement::InsertVertices { .. }));

    // Insert batch vertices
    let stmt = parse("INSERT VERTEX Person (id, name) VALUES (1, 'A'), (2, 'B');").unwrap();
    assert!(matches!(stmt, Statement::InsertVertices { .. }));

    // Insert edge
    let stmt = parse("INSERT EDGE FOLLOWS FROM 10 TO 20;").unwrap();
    assert!(matches!(stmt, Statement::InsertEdge { .. } | Statement::InsertEdges { .. }));
}

#[test]
fn test_parser_cypher_queries_and_expressions() {
    // Simple Match
    let stmt = parse("MATCH (a:Person) WHERE a.age >= 18 AND a.name != 'Bob' RETURN a.name, a.age").unwrap();
    assert!(matches!(stmt, Statement::Query(_)));

    // Multi-hop edge match
    let stmt = parse("MATCH (a:Person)-[:KNOWS*1..3]->(b:Person) RETURN b.name").unwrap();
    assert!(matches!(stmt, Statement::Query(_)));

    // WITH clause with aggregations, order by, limit
    let stmt = parse("MATCH (u:User) WITH u.age AS age, count(u) AS cnt WHERE cnt > 1 RETURN age, cnt ORDER BY age DESC LIMIT 10").unwrap();
    if let Statement::Query(q) = stmt {
        assert!(q.with_clause.is_some());
    } else {
        panic!("Expected Query with with_clause");
    }

    // Graph algorithm calls
    let stmt = parse("CALL algo.pageRank({damping: 0.85, max_iter: 20}) YIELD vertex_id, score;").unwrap();
    assert!(matches!(stmt, Statement::CallAlgorithm { .. }));

    let stmt = parse("CALL algo.wcc() YIELD vertex_id, component_id;").unwrap();
    assert!(matches!(stmt, Statement::CallAlgorithm { .. }));

    let stmt = parse("CALL algo.louvain({max_iter: 10}) YIELD vertex_id, community_id;").unwrap();
    assert!(matches!(stmt, Statement::CallAlgorithm { .. }));

    let stmt = parse("CALL algo.triangleCount() YIELD vertex_id, triangles;").unwrap();
    assert!(matches!(stmt, Statement::CallAlgorithm { .. }));

    // EXPLAIN query
    let stmt = parse("EXPLAIN MATCH (a)-[:REL]->(b) RETURN a, b").unwrap();
    assert!(matches!(stmt, Statement::Explain(_)));
}

#[test]
fn test_parser_errors() {
    assert!(parse("").is_err());
    assert!(parse("INVALID SYNTAX STATEMENT").is_err());
    assert!(parse("CREATE VERTEX").is_err());
    assert!(parse("MATCH (a) WHERE").is_err());
}

#[test]
fn test_parser_v050_features() {
    // 1. split_statements and parse_script
    let script = "CREATE VERTEX V (vec VECTOR(3)); INSERT VERTEX V (id, vec) VALUES (1, [0.1, 0.2, 0.3]); ANALYZE GRAPH;";
    let parts = split_statements(script);
    assert_eq!(parts.len(), 3);
    let parsed_stmts = parse_script(script).unwrap();
    assert_eq!(parsed_stmts.len(), 3);

    // 2. Vector property and literals
    let stmt = parse("CREATE VERTEX Doc (title STRING, emb VECTOR(64));").unwrap();
    assert!(matches!(stmt, Statement::CreateVertexLabel { .. }));

    let stmt = parse("INSERT VERTEX Doc (id, title, emb) VALUES (1, 'Paper', [1.0, 2.0, 3.0]);").unwrap();
    assert!(matches!(stmt, Statement::InsertVertex { .. }));

    // 3. ANALYZE GRAPH
    let stmt = parse("ANALYZE GRAPH;").unwrap();
    assert!(matches!(stmt, Statement::AnalyzeGraph));

    // 4. Edge mutation in Cypher
    let stmt = parse("MATCH (a:User), (b:User) CREATE (a)-[r:FOLLOWS]->(b);").unwrap();
    assert!(matches!(stmt, Statement::Query(_)));

    let stmt = parse("MATCH (a:User), (b:User) MERGE (a)-[r:FOLLOWS]->(b);").unwrap();
    assert!(matches!(stmt, Statement::Query(_)));

    // 5. Node2Vec and Vector search
    let stmt = parse("CALL algo.node2vec({walk_length: 10, dimensions: 32}) YIELD vertex_id, embedding;").unwrap();
    assert!(matches!(stmt, Statement::CallAlgorithm { .. }));

    let stmt = parse("CALL vector.similaritySearch('Doc', 'emb', [1.0, 0.0], 5, 'cosine') YIELD vertex_id, score;").unwrap();
    assert!(matches!(stmt, Statement::CallAlgorithm { .. }));
}

