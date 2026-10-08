[English](OPERATIONS_GUIDE.md) | [Русский](OPERATIONS_GUIDE_RU.md)

# GDB Operations Guide: Cluster Setup, High-Throughput Bulk Ingestion, and Graph Analytics

Complete operational and administration guide for the distributed in-memory graph database **GDB** (a high-performance Nebula Graph / Nebula Enterprise alternative), release **v0.4.0**.

---

## 1. Cluster Installation and Launch

### 1.1. System Requirements & OS Tuning
To achieve peak performance (tens of millions of operations per second):
```bash
# 1. Increase file descriptor limits
ulimit -n 65535

# 2. Verify Rust toolchain
source "$HOME/.cargo/env"
cargo --version
```

### 1.2. Quick 3-Node Cluster Launch via Scripts

The repository includes cluster launch scripts with dynamic binary resolution (locating binaries in `bin/` or `target/release/`) and GPU options:

```bash
# ARM Mac (Apple Silicon):
./scripts/start_cluster.sh --rf 3 --sync

# Enable hardware acceleration (Apple Metal):
./scripts/start_cluster.sh --rf 3 --sync --enable-gpu true --gpu-offload-threshold 10000

# Linux AMD64 (x86_64) / NVIDIA CUDA:
./scripts/start_cluster_amd64.sh --rf 3 --sync

# Stop cluster:
./scripts/stop_cluster.sh
```

### 1.3. Manual Multi-Node Cluster Setup
Every GDB node is symmetric: it processes client requests, manages its Multi-Raft partitions, and serves Arrow Flight requests.

```bash
# Node 1 (Coordinator + Shards 0..7)
cargo run --release --bin gdb-server -- \
  --node-id 1 \
  --partitions 8 \
  --port 8848 \
  --http-port 8847 \
  --wal-dir ./data/node1/wal \
  --peers http://127.0.0.1:8846,http://127.0.0.1:8845 \
  --replication-factor 3 \
  --replication-mode sync \
  --enable-gpu false

# Node 2
cargo run --release --bin gdb-server -- \
  --node-id 2 \
  --partitions 8 \
  --port 8849 \
  --http-port 8846 \
  --wal-dir ./data/node2/wal \
  --peers http://127.0.0.1:8847,http://127.0.0.1:8845 \
  --replication-factor 3 \
  --replication-mode sync \
  --enable-gpu false

# Node 3
cargo run --release --bin gdb-server -- \
  --node-id 3 \
  --partitions 8 \
  --port 8850 \
  --http-port 8845 \
  --wal-dir ./data/node3/wal \
  --peers http://127.0.0.1:8847,http://127.0.0.1:8846 \
  --replication-factor 3 \
  --replication-mode sync \
  --enable-gpu false
```

### 1.4. Docker Compose Deployment with S3 (MinIO)
The root repository includes `docker-compose.yml` deploying a 3-node GDB cluster along with MinIO S3 storage:

```bash
# Start cluster and S3 in background
docker compose up -d

# Check service status
docker compose ps

# MinIO S3 Web Console: http://localhost:9001 (user: minioadmin, pass: minioadminpassword)
# GDB Arrow Flight port: localhost:8848
```

---

## 2. High-Throughput Bulk Ingestion

GDB uses a **Dual-Store** in-memory architecture:
1. Mutations write to a lock-free **Delta MemTable** and local append-only WAL at **over 30 million edges/sec**.
2. Once batch import completes, **Compaction** merges delta edges into contiguous, cache-aligned **Chunked-CSR**, ready for sub-millisecond traversals and GPU analytics.

### 2.1. End-to-End Bulk Ingestion Benchmark Script
To test ingestion of 1,000,000 edges and 50,000 vertices:

```bash
cargo run -p gdb-server --example benchmark_and_bulk_load --release
```

**Measured performance on Apple M5:**
- 50,000 vertices with Arrow properties: **16.3 ms** (`3,065,588 vertices/sec`).
- 1,000,000 edges into Delta MemTable: **31.5 ms** (`31.73 MILLION edges/sec`).
- CSR Compaction of 1,000,000 edges: **92.2 ms**.

### 2.2. Interactive CLI Ingestion (`gdb-cli`)
Launch CLI:
```bash
cargo run --release --bin gdb-cli
```

Create schema, secondary indexes, and insert data:
```sql
-- 1. Create schema
CREATE VERTEX User (name STRING, age INT64);
CREATE EDGE FOLLOWS ();

-- 2. Create secondary index for O(1) property lookup
CREATE INDEX ON :User(name);

-- 3. Insert vertices
INSERT VERTEX User (id, name, age) VALUES (1, 'Alice', 30);
INSERT VERTEX User (id, name, age) VALUES (2, 'Bob', 25);
INSERT VERTEX User (id, name, age) VALUES (3, 'Charlie', 35);
INSERT VERTEX User (id, name, age) VALUES (4, 'Dave', 22);

-- 4. Insert edges
INSERT EDGE FOLLOWS FROM 1 TO 2;
INSERT EDGE FOLLOWS FROM 2 TO 3;
INSERT EDGE FOLLOWS FROM 3 TO 1;
INSERT EDGE FOLLOWS FROM 3 TO 4;

-- 5. Mutation & Upsert (DML)
MATCH (u:User {name: 'Alice'}) SET u.age = 31;
MERGE (u:User {name: 'Eve', age: 28});

-- 6. Trigger CSR compaction (maximize traversal speed)
compact;
```

