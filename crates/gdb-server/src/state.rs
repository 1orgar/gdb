use clap::Parser;
use gdb_core::schema::GraphSchema;
use gdb_core::{EdgeId, EdgeType, VertexId};
use gdb_planner::QueryExecutor;
use gdb_s3::S3StorageManager;
use gdb_storage::PartitionStorageEngine;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Instant;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReplicationMode {
    Sync,
    Async,
}

impl std::fmt::Display for ReplicationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplicationMode::Sync => write!(f, "SYNC"),
            ReplicationMode::Async => write!(f, "ASYNC"),
        }
    }
}

impl FromStr for ReplicationMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sync" | "synchronous" => Ok(ReplicationMode::Sync),
            "async" | "asynchronous" => Ok(ReplicationMode::Async),
            other => Err(format!("Unknown replication mode: '{}'. Use 'sync' or 'async'.", other)),
        }
    }
}

pub fn default_address() -> String {
    "127.0.0.1".to_string()
}

pub fn detect_local_address() -> String {
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(local_addr) = socket.local_addr() {
                let ip = local_addr.ip();
                if !ip.is_loopback() && !ip.is_unspecified() {
                    return ip.to_string();
                }
            }
        }
    }
    if let Ok(hostname) = std::env::var("HOSTNAME") {
        let h = hostname.trim();
        if !h.is_empty() {
            return h.to_string();
        }
    }
    "127.0.0.1".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RingNode {
    pub node_id: u64,
    pub http_url: String,
    #[serde(default = "default_address")]
    pub flight_address: String,
    pub flight_port: u16,
    #[serde(default = "default_address")]
    pub client_flight_address: String,
    pub client_flight_port: u16,
}

#[derive(Parser, Debug, Clone)]
#[command(author, version, about = "GDB: Distributed High-Performance In-Memory Graph Database", long_about = None)]
pub struct Args {
    /// Unique Node ID in the cluster
    #[arg(short, long, default_value_t = 1)]
    pub node_id: u64,

    /// Total partitions in cluster
    #[arg(long, default_value_t = 4)]
    pub partitions: u32,

    /// IP or hostname for internal inter-node Arrow Flight & RPC connections (auto-detected if omitted)
    #[arg(long, env = "GDB_FLIGHT_ADDRESS")]
    pub flight_address: Option<String>,

    /// Port for internal inter-node Arrow Flight & RPC
    #[arg(short, long, default_value_t = 8848)]
    pub port: u16,

    /// External IP or hostname for client Arrow Flight ingestion & queries (auto-detected if omitted)
    #[arg(long, env = "GDB_CLIENT_FLIGHT_ADDRESS")]
    pub client_flight_address: Option<String>,

    /// Dedicated Arrow Flight port for external client data ingestion & bulk loading
    #[arg(long, default_value_t = 8860)]
    pub client_flight_port: u16,

    /// Port for HTTP REST API
    #[arg(long, default_value_t = 8847)]
    pub http_port: u16,

    /// Directory for local NVMe Raft WAL
    #[arg(long, default_value = "./data/wal")]
    pub wal_dir: PathBuf,

    /// Comma-separated list of peer HTTP endpoints for Ring replication (e.g. "http://127.0.0.1:8846,http://127.0.0.1:8845")
    #[arg(long)]
    pub peers: Option<String>,

    /// S3 Bucket name for tiered persistence snapshots (e.g. "gdb-snapshots")
    #[arg(long, env = "AWS_BUCKET")]
    pub s3_bucket: Option<String>,

    /// S3 Custom Endpoint URL (e.g. "http://localhost:9000" for MinIO)
    #[arg(long, env = "AWS_ENDPOINT")]
    pub s3_endpoint: Option<String>,

    /// S3 Region (e.g. "us-east-1")
    #[arg(long, env = "AWS_REGION", default_value = "us-east-1")]
    pub s3_region: String,

    /// Replication factor on the leaderless hash ring (1 = pure sharding, N = full replication)
    #[arg(long, short = 'r', default_value_t = 3)]
    pub replication_factor: u32,

    /// Replication mode: 'sync' (synchronous quorum/all) or 'async' (background asynchronous)
    #[arg(long, default_value = "sync")]
    pub replication_mode: String,

    /// Cluster architecture mode (compatibility alias: 'ring', 'replication', 'sharding')
    #[arg(long, default_value = "ring")]
    pub cluster_mode: String,

    /// Enable hardware GPU acceleration (Metal on macOS, CUDA on Linux). Disabled by default.
    #[arg(long, env = "GDB_ENABLE_GPU", default_value_t = false)]
    pub enable_gpu: bool,

    /// Select GPU device index when multiple GPUs are available (default: 0)
    #[arg(long, env = "GDB_GPU_DEVICE", default_value_t = 0)]
    pub gpu_device: u32,

