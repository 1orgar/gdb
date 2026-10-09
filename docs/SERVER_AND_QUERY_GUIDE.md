[English](SERVER_AND_QUERY_GUIDE.md) | [Русский](SERVER_AND_QUERY_GUIDE_RU.md)

# GDB: Server Configuration Guide & Query Reference

Comprehensive technical guide for deploying, configuring, and operating the distributed in-memory graph database **GDB** (a high-performance Nebula Graph / Nebula Enterprise alternative), release **v0.4.2**.

---

## Part 1. Server Launch & Configuration

Each GDB server node (`gdb-server`) combines:
- A two-tier in-memory engine (Chunked-CSR topology + Delta MemTable with MVCC).
- Concurrent secondary property indexes (`DashMap`) with automated maintenance on mutations.
- A Multi-Raft consensus engine with local append-only WAL journal.
- Hardware compute acceleration (Apple Metal UMA Zero-Copy / NVIDIA CUDA / CPU SIMD).
- Transport layer: Apache Arrow Flight (gRPC) + HTTP REST API (:8847).
- Cluster peer replication (`/raft/replicate`).
- Schema introspection endpoint (`/schema`).
- Prometheus metrics exporter (`/metrics`).

### 1.1. Complete `gdb-server` Command-Line Reference

```bash
gdb-server [OPTIONS]
```

#### Command-Line Flags & Environment Variables

| CLI Flag | Short | Environment Variable | Type | Default | Description |
| :--- | :---: | :--- | :---: | :---: | :--- |
| `--node-id` | `-n` | — | `u64` | `1` | Unique numerical node ID in cluster ring. Determines primary token range: $\text{Token} = (u \pmod N) + 1$. |
| `--partitions` | — | — | `u32` | `4` | Number of independent Multi-Raft groups and storage partitions per node. |
| `--port` | `-p` | — | `u16` | `8848` | **Internal Apache Arrow Flight gRPC** network port for inter-node MPP shuffle and RecordBatch exchange. |
| `--client-flight-port` | — | `GDB_CLIENT_FLIGHT_PORT` | `u16` | `8860` | **External Client Flight gRPC** port for streaming bulk ingestion (`do_put`) and direct Cypher streaming (`do_get`). |
| `--http-port` | — | — | `u16` | `8847` | **HTTP REST API** port for client queries (`POST /query`), schema (`GET /schema`), replication (`POST /replicate`), metrics (`GET /metrics`), cluster status (`GET /cluster`), and health checks (`GET /health`). |
| `--wal-dir` | — | — | `path` | `./data/wal` | Disk path for the append-only **Write-Ahead Log (WAL)** with CRC32 checksums. |
| `--peers` | — | — | `string` | *(empty)* | Comma-separated list of peer HTTP addresses (e.g. `"http://127.0.0.1:8846,http://127.0.0.1:8845"`). |
| `--replication-factor` | `-r` | `GDB_REPLICATION_FACTOR` | `u32` | `3` | **Replication Factor (RF):**<br>• `1` — Pure distributed sharding (MPP mode, $\sum\text{RAM}$).<br>• `k` — Partial replication across $k$ consecutive nodes.<br>• `N` — Full mirroring across all active peers. |
| `--replication-mode` | — | `GDB_REPLICATION_MODE` | `string` | `sync` | **Replication Mode:**<br>• `sync` — Synchronous: Coordinator awaits quorum write confirmation before returning.<br>• `async` — Asynchronous: Coordinator responds immediately, dispatching replication in background. |
| `--enable-gpu` | — | `GDB_ENABLE_GPU` | `bool` | `false` | **Enable GPU Acceleration:** Default is `false` (CPU SIMD). |
| `--gpu-device` | — | `GDB_GPU_DEVICE` | `usize` | `0` | **Target GPU Index:** Device ID in multi-GPU environments (Tesla V100, A100, H100, RTX). |
| `--gpu-offload-threshold` | — | `GDB_GPU_THRESHOLD` | `usize` | `10000` | **Offload Threshold:** Minimum number of graph edges before offloading computations to GPU. |
| `--s3-bucket` | — | `AWS_BUCKET` | `string` | *(empty)* | AWS S3 or MinIO bucket name for tiered storage snapshots. |
| `--s3-endpoint` | — | `AWS_ENDPOINT` | `string` | *(empty)* | Custom S3 endpoint URL (e.g. `http://localhost:9000` for MinIO). |
| `--s3-region` | — | `AWS_REGION` | `string` | `us-east-1` | S3 AWS region. |
| *(credential)* | — | `AWS_ACCESS_KEY_ID` | `string` | *(empty)* | Access key ID for S3/MinIO. |
| *(credential)* | — | `AWS_SECRET_ACCESS_KEY`| `string` | *(empty)* | Secret access key for S3/MinIO. |
| *(logging)* | — | `RUST_LOG` | `string` | `info` | Tracing logging level (`error`, `warn`, `info`, `debug`, `trace`). |
| `--cluster-mode` | — | — | `string` | `ring` | Compatibility alias (`ring`, `replication`, `sharding`). |

