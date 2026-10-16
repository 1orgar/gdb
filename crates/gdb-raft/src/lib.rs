pub mod multi_raft;
pub mod types;

pub use multi_raft::{MultiRaftManager, RaftGroup};
pub use types::{NodeId, RaftMutation, RaftResponse};

#[cfg(test)]
mod tests {
    use super::*;
    use gdb_core::schema::GraphSchema;
    use gdb_core::{EdgeId, EdgeType, VertexId};
    use gdb_storage::PartitionStorageEngine;
    use parking_lot::RwLock;
    use std::sync::Arc;

    #[test]
    fn test_multi_raft_routing_and_commit() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let manager = MultiRaftManager::new(1, 4, tmp_dir.path().to_path_buf());

        let mut graph_schema = GraphSchema::new("test");
        graph_schema.register_vertex_label(
            "User",
            vec![gdb_core::PropertySpec::new("name", gdb_core::DataType::String, false)],
        ).unwrap();
        let schema = Arc::new(RwLock::new(graph_schema));
        let engine_p0 = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
        let engine_p1 = Arc::new(PartitionStorageEngine::new(1, schema.clone()));

        manager.register_partition(0, engine_p0.clone(), vec![2, 3]).unwrap();
        manager.register_partition(1, engine_p1.clone(), vec![2, 3]).unwrap();

        // Vertex 0 hashes to partition 0 (0 % 4 = 0)
        let v0 = VertexId(0);
        let resp0 = manager.route_mutation(
            v0,
            RaftMutation::InsertEdge {
                edge: EdgeId::simple(v0, EdgeType(1), VertexId(10)),
            },
        ).unwrap();
        assert!(resp0.success);

        // Vertex 1 hashes to partition 1 (1 % 4 = 1)
        let v1 = VertexId(1);
        let resp1 = manager.route_mutation(
            v1,
            RaftMutation::InsertEdge {
                edge: EdgeId::simple(v1, EdgeType(1), VertexId(20)),
            },
        ).unwrap();
        assert!(resp1.success);

        // Verify storage received edges
        assert_eq!(engine_p0.get_out_edges(v0, Some(EdgeType(1)), 100).len(), 1);
        assert_eq!(engine_p1.get_out_edges(v1, Some(EdgeType(1)), 100).len(), 1);

        // DeleteEdge mutation
        let del_resp = manager.route_mutation(
            v0,
            RaftMutation::DeleteEdge {
                edge: EdgeId::simple(v0, EdgeType(1), VertexId(10)),
            },
        ).unwrap();
        assert!(del_resp.success);

        // InsertVertex mutation
        let mut props = std::collections::HashMap::new();
        props.insert("name".to_string(), gdb_core::DataValue::String("Alice".into()));
        let v_resp = manager.route_mutation(
            v0,
            RaftMutation::InsertVertex {
                label_id: gdb_core::LabelId(1),
                id: v0,
                properties: props,
            },
        ).unwrap();
        assert!(v_resp.success);

        // Routing to unhosted partition (Vertex 2 -> partition 2 not registered)
        let v2 = VertexId(2);
        let err_resp = manager.route_mutation(
            v2,
            RaftMutation::InsertEdge {
                edge: EdgeId::simple(v2, EdgeType(1), VertexId(30)),
            },
        );
        assert!(err_resp.is_err());
        assert!(manager.get_group(999).is_none());
    }
}