    /// Minimum number of edges required to trigger GPU hardware offload (default: 10000)
    #[arg(long, env = "GDB_GPU_THRESHOLD", default_value_t = 10_000)]
    pub gpu_offload_threshold: usize,

    /// Maximum VRAM allocation in Megabytes for discrete GPUs before chunked streaming paging (default: 2048 MB)
    #[arg(long, env = "GDB_GPU_MAX_VRAM_MB", default_value_t = 2048)]
    pub gpu_max_vram_mb: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QueryRequest {
    pub query: String,
    #[serde(default)]
    pub local_only: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BatchRequest {
    pub queries: Option<Vec<String>>,
    pub script: Option<String>,
    #[serde(default)]
    pub local_only: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReplicationPayload {
    pub query: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct ResourcesParams {
    pub local: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StorageOutEdgesRequest {
    pub vids: Vec<u64>,
    pub edge_type: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EdgeDto {
    pub src: u64,
    pub edge_type: u32,
    pub rank: i64,
    pub dst: u64,
}

impl From<EdgeId> for EdgeDto {
    fn from(e: EdgeId) -> Self {
        Self {
            src: e.src.0,
            edge_type: e.edge_type.0,
            rank: e.rank,
            dst: e.dst.0,
        }
    }
}

impl From<EdgeDto> for EdgeId {
    fn from(dto: EdgeDto) -> Self {
        EdgeId::new(VertexId(dto.src), EdgeType(dto.edge_type), dto.rank, VertexId(dto.dst))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StorageVertexPropertiesRequest {
    pub vids: Vec<u64>,
    pub label_id: Option<u32>,
    pub properties: Option<Vec<String>>,
}

#[derive(Clone)]
pub struct AppState {
    pub node_id: u64,
    pub partitions: u32,
    pub flight_address: String,
    pub port: u16,
    pub client_flight_address: String,
    pub client_flight_port: u16,
    pub http_port: u16,
    pub start_time: Instant,
    pub schema: Arc<RwLock<GraphSchema>>,
    pub executor: Arc<QueryExecutor>,
    pub storage: Arc<PartitionStorageEngine>,
    pub peers: Vec<String>,
    pub ring_nodes: Arc<RwLock<Vec<RingNode>>>,
    pub replication_factor: u32,
    pub replication_mode: ReplicationMode,
    pub http_client: reqwest::Client,
    pub queries_ok: Arc<AtomicU64>,
    pub queries_err: Arc<AtomicU64>,
    pub total_query_duration_us: Arc<AtomicU64>,
    pub replications_count: Arc<AtomicU64>,
    pub compactions_count: Arc<AtomicU64>,
    pub gpu_enabled: bool,
    pub gpu_device: u32,
    pub gpu_backend: String,
    pub gpu_threshold: usize,
    pub gpu_max_vram_mb: usize,
    pub s3_manager: Option<Arc<S3StorageManager>>,
    pub s3_bucket: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replication_mode_conversions() {
        assert_eq!(ReplicationMode::Sync.to_string(), "SYNC");
        assert_eq!(ReplicationMode::Async.to_string(), "ASYNC");

        assert_eq!(ReplicationMode::from_str("sync").unwrap(), ReplicationMode::Sync);
        assert_eq!(ReplicationMode::from_str("synchronous").unwrap(), ReplicationMode::Sync);
        assert_eq!(ReplicationMode::from_str("async").unwrap(), ReplicationMode::Async);
        assert_eq!(ReplicationMode::from_str("asynchronous").unwrap(), ReplicationMode::Async);
        assert!(ReplicationMode::from_str("invalid_mode").is_err());
    }

    #[test]
    fn test_edge_dto_roundtrip() {
        let edge = EdgeId::new(VertexId(10), EdgeType(2), 100, VertexId(20));
        let dto: EdgeDto = edge.into();
        assert_eq!(dto.src, 10);
        assert_eq!(dto.edge_type, 2);
        assert_eq!(dto.rank, 100);
        assert_eq!(dto.dst, 20);

        let roundtrip: EdgeId = dto.into();
        assert_eq!(roundtrip, edge);
    }

    #[test]
    fn test_address_helpers_and_args() {
        assert_eq!(default_address(), "127.0.0.1");
        let local_addr = detect_local_address();
        assert!(!local_addr.is_empty());

        let args = Args::parse_from(&[
            "gdb-server",
            "--node-id", "2",
            "--partitions", "8",
            "--port", "8855",
            "--client-flight-port", "8870",
            "--http-port", "8899",
            "--replication-factor", "2",
            "--replication-mode", "async",
        ]);
        assert_eq!(args.node_id, 2);
        assert_eq!(args.partitions, 8);
        assert_eq!(args.port, 8855);
        assert_eq!(args.client_flight_port, 8870);
        assert_eq!(args.http_port, 8899);
        assert_eq!(args.replication_factor, 2);
        assert_eq!(args.replication_mode, "async");
    }
}
