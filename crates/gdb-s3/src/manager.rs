use crate::parquet_codec::{csr_to_parquet, parquet_to_csr};
use bytes::Bytes;
use gdb_core::{GdbError, GdbResult};
use gdb_storage::PartitionStorageEngine;
use object_store::path::Path as ObjPath;
use object_store::ObjectStore;
use std::sync::Arc;

pub struct S3StorageManager {
    store: Arc<dyn ObjectStore>,
}

impl S3StorageManager {
    pub fn new(store: Arc<dyn ObjectStore>) -> Self {
        Self { store }
    }

    /// Uploads an immutable snapshot of the partition's CSR topology to S3 in Parquet format.
    pub async fn upload_csr_snapshot(
        &self,
        partition_id: u32,
        version: u64,
        engine: &PartitionStorageEngine,
    ) -> GdbResult<String> {
        let csr = engine.current_csr();
        let parquet_bytes = csr_to_parquet(&csr)?;

        let key = format!("partitions/p{}/snapshot_v{}.parquet", partition_id, version);
        let path = ObjPath::from(key.as_str());

        self.store
            .put(&path, Bytes::from(parquet_bytes).into())
            .await
            .map_err(|e| GdbError::S3(e.to_string()))?;

        Ok(key)
    }

    /// Downloads and restores an immutable CSR snapshot from S3.
    pub async fn download_and_restore_csr(
        &self,
        key: &str,
        engine: &PartitionStorageEngine,
    ) -> GdbResult<()> {
        let path = ObjPath::from(key);
        let get_result = self.store.get(&path).await.map_err(|e| GdbError::S3(e.to_string()))?;
        let bytes = get_result.bytes().await.map_err(|e| GdbError::S3(e.to_string()))?;

        let csr = parquet_to_csr(&bytes)?;
        // Insert reconstructed edges into engine
        for &src_raw in &csr.reverse_map {
            let src = gdb_core::VertexId(src_raw);
            let edges = csr.get_out_edges(src, None);
            for e in edges {
                engine.insert_edge(e, 1);
            }
        }
        engine.compact();

        Ok(())
    }
}
