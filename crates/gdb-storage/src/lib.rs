pub mod csr;
pub mod delta;
pub mod engine;
pub mod properties;

pub use csr::ChunkedCsr;
pub use delta::{DeltaMemTable, MvccEdge};
pub use engine::{GraphStatistics, PartitionStorageEngine};
pub use properties::VertexPropertyTable;

#[cfg(test)]
mod tests {
    use super::*;
    use gdb_core::schema::{DataType, GraphSchema, PropertySpec};
    use gdb_core::{DataValue, EdgeId, EdgeType, VertexId};
    use parking_lot::RwLock;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn test_dual_store_traversal_and_compaction() {
        let mut schema = GraphSchema::new("test_social");
        let user_label = schema
            .register_vertex_label(
                "User",
                vec![
                    PropertySpec::new("name", DataType::String, false),
                    PropertySpec::new("age", DataType::Int64, false),
                ],
            )
            .unwrap();
        let _knows_edge = schema
            .register_edge_type("KNOWS", vec![])
            .unwrap();

        let engine = PartitionStorageEngine::new(0, Arc::new(RwLock::new(schema)));

        let v1 = VertexId(101);
        let v2 = VertexId(102);
        let v3 = VertexId(103);
        let knows = EdgeType(1);

        // 1. Insert properties
        let mut p1 = HashMap::new();
        p1.insert("name".to_string(), DataValue::String("Alice".to_string()));
        p1.insert("age".to_string(), DataValue::Int64(30));
        engine.set_vertex_properties(v1, user_label, p1).unwrap();

        assert_eq!(
            engine.get_vertex_property(v1, user_label, "name"),
            Some(DataValue::String("Alice".to_string()))
        );

        // 2. Insert edges into Delta
        let ver1 = engine.next_commit_version();
        engine.insert_edge(EdgeId::simple(v1, knows, v2), ver1);

        let ver2 = engine.next_commit_version();
        engine.insert_edge(EdgeId::simple(v1, knows, v3), ver2);

        // 3. Snapshot isolation check
        let out_at_v1 = engine.get_out_edges(v1, Some(knows), ver1);
        assert_eq!(out_at_v1.len(), 1);
        assert_eq!(out_at_v1[0].dst, v2);

        let out_at_v2 = engine.get_out_edges(v1, Some(knows), ver2);
        assert_eq!(out_at_v2.len(), 2);

        // 4. Compaction
        engine.compact();

        // After compaction, CSR contains both edges
        let out_after_compact = engine.get_out_edges(v1, Some(knows), ver2);
        assert_eq!(out_after_compact.len(), 2);

        // 5. Test Arrow batch rebuilding
        engine.rebuild_arrow_batches().unwrap();
        let prop_table = engine.get_property_table(user_label).unwrap();
        let batches = prop_table.get_batches();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].num_rows(), 1);
    }

    #[test]
    fn test_deletion_and_tombstones() {
        let schema = GraphSchema::new("test");
        let engine = PartitionStorageEngine::new(0, Arc::new(RwLock::new(schema)));

        let v1 = VertexId(1);
        let v2 = VertexId(2);
        let follows = EdgeType(2);

        let ver1 = engine.next_commit_version();
        engine.insert_edge(EdgeId::simple(v1, follows, v2), ver1);
        engine.compact(); // Now in CSR

        assert_eq!(engine.get_out_edges(v1, Some(follows), ver1).len(), 1);

        // Delete edge in delta
        let ver2 = engine.next_commit_version();
        engine.delete_edge(EdgeId::simple(v1, follows, v2), ver2);

        // Snapshot at ver1 sees edge, snapshot at ver2 does NOT
        assert_eq!(engine.get_out_edges(v1, Some(follows), ver1).len(), 1);
        assert_eq!(engine.get_out_edges(v1, Some(follows), ver2).len(), 0);

        // Compact deletes it permanently
        engine.compact();
        assert_eq!(engine.get_out_edges(v1, Some(follows), ver2).len(), 0);
    }

    #[test]
    fn test_secondary_indexing_and_vertex_mutations() {
        let mut schema = GraphSchema::new("test");
        let user_label = schema
            .register_vertex_label(
                "User",
                vec![
                    PropertySpec::new("name", DataType::String, false),
                    PropertySpec::new("email", DataType::String, false),
                ],
            )
            .unwrap();

        let engine = PartitionStorageEngine::new(0, Arc::new(RwLock::new(schema)));

        // Create secondary index on 'email'
        engine.create_vertex_index(user_label, "email").unwrap();
        assert!(engine.has_vertex_index(user_label, "email"));

        // Insert vertices
        let v1 = VertexId(1);
        let mut p1 = HashMap::new();
        p1.insert("name".to_string(), DataValue::String("Alice".to_string()));
        p1.insert("email".to_string(), DataValue::String("alice@test.com".to_string()));
        engine.set_vertex_properties(v1, user_label, p1).unwrap();

        let v2 = VertexId(2);
        let mut p2 = HashMap::new();
        p2.insert("name".to_string(), DataValue::String("Bob".to_string()));
        p2.insert("email".to_string(), DataValue::String("bob@test.com".to_string()));
        engine.set_vertex_properties(v2, user_label, p2).unwrap();

        // Index lookup
        let res = engine.lookup_vertex_by_index(user_label, "email", &DataValue::String("alice@test.com".to_string()));
        assert_eq!(res, Some(vec![v1]));

        // Update property
        engine.update_vertex_property(v1, user_label, "email", DataValue::String("alice_new@test.com".to_string())).unwrap();
        assert_eq!(
            engine.lookup_vertex_by_index(user_label, "email", &DataValue::String("alice@test.com".to_string())),
            None
        );
        assert_eq!(
            engine.lookup_vertex_by_index(user_label, "email", &DataValue::String("alice_new@test.com".to_string())),
            Some(vec![v1])
        );

        // Delete vertex
        engine.delete_vertex(v1, user_label, false).unwrap();
        assert_eq!(
            engine.lookup_vertex_by_index(user_label, "email", &DataValue::String("alice_new@test.com".to_string())),
            None
        );

        // Drop index
        assert!(engine.drop_vertex_index(user_label, "email").unwrap());
        assert!(!engine.has_vertex_index(user_label, "email"));
    }
}