---

### 1.2. Cluster Startup Scripts (`start_cluster.sh` & `start_cluster_amd64.sh`)

Automated 3-node cluster scripts support dynamic binary discovery and the following arguments:

| Script Option | Aliases | Example | Description |
| :--- | :--- | :--- | :--- |
| `--nodes <N>` | `-n <N>` | `./scripts/start_cluster.sh --nodes 5` | Number of cluster nodes to spawn (default: 3). Supports any count $\ge 1$. |
| `--rf <N>` | `-r <N>`, `--replication-factor <N>` | `./scripts/start_cluster.sh --rf 1` | Sets replication factor across nodes (`1`, `2`, `3`). |
| `--sync` | `sync`, `--replication-mode sync` | `./scripts/start_cluster.sh --sync` | Enables synchronous quorum replication. |
| `--async` | `async`, `--replication-mode async` | `./scripts/start_cluster.sh --async` | Enables asynchronous replication for maximum write throughput. |
| `--sharding` | `--sharded`, `sharding` | `./scripts/start_cluster.sh --sharding` | Express shortcut for `--rf 1 --sync` (pure MPP). |
| `--replication` | `--replicated`, `replication` | `./scripts/start_cluster.sh --replication` | Express shortcut for `--rf 3 --sync` (full mirroring). |
| `--enable-gpu <bool>` | `--gpu <bool>` | `./scripts/start_cluster.sh --enable-gpu true` | Enables or disables hardware GPU acceleration. |
| `--gpu-device <ID>` | `--device <ID>` | `./scripts/start_cluster.sh --gpu-device 0` | Selects target GPU device ID. |
| `--gpu-offload-threshold <N>` | `--threshold <N>` | `./scripts/start_cluster.sh --gpu-offload-threshold 5000` | Sets edge count offload threshold. |

#### Standard 3-Node Cluster Port Assignment:

| Node | Role in Ring | HTTP REST API | Internal Flight | Client Flight Port | WAL Directory | Log File |
| :--- | :--- | :---: | :---: | :---: | :--- | :--- |
| **Peer 1** | Peer / Coordinator | `http://127.0.0.1:8847` | `:8848` | `:8860` | `./data/node1/wal` | `./logs/node1.log` |
| **Peer 2** | Peer / Storage | `http://127.0.0.1:8846` | `:8849` | `:8861` | `./data/node2/wal` | `./logs/node2.log` |
| **Peer 3** | Peer / Storage | `http://127.0.0.1:8845` | `:8850` | `:8862` | `./data/node3/wal` | `./logs/node3.log` |
| **Web Studio UI**| Management Console | `http://localhost:3000`| — | — | — | `./logs/studio.log` |

---

### 1.3. Architecture: Leaderless Hash Ring

GDB nodes form a symmetric peer ring (Dynamo / Cassandra style):
- **Zero Single Point of Failure (SPOF):** No rigid Leader/Follower topology. Any node accepts reads and writes, acting as query **Coordinator**.
- **Token Assignment:** Vertex $u$ or directed edge $u \to v$ assigns primary partition by:
  $$\text{Primary Node Index} = u \pmod N$$
  Replicated onto consecutive nodes:
  $$\text{Replicas}(u) = \{ (\text{Primary Node Index} + i) \pmod N \mid i \in [0, \text{RF} - 1] \}$$

#### Inspect Cluster State via CLI
```sql
SHOW CLUSTER;
SHOW RESOURCES;
```

---

### 1.4. Hardware Acceleration (Metal & CUDA)

