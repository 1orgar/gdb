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
    #[serde(default)]
    pub graph: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BatchRequest {
    pub queries: Option<Vec<String>>,
    pub script: Option<String>,
    #[serde(default)]
    pub local_only: Option<bool>,
    #[serde(default)]
    pub graph: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReplicationPayload {
    pub query: String,
    #[serde(default)]
    pub graph: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ResourcesParams {
    pub local: Option<bool>,
    pub graph: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StorageOutEdgesRequest {
    pub vids: Vec<u64>,
    pub edge_type: Option<u32>,
    #[serde(default)]
    pub graph: Option<String>,
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
    #[serde(default)]
    pub graph: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SuperstepRecord {
    pub superstep: usize,
    pub active_vertices: usize,
    pub messages_exchanged: usize,
    pub bytes_shuffled: usize,
    pub max_delta: f64,
    pub elapsed_ms: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PregelTelemetry {
    pub algorithm: String,
    pub graph_space: String,
    pub status: String,
    pub current_superstep: usize,
    pub max_supersteps: usize,
    pub active_vertices: usize,
    pub total_vertices: usize,
    pub total_edges: usize,
    pub messages_exchanged: usize,
    pub bytes_shuffled: usize,
    pub convergence_delta: f64,
    pub tolerance: f64,
    pub replication_factor: u32,
    pub shards_participating: usize,
    pub elapsed_ms: f64,
    pub engine_mode: String,
    pub supersteps: Vec<SuperstepRecord>,
}

impl Default for PregelTelemetry {
    fn default() -> Self {
        Self {
            algorithm: "algo.pageRank".to_string(),
            graph_space: "default".to_string(),
            status: "IDLE".to_string(),
            current_superstep: 0,
            max_supersteps: 20,
            active_vertices: 0,
            total_vertices: 0,
            total_edges: 0,
            messages_exchanged: 0,
            bytes_shuffled: 0,
            convergence_delta: 0.0,
            tolerance: 1e-6,
            replication_factor: 1,
            shards_participating: 1,
            elapsed_ms: 0.0,
            engine_mode: "Distributed Pregel / BSP + Metal GPU".to_string(),
            supersteps: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct GraphSpace {
    pub name: String,
    pub schema: Arc<RwLock<GraphSchema>>,
    pub storage: Arc<PartitionStorageEngine>,
    pub executor: Arc<QueryExecutor>,
    pub global_csr_cache: Arc<RwLock<Option<(u64, Arc<gdb_storage::ChunkedCsr>)>>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GraphSpaceSummary {
    pub name: String,
    pub active: bool,
    pub vertex_labels: usize,
    pub edge_types: usize,
    pub total_vertices: usize,
    pub total_edges: usize,
    pub csr_edges: usize,
    pub delta_edges: usize,
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
    pub global_csr_cache: Arc<RwLock<Option<(u64, Arc<gdb_storage::ChunkedCsr>)>>>,
    pub spaces: Arc<RwLock<std::collections::HashMap<String, Arc<GraphSpace>>>>,
    pub active_space: Arc<RwLock<String>>,
    pub pregel_status: Arc<RwLock<PregelTelemetry>>,
}

impl AppState {
    pub fn normalize_graph_name(raw: &str) -> String {
        let trimmed = raw
            .trim()
            .trim_start_matches("gdb://")
            .trim_matches('/');
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("default_graph") {
            "default".to_string()
        } else {
            trimmed.to_string()
        }
    }

    pub fn current_graph_name(&self) -> String {
        self.active_space.read().clone()
    }

    pub fn resolve_space(&self, requested: Option<&str>) -> Arc<GraphSpace> {
        let target_name = match requested {
            Some(s) if !s.trim().is_empty() => Self::normalize_graph_name(s),
            _ => self.active_space.read().clone(),
        };

        if let Some(space) = self.spaces.read().get(&target_name) {
            return space.clone();
        }
        if let Some(default_space) = self.spaces.read().get("default") {
            return default_space.clone();
        }

        Arc::new(GraphSpace {
            name: "default".to_string(),
            schema: self.schema.clone(),
            storage: self.storage.clone(),
            executor: self.executor.clone(),
            global_csr_cache: self.global_csr_cache.clone(),
        })
    }

    pub fn with_graph_space(&self, requested: Option<&str>) -> AppState {
        let space = self.resolve_space(requested);
        let mut cloned = self.clone();
        cloned.schema = space.schema.clone();
        cloned.storage = space.storage.clone();
        cloned.executor = space.executor.clone();
        cloned.global_csr_cache = space.global_csr_cache.clone();
        cloned
    }

    pub fn create_graph_space(&self, raw_name: &str, if_not_exists: bool) -> Result<Arc<GraphSpace>, String> {
        let name = Self::normalize_graph_name(raw_name);
        {
            let guard = self.spaces.read();
            if let Some(existing) = guard.get(&name) {
                if if_not_exists {
                    return Ok(existing.clone());
                }
                return Err(format!("Graph space '{}' already exists", name));
            }
        }

        let schema = Arc::new(RwLock::new(GraphSchema::new(&name)));
        let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
        let mut exec = QueryExecutor::new(schema.clone(), storage.clone());
        exec.gpu = self.executor.gpu.clone();
        exec.remote_expander = self.executor.remote_expander.clone();
        exec.remote_property_resolver = self.executor.remote_property_resolver.clone();

        let space = Arc::new(GraphSpace {
            name: name.clone(),
            schema,
            storage,
            executor: Arc::new(exec),
            global_csr_cache: Arc::new(RwLock::new(None)),
        });

        self.spaces.write().insert(name, space.clone());
        Ok(space)
    }

    pub fn drop_graph_space(&self, raw_name: &str, if_exists: bool) -> Result<bool, String> {
        let name = Self::normalize_graph_name(raw_name);
        if name == "default" {
            return Err("Cannot drop the default system graph space".to_string());
        }
        let removed = self.spaces.write().remove(&name).is_some();
        if !removed && !if_exists {
            return Err(format!("Graph space '{}' does not exist", name));
        }
        if *self.active_space.read() == name {
            *self.active_space.write() = "default".to_string();
        }
        Ok(removed)
    }

    pub fn use_graph_space(&self, raw_name: &str) -> Result<String, String> {
        let name = Self::normalize_graph_name(raw_name);
        if !self.spaces.read().contains_key(&name) {
            return Err(format!(
                "Graph space '{}' does not exist. Create it first with CREATE GRAPH {};",
                name, name
            ));
        }
        *self.active_space.write() = name.clone();
        Ok(name)
    }

    pub fn list_graph_spaces(&self) -> Vec<GraphSpaceSummary> {
        let active = self.active_space.read().clone();
        let guard = self.spaces.read();
        let mut list: Vec<GraphSpaceSummary> = guard
            .values()
            .map(|sp| {
                let sc = sp.schema.read();
                GraphSpaceSummary {
                    name: sp.name.clone(),
                    active: sp.name == active,
                    vertex_labels: sc.list_vertex_schemas().len(),
                    edge_types: sc.list_edge_schemas().len(),
                    total_vertices: sp.storage.total_vertices(),
                    total_edges: sp.storage.total_edges(),
                    csr_edges: sp.storage.csr_edges(),
                    delta_edges: sp.storage.delta_edges(),
                }
            })
            .collect();
        list.sort_by(|a, b| {
            if a.name == "default" {
                std::cmp::Ordering::Less
            } else if b.name == "default" {
                std::cmp::Ordering::Greater
            } else {
                a.name.cmp(&b.name)
            }
        });
        list
    }
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
