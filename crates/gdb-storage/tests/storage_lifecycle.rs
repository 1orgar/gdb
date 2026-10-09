use gdb_core::schema::{DataType, GraphSchema, PropertySpec};
use gdb_core::{DataValue, EdgeId, VertexId};
use gdb_storage::PartitionStorageEngine;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

#[test]
fn test_storage_lifecycle_high_volume_compaction() {
    let mut schema = GraphSchema::new("lifecycle_graph");
    let account_label = schema
        .register_vertex_label(
            "Account",
            vec![
                PropertySpec::new("name", DataType::String, false),
                PropertySpec::new("balance", DataType::Int64, false),
            ],
        )
        .unwrap();
    let transfer_edge = schema
        .register_edge_type("TRANSFER", vec![PropertySpec::new("amount", DataType::Int64, false)])
        .unwrap();

    let engine = PartitionStorageEngine::new(0, Arc::new(RwLock::new(schema)));

    // 1. Insert 500 vertices
    for i in 1..=500u64 {
        let vid = VertexId(i);
        let mut props = HashMap::new();
        props.insert("name".to_string(), DataValue::String(format!("user_{}", i)));
        props.insert("balance".to_string(), DataValue::Int64((i * 100) as i64));
        engine.set_vertex_properties(vid, account_label, props).unwrap();
    }
    assert_eq!(engine.get_property_table(account_label).unwrap().len(), 500);

    // 2. Insert 1,000 edges into MemTable
    let _ver_start = engine.next_commit_version();
    for i in 1..=500u64 {
        let src = VertexId(i);
        let dst1 = VertexId(if i < 500 { i + 1 } else { 1 });
        let dst2 = VertexId(if i > 1 { i - 1 } else { 500 });
        let v_cur = engine.next_commit_version();
        engine.insert_edge(EdgeId::simple(src, transfer_edge, dst1), v_cur);
        engine.insert_edge(EdgeId::simple(src, transfer_edge, dst2), v_cur);
    }
    let ver_end = engine.latest_version();

    assert_eq!(engine.delta_edges(), 1000);
    assert_eq!(engine.csr_edges(), 0);
    assert_eq!(engine.total_edges(), 1000);

    // 3. Compact into Chunked CSR
    engine.compact();

    assert_eq!(engine.delta_edges(), 0);
    assert_eq!(engine.csr_edges(), 1000);
    assert_eq!(engine.total_edges(), 1000);

    // Verify out edges after compaction
    let out_edges_1 = engine.get_out_edges(VertexId(1), Some(transfer_edge), ver_end);
    assert_eq!(out_edges_1.len(), 2);

    // 4. Second wave: insert 500 more edges, delete 100 edges
    for i in 1..=250u64 {
        let src = VertexId(i);
        let dst = VertexId(i + 250);
        let v_cur = engine.next_commit_version();
        engine.insert_edge(EdgeId::simple(src, transfer_edge, dst), v_cur);
    }
    for i in 1..=100u64 {
        let src = VertexId(i);
        let dst = VertexId(i + 1);
        let v_del = engine.next_commit_version();
        engine.delete_edge(EdgeId::simple(src, transfer_edge, dst), v_del);
    }
    let ver_latest = engine.latest_version();

    // Verify pre-compaction snapshot visibility
    let out_edges_1_del = engine.get_out_edges(VertexId(1), Some(transfer_edge), ver_latest);
    assert_eq!(out_edges_1_del.len(), 2); // 1->500 remains, 1->260 added, 1->2 deleted = 2 edges

    // 5. Compact again
    engine.compact();
    assert_eq!(engine.delta_edges(), 0);
    assert_eq!(engine.total_edges(), 1150); // 1000 + 250 - 100 = 1150
}

#[test]
fn test_cascade_vertex_deletion() {
    let mut schema = GraphSchema::new("cascade_test");
    let node_label = schema.register_vertex_label("Node", vec![]).unwrap();
    let edge_type = schema.register_edge_type("EDGE", vec![]).unwrap();

    let engine = PartitionStorageEngine::new(0, Arc::new(RwLock::new(schema)));

    // Create 3 vertices
    for i in 1..=3 {
        engine.set_vertex_properties(VertexId(i), node_label, HashMap::new()).unwrap();
    }

    // Edges: 1->2, 1->3, 2->3
    let v = engine.next_commit_version();
    engine.insert_edge(EdgeId::simple(VertexId(1), edge_type, VertexId(2)), v);
    engine.insert_edge(EdgeId::simple(VertexId(1), edge_type, VertexId(3)), v);
    engine.insert_edge(EdgeId::simple(VertexId(2), edge_type, VertexId(3)), v);
    engine.compact();

    assert_eq!(engine.total_vertices(), 3);
    assert_eq!(engine.total_edges(), 3);

    // Delete vertex 1 with cascade = true
    engine.delete_vertex(VertexId(1), node_label, true).unwrap();
    engine.compact();

    assert_eq!(engine.total_vertices(), 2);
    let v_now = engine.latest_version();
    assert_eq!(engine.get_out_edges(VertexId(1), Some(edge_type), v_now).len(), 0);
    assert_eq!(engine.get_out_edges(VertexId(2), Some(edge_type), v_now).len(), 1);
}

#[test]
fn test_secondary_index_multiple_values_and_updates() {
    let mut schema = GraphSchema::new("index_test");
    let item_label = schema
        .register_vertex_label(
            "Item",
            vec![
                PropertySpec::new("category", DataType::String, false),
                PropertySpec::new("price", DataType::Int64, false),
            ],
        )
        .unwrap();

    let engine = PartitionStorageEngine::new(0, Arc::new(RwLock::new(schema)));
    engine.create_vertex_index(item_label, "category").unwrap();
    engine.create_vertex_index(item_label, "price").unwrap();

    // Insert 10 items in category 'Books' and 10 items in category 'Electronics'
    for i in 1..=20u64 {
        let cat = if i <= 10 { "Books" } else { "Electronics" };
        let mut props = HashMap::new();
        props.insert("category".to_string(), DataValue::String(cat.to_string()));
        props.insert("price".to_string(), DataValue::Int64((i * 10) as i64));
        engine.set_vertex_properties(VertexId(i), item_label, props).unwrap();
    }

    // Lookup Books
    let books = engine.lookup_vertex_by_index(item_label, "category", &DataValue::String("Books".to_string())).unwrap();
    assert_eq!(books.len(), 10);

    // Lookup Price 100
    let item_10 = engine.lookup_vertex_by_index(item_label, "price", &DataValue::Int64(100)).unwrap();
    assert_eq!(item_10, vec![VertexId(10)]);

    // Update Category of Item 10 to 'Collectibles'
    engine.update_vertex_property(VertexId(10), item_label, "category", DataValue::String("Collectibles".to_string())).unwrap();

    let books_after = engine.lookup_vertex_by_index(item_label, "category", &DataValue::String("Books".to_string())).unwrap();
    assert_eq!(books_after.len(), 9);

    let collectibles = engine.lookup_vertex_by_index(item_label, "category", &DataValue::String("Collectibles".to_string())).unwrap();
    assert_eq!(collectibles, vec![VertexId(10)]);
}