GDB auto-detects host hardware capabilities:
- **Apple Silicon (M1/M2/M3/M4/M5 on macOS):** **Apple Metal Compute Backend** with **Unified Memory Architecture (UMA Zero-Copy)**.
- **Linux x86_64 / NVIDIA GPU:** **NVIDIA CUDA Backend** supporting Tesla V100, A100, H100, and RTX GPUs.
- **CPU SIMD Fallback:** Automatically active when `--enable-gpu false` or when no compatible GPU is present.

#### Supported Compute Kernels:
- `parallel_bfs_step` / `cuda_bfs_frontier_kernel` — Parallel BFS wave expansion.
- `parallel_pagerank_step` / `cuda_pagerank_spmv_kernel` — Vectorized PageRank SpMV edge mass push.
- `louvain_step` / `cuda_louvain_kernel` — Modularity community detection.
- `wcc_step` / `cuda_wcc_kernel` — Component label propagation.
- `triangle_count_step` / `cuda_triangle_count_kernel` — Adjacency list intersection.

---

## Part 2. Query Reference & Examples

Connect to the cluster via:
- Interactive CLI: `./bin/gdb-cli`
- Web Studio UI: `./scripts/start_studio.sh` (http://localhost:3000)
- HTTP REST API: `POST http://localhost:8847/query` with body `{"query": "..."}`
- Arrow Flight External Port: `:8860` via `gdb-py-client`

---

### 2.1. DDL: Graph Schema & Secondary Property Indexes

```sql
-- 1. Create Vertex Tags
CREATE VERTEX User (name STRING, age INT64);
CREATE VERTEX Device (model STRING, ram_gb INT64);
CREATE VERTEX Server (ip STRING, cores INT64);

-- 2. Create Edge Types
CREATE EDGE FOLLOWS ();
CREATE EDGE KNOWS ();
CREATE EDGE CONNECTS ();

-- 3. Create Secondary Property Indexes
-- Enables O(1) point lookups via physical IndexScan operator
CREATE INDEX ON :User(name);
CREATE INDEX ON :User(age);

-- 4. Alter Vertex Tags & Edge Types
ALTER VERTEX User ADD (email STRING, country STRING);
ALTER VERTEX User DROP (country);
ALTER EDGE FOLLOWS ADD (since INT64);
ALTER EDGE FOLLOWS DROP (since);

-- 5. Drop Schema Elements
DROP INDEX ON :User(age);
DROP VERTEX Device;
DROP EDGE CONNECTS;
```

---

### 2.2. DML: Ingestion, Mutation & Upsert (Cypher DML)

```sql
-- 1. Insert Vertices (single & bulk)
INSERT VERTEX User (id, name, age) VALUES (1, 'Alice', 30);

INSERT VERTEX User (id, name, age) VALUES 
  (2, 'Bob', 25), 
  (3, 'Charlie', 35), 
  (4, 'Dave', 28);

-- 2. Insert Edges
INSERT EDGE FOLLOWS FROM 1 TO 2;

INSERT EDGE FOLLOWS VALUES 
  (2, 3), 
  (3, 1), 
  (3, 4);

-- 3. Property Mutation (openCypher SET)
MATCH (u:User {name: 'Alice'}) SET u.age = 31, u.status = 'active';

-- 4. Delete Vertices & Relationships (openCypher DELETE / DETACH DELETE)
MATCH (u:User {name: 'Dave'}) DELETE u;
MATCH (u:User {name: 'Dave'}) DETACH DELETE u;

-- 5. Idempotent Ingestion / Upsert (openCypher MERGE)
MERGE (u:User {name: 'Eve', age: 26});

-- 6. Trigger Chunked-CSR Compaction
compact;
```

---

### 2.3. openCypher / GQL: Traversal, Multi-Hop, Aggregations & Pagination

```sql
-- 1-Hop Traversal with Property Projection:
MATCH (a:User)-[:FOLLOWS]->(b:User)
RETURN a.name, b.name;

-- Multi-Hop Traversal with Variable-Length Path (*min..max):
MATCH (a:User)-[:FOLLOWS*1..3]->(b:User)
WHERE a.id = 1
RETURN a.name, b.name;

-- Chained Transformations via WITH Operator:
MATCH (u:User)
WITH u.department AS dept, count(u) AS team_size, avg(u.salary) AS avg_sal
WHERE team_size >= 2
RETURN dept, team_size, avg_sal
ORDER BY team_size DESC;

-- Multi-hop Pipelining with WITH:
MATCH (a:User)-[:FOLLOWS]->(b:User)
WITH b.name AS followee, b.age AS followee_age
WHERE followee_age > 20
RETURN followee, followee_age;

-- Aggregations & Group By:
MATCH (a:User)-[:FOLLOWS]->(b:User)
RETURN a.name, COUNT(b) AS followers, AVG(b.age) AS avg_age
ORDER BY followers DESC;

-- Distinct Values & Pagination (SKIP, LIMIT):
MATCH (a:User)-[:FOLLOWS]->(b:User)
RETURN DISTINCT b.name
ORDER BY b.name ASC
SKIP 10
LIMIT 20;

-- Query Physical Execution Plan (EXPLAIN):
EXPLAIN MATCH (a:User)-[:FOLLOWS]->(b:User)
WHERE a.name = 'Alice'
RETURN b.name;
```

---

### 2.4. Enterprise Graph Analytics Suite

All algorithms execute concurrently on CPU SIMD or GPU:

#### 1. PageRank (Vertex Importance):
```sql
CALL algo.pageRank({damping: 0.85, max_iter: 20}) YIELD vertex_id, score;
```

#### 2. Louvain (Community Detection):
```sql
CALL algo.louvain({max_iter: 10}) YIELD vertex_id, community_id;
```

#### 3. WCC (Weakly Connected Components):
```sql
CALL algo.wcc() YIELD vertex_id, component_id;
```

#### 4. SCC (Strongly Connected Components):
```sql
CALL algo.scc() YIELD vertex_id, component_id;
```

#### 5. Triangle Count & Local Clustering Coefficient (LCC):
```sql
CALL algo.triangleCount() YIELD vertex_id, triangles;
```

#### 6. K-Core Decomposition (Dense Subgraphs):
```sql
CALL algo.kCore() YIELD vertex_id, coreness;
```

#### 7. Betweenness Centrality (Brandes Algorithm):
```sql
CALL algo.betweenness() YIELD vertex_id, betweenness;
```

#### 8. Closeness Centrality:
```sql
CALL algo.closeness() YIELD vertex_id, closeness;
```

#### 9. Degree Centrality:
```sql
CALL algo.degree() YIELD vertex_id, in_degree, out_degree;
```

#### 10. SSSP (Single-Source Shortest Path):
```sql
CALL algo.sssp(1) YIELD vertex_id, distance;
```

#### 11. Similarity (Jaccard and Cosine Neighbor Overlap):
```sql
CALL algo.similarity({node1: 1, node2: 2}) YIELD jaccard, cosine;
```

---

### 2.5. Interactive CLI Commands

```text
SHOW CLUSTER              - Display cluster topology, roles, ports, and ping latency.
SHOW RESOURCES            - Display allocated memory, vertices, and edge counts (CSR vs MemTable).
SHOW GPU                  - Display hardware acceleration status, UMA memory, and offload thresholds.
:connect <http://url>     - Switch coordinator endpoint dynamically.
compact                   - Trigger CSR compaction on the connected node.
exit / quit               - Exit console.
```

---

## Part 3. External Arrow Flight Client (`gdb-py-client`)

Dedicated Flight port (`--client-flight-port`, default `:8860`) enables zero-overhead scatter-ingest directly from Python.

```python
import polars as pl
from gdb_client import GdbClient

# Connect to cluster seed
client = GdbClient(seed_url="http://localhost:8847")

# Bulk insert 1,000,000 vertices via Arrow Flight do_put:
df_vertices = pl.DataFrame({
    "id": range(1, 1_000_001),
    "name": [f"User_{i}" for i in range(1, 1_000_001)],
    "age": [20 + (i % 50) for i in range(1, 1_000_001)],
})
client.scatter_ingest_vertices(df_vertices, tag="User")

# Bulk insert edges:
df_edges = pl.DataFrame({
    "src": range(1, 1_000_000),
    "dst": range(2, 1_000_001),
})
client.scatter_ingest_edges(df_edges, edge_type="FOLLOWS")
```

---

## Part 4. Testing, Verification & Code Coverage

GDB enforces a continuous testing and coverage pipeline:

```bash
# 1. Run all workspace tests (unit, integration, and E2E)
cargo test --workspace

# 2. Run automated coverage pipeline (cargo-llvm-cov)
./scripts/coverage.sh

# 3. View interactive HTML coverage report
cargo llvm-cov --workspace --html --open
```

See **[COVERAGE.md](COVERAGE.md)** for detailed metrics across all subsystems and test suites.
