use gdb_core::schema::{DataType, PropertySpec, VertexSchema};
use gdb_core::{DataValue, LabelId, VertexId};
use gdb_storage::VertexPropertyTable;
use std::collections::HashMap;

#[test]
fn test_vertex_property_table_lifecycle() {
    let schema_def = VertexSchema::new(
        "User",
        LabelId(1),
        vec![
            PropertySpec::new("name", DataType::String, false),
            PropertySpec::new("age", DataType::Int64, true),
            PropertySpec::new("score", DataType::Float64, true),
        ],
    );

    let table = VertexPropertyTable::new(&schema_def);
    assert!(table.is_empty());
    assert_eq!(table.len(), 0);

    // Insert vertex 100
    let mut props_100 = HashMap::new();
    props_100.insert("name".to_string(), DataValue::String("Alice".into()));
    props_100.insert("age".to_string(), DataValue::Int64(30));
    props_100.insert("score".to_string(), DataValue::Float64(95.5));
    table.set_properties(VertexId(100), props_100);

    // Insert vertex 101
    let mut props_101 = HashMap::new();
    props_101.insert("name".to_string(), DataValue::String("Bob".into()));
    props_101.insert("age".to_string(), DataValue::Int64(30));
    props_101.insert("score".to_string(), DataValue::Float64(88.0));
    table.set_properties(VertexId(101), props_101);

    assert_eq!(table.len(), 2);
    assert!(!table.is_empty());
    assert_eq!(table.vertex_ids().len(), 2);

    // Query property
    assert_eq!(table.get_property(VertexId(100), "name"), Some(DataValue::String("Alice".into())));
    assert_eq!(table.get_property(VertexId(101), "age"), Some(DataValue::Int64(30)));
    assert_eq!(table.get_property(VertexId(999), "name"), None);

    // Update property
    table.update_property(VertexId(100), "score", DataValue::Float64(99.0));
    assert_eq!(table.get_property(VertexId(100), "score"), Some(DataValue::Float64(99.0)));

    // Indexes
    table.create_index("age");
    assert!(table.has_index("age"));
    assert!(!table.has_index("unknown"));

    let age_30_vids = table.lookup_by_index("age", &DataValue::Int64(30)).unwrap();
    assert_eq!(age_30_vids.len(), 2);
    assert!(age_30_vids.contains(&VertexId(100)));
    assert!(age_30_vids.contains(&VertexId(101)));

    // Rebuild Arrow batches
    table.rebuild_arrow_batches().unwrap();
    let batches = table.get_batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].num_rows(), 2);
    assert_eq!(batches[0].num_columns(), 4); // _vertex_id + name + age + score

    // Drop property
    table.drop_property("score");
    assert_eq!(table.get_property(VertexId(100), "score"), None);

    // Drop index
    assert!(table.drop_index("age"));
    assert!(!table.has_index("age"));

    // Delete vertex
    assert!(table.delete_vertex(VertexId(100)));
    assert_eq!(table.len(), 1);
    assert!(!table.delete_vertex(VertexId(100))); // second delete returns false
}

#[test]
fn test_vector_similarity_and_graph_statistics() {
    use gdb_core::schema::GraphSchema;
    use gdb_storage::PartitionStorageEngine;
    use parking_lot::RwLock;
    use std::sync::Arc;

    let schema_def = VertexSchema::new(
        "Document",
        LabelId(2),
        vec![
            PropertySpec::new("title", DataType::String, false),
            PropertySpec::new("emb", DataType::Vector(3), false),
        ],
    );

    let table = VertexPropertyTable::new(&schema_def);

    let mut p1 = HashMap::new();
    p1.insert("title".to_string(), DataValue::String("Doc1".into()));
    p1.insert("emb".to_string(), DataValue::Vector(vec![1.0, 0.0, 0.0]));
    table.set_properties(VertexId(1), p1);

    let mut p2 = HashMap::new();
    p2.insert("title".to_string(), DataValue::String("Doc2".into()));
    p2.insert("emb".to_string(), DataValue::Vector(vec![0.0, 1.0, 0.0]));
    table.set_properties(VertexId(2), p2);

    let mut p3 = HashMap::new();
    p3.insert("title".to_string(), DataValue::String("Doc3".into()));
    p3.insert("emb".to_string(), DataValue::Vector(vec![0.8, 0.2, 0.0]));
    table.set_properties(VertexId(3), p3);

    // Vector similarity search
    let matches = table.vector_similarity_search("emb", &[1.0, 0.0, 0.0], 2, "cosine");
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].0, VertexId(1));
    assert_eq!(matches[1].0, VertexId(3));

    // Analyze graph statistics
    let mut schema = GraphSchema::new("test_stats");
    let label_id = schema.register_vertex_label("Document", vec![
        PropertySpec::new("title", DataType::String, false),
    ]).unwrap();
    let edge_type = schema.register_edge_type("REF", vec![]).unwrap();

    let storage = PartitionStorageEngine::new(0, Arc::new(RwLock::new(schema)));
    storage.set_vertex_properties(VertexId(10), label_id, HashMap::new()).unwrap();
    storage.set_vertex_properties(VertexId(20), label_id, HashMap::new()).unwrap();
    storage.insert_edge(gdb_core::EdgeId::new(VertexId(10), edge_type, 1, VertexId(20)), 1);

    let stats = storage.analyze_graph();
    assert_eq!(stats.total_vertices, 2);
    assert_eq!(stats.total_edges, 1);
    assert_eq!(stats.vertices_per_label.get("Document"), Some(&2));
    assert_eq!(stats.edges_per_type.get("REF"), Some(&1));
}
