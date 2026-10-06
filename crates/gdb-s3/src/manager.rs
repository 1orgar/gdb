use crate::parquet_codec::{csr_to_parquet, parquet_to_csr};
use bytes::Bytes;
use gdb_core::{GdbError, GdbResult};
use gdb_storage::PartitionStorageEngine;
use object_store::path::Path as ObjPath;
use object_store::ObjectStore;
use std::sync::Arc;

use object_store::aws::AmazonS3Builder;

pub struct S3StorageManager {
    store: Arc<dyn ObjectStore>,
}

impl S3StorageManager {
    pub fn new(store: Arc<dyn ObjectStore>) -> Self {
        Self { store }
    }

    pub fn from_config(
        bucket: &str,
        endpoint: Option<&str>,
        region: Option<&str>,
        access_key: Option<&str>,
        secret_key: Option<&str>,
    ) -> GdbResult<Self> {
        let mut builder = AmazonS3Builder::new().with_bucket_name(bucket);
        if let Some(ep) = endpoint {
            builder = builder.with_endpoint(ep).with_allow_http(true);
        }
        if let Some(reg) = region {
            builder = builder.with_region(reg);
        }
        if let (Some(ak), Some(sk)) = (access_key, secret_key) {
            builder = builder.with_access_key_id(ak).with_secret_access_key(sk);
        }
        let store = builder.build().map_err(|e| GdbError::S3(e.to_string()))?;
        Ok(Self { store: Arc::new(store) })
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