---

## 3. Query Execution and Graph Analytics

GDB supports three tiers of analytical computing:
1. **Cypher Queries (MATCH, Filter, Expand, DML, Indexes, Explain)** for targeted point and k-hop lookups.
2. **Aggregations & Pagination** (`COUNT`, `SUM`, `AVG`, `MIN`, `MAX`, `DISTINCT`, `ORDER BY`, `SKIP`, `LIMIT`).
3. **Nebula Enterprise Analytics Suite (`CALL algo.<name>`)** for whole-graph GPU-accelerated algorithms.

### 3.1. Cypher Query Execution (openCypher / GQL)
```sql
-- k-hop graph traversal with property filtering
MATCH (a:User)-[:FOLLOWS]->(b:User)-[:FOLLOWS]->(c:User)
WHERE a.age >= 25
RETURN a.name, b.name, c.name
ORDER BY a.name ASC
LIMIT 10;

-- Aggregations & Group By
MATCH (a:User)-[:FOLLOWS]->(b:User)
RETURN a.name, COUNT(b) AS followers
ORDER BY followers DESC;

-- Inspect Physical Execution Plan (EXPLAIN)
EXPLAIN MATCH (a:User)-[:FOLLOWS]->(b:User)
WHERE a.name = 'Alice'
RETURN b.name;
```

---

### 3.2. Enterprise Graph Analytics Algorithms

All algorithms return results in **Apache Arrow** (`RecordBatch`) columnar format with zero serialization overhead:

#### A. Community Detection & Clustering
```sql
-- 1. Louvain Community Detection (Modularity optimization, GPU/SIMD)
CALL algo.louvain({max_iter: 10})
YIELD vertex_id, community_id;

-- 2. WCC (Weakly Connected Components, GPU/SIMD)
CALL algo.wcc()
YIELD vertex_id, component_id;

-- 3. SCC (Strongly Connected Components)
CALL algo.scc()
YIELD vertex_id, component_id;

-- 4. Triangle Count & Local Clustering Coefficient (LCC, GPU/SIMD)
CALL algo.triangleCount()
YIELD vertex_id, triangles;

-- 5. K-Core Decomposition (Dense subgraphs)
CALL algo.kCore()
YIELD vertex_id, coreness;
```

#### B. Centrality & Ranking
```sql
-- 6. PageRank (damping 0.85, 20 iterations, GPU SpMV)
-- 1 Million edges in 6 milliseconds!
CALL algo.pageRank({damping: 0.85, max_iter: 20, tolerance: 0.0001})
YIELD vertex_id, score;

-- 7. Betweenness Centrality (Brandes algorithm)
CALL algo.betweenness({normalized: true})
YIELD vertex_id, betweenness;

-- 8. Closeness Centrality (Harmonic distance)
CALL algo.closeness()
YIELD vertex_id, closeness;

-- 9. Degree Centrality (In/Out/Total degree)
CALL algo.degree()
YIELD vertex_id, in_degree, out_degree, total_degree;
```

#### C. Pathfinding & Similarity
```sql
-- 10. SSSP (Single-Source Shortest Path)
CALL algo.sssp({source: 1})
YIELD vertex_id, distance;

-- 11. Similarity Metrics (Jaccard, Cosine, Common Neighbors)
CALL algo.similarity({node1: 1, node2: 2})
YIELD jaccard, common_neighbors;
```

---

## 4. Hardware Acceleration (Metal on Mac / CUDA on Linux)

GDB features a high-performance GPU compute pipeline:
- **CLI Options:**
  - `--enable-gpu true` — enables GPU acceleration (default: `false`).
  - `--gpu-device <ID>` — selects GPU device index when multiple GPUs exist (e.g. `--gpu-device 0`).
  - `--gpu-offload-threshold <N>` — offload threshold in number of edges (default: 10,000 edges).
- **Apple Silicon (M-Series):** **Unified Memory Architecture (UMA)** allows GPU cores to read CSR graph topology directly from RAM with **zero PCIe copy overhead (Zero-Copy)**.
- **Linux NVIDIA (CUDA):** Native CUDA backend supporting Tesla V100, A100, H100, and RTX GPUs.
- **Adaptive Dispatcher:**
  - Queries $< 10,000$ edges run on vectorized CPU SIMD to avoid dispatch overhead.
  - Large traversals and analytics ($> 10,000$ elements) automatically offload to Metal / CUDA kernels.

---

## 5. Tiered Storage and S3 Snapshots

For durable persistence and fast disaster recovery:
1. GDB periodically exports compact CSR partitions into **Apache Parquet (ZSTD)** on S3:
   `partitions/p{id}/snapshot_v{version}.parquet`.
2. On node restart, the node downloads the latest Parquet snapshot from S3 and replays remaining records from local append-only Raft WAL (`gdb-wal`).
