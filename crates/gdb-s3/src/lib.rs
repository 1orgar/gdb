pub mod manager;
pub mod parquet_codec;

pub use manager::S3StorageManager;
pub use parquet_codec::{csr_to_parquet, parquet_to_csr};

#[cfg(test)]
mod tests {
    use super::*;
    use gdb_core::schema::GraphSchema;
    use gdb_core::{EdgeId, EdgeType, VertexId};
    use gdb_storage::PartitionStorageEngine;
    use object_store::memory::InMemory;
    use parking_lot::RwLock;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_s3_snapshot_upload_and_restore() {
        let schema = Arc::new(RwLock::new(GraphSchema::new("test")));
        let engine1 = PartitionStorageEngine::new(0, schema.clone());

        // Insert edges into engine1 and compact
        let ver = engine1.next_commit_version();
        engine1.insert_edge(EdgeId::simple(VertexId(1), EdgeType(1), VertexId(2)), ver);
        engine1.insert_edge(EdgeId::simple(VertexId(2), EdgeType(1), VertexId(3)), ver);
        engine1.compact();

        // Upload to S3 (in-memory store for test)
        let s3 = Arc::new(InMemory::new());
        let s3_mgr = S3StorageManager::new(s3.clone());

        let snapshot_key = s3_mgr.upload_csr_snapshot(0, ver, &engine1).await.unwrap();
        assert!(snapshot_key.contains("partitions/p0/snapshot_v"));

        // Create new empty engine2 and restore from S3
        let engine2 = PartitionStorageEngine::new(0, schema.clone());
        s3_mgr.download_and_restore_csr(&snapshot_key, &engine2).await.unwrap();

        // Verify engine2 has all edges restored
        let edges = engine2.get_out_edges(VertexId(1), Some(EdgeType(1)), 100);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].dst, VertexId(2));

        let edges_v2 = engine2.get_out_edges(VertexId(2), Some(EdgeType(1)), 100);
        assert_eq!(edges_v2.len(), 1);
        assert_eq!(edges_v2[0].dst, VertexId(3));
    }
}
