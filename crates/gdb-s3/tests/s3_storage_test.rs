use gdb_core::schema::GraphSchema;
use gdb_core::{EdgeId, EdgeType, VertexId};
use gdb_s3::S3StorageManager;
use gdb_storage::PartitionStorageEngine;
use object_store::memory::InMemory;
use parking_lot::RwLock;
use std::sync::Arc;

#[tokio::test]
async fn test_s3_snapshot_upload_and_restore() {
    let schema = Arc::new(RwLock::new(GraphSchema::new("s3_test")));
    let engine1 = PartitionStorageEngine::new(0, schema.clone());

    // Insert edges into engine1
    engine1.insert_edge(EdgeId::simple(VertexId(1), EdgeType(1), VertexId(2)), 1);
    engine1.insert_edge(EdgeId::simple(VertexId(2), EdgeType(1), VertexId(3)), 1);
    engine1.compact();

    assert_eq!(engine1.total_edges(), 2);

    let store = Arc::new(InMemory::new());
    let s3_manager = S3StorageManager::new(store);

    // 1. Upload snapshot
    let key = s3_manager.upload_csr_snapshot(0, 100, &engine1).await.unwrap();
    assert!(key.contains("p0/snapshot_v100.parquet"));

    // 2. Restore snapshot into fresh engine2
    let engine2 = PartitionStorageEngine::new(0, schema.clone());
    assert_eq!(engine2.total_edges(), 0);

    s3_manager.download_and_restore_csr(&key, &engine2).await.unwrap();
    assert_eq!(engine2.total_edges(), 2);

    // 3. Test non-existent key returns error
    assert!(s3_manager.download_and_restore_csr("non_existent_key.parquet", &engine2).await.is_err());
}

#[test]
fn test_s3_from_config_constructor() {
    // Basic configuration creation
    let s3_res = S3StorageManager::from_config(
        "test-bucket",
        Some("http://localhost:9000"),
        Some("us-east-1"),
        Some("minioadmin"),
        Some("minioadmin"),
    );
    assert!(s3_res.is_ok());
}
