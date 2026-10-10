[English](README.md) | [Русский](README_RU.md)

# GDB — Distributed High-Performance In-Memory Graph Database (Nebula Graph Alternative)

[![Version](https://img.shields.io/badge/version-v0.5.1-blue.svg)](Cargo.toml)
[![CI & Code Coverage](https://github.com/kirill/gdb/actions/workflows/ci.yml/badge.svg)](.github/workflows/ci.yml)
[![Coverage](https://img.shields.io/badge/coverage-80.8%25-brightgreen)](docs/COVERAGE.md)
[![License](https://img.shields.io/badge/license-Apache--2.0-green.svg)](LICENSE)

A next-generation, high-performance distributed HTAP graph database engine built with **Rust**, featuring in-memory Compressed Sparse Row (CSR) topology, Apache Arrow columnar properties, openCypher/GQL query support, secondary property indexing, Cypher DML mutations, Multi-Raft leaderless replication, S3-backed tiered persistence, MPP distributed exchange over Arrow Flight, **Apple Metal (UMA Zero-Copy) / NVIDIA CUDA hardware acceleration with Discrete GPU Out-of-Core Memory Paging**, native **Vector Embeddings & Similarity Search**, and **Cost-Based Optimization (CBO)**.

---

## ⚡ Key Capabilities

1. **Extreme In-Memory Performance (Dual-Store & Secondary Indexes):**
   - **Chunked-CSR (Topology):** 64-byte cache-line aligned, graph traversal speed of **293.9 MILLION hops/sec** on a single Apple M5 core.
   - **Delta MemTable (OLTP):** Concurrent lock-free mutation buffer with MVCC Snapshot Isolation — **31.7 MILLION edges inserted per second**.
   - **Apache Arrow (Properties):** Columnar property storage for vectorized SIMD filtering.
   - **Secondary Property Indexes:** Ultra-fast $O(1)$ point lookups (`CREATE INDEX ON :Label(prop)` / `DROP INDEX`), automatically maintained across inserts, updates, and deletes.
   - **Background Micro-Compactor:** Merges Delta MemTable into compact Chunked-CSR without blocking reads.

2. **Query Language, Multi-Statement DML & Analytics (openCypher / GQL / CALL algo):**
   - **Multi-Statement Script Execution & Batch API:** Semicolon-delimited queries (`split_statements`), `/query` multi-statement execution, `/batch` endpoint, and Studio UI "Run All" with step progress tracking.
   - **Cypher Relationship DML:** `MATCH (a:User), (b:User) CREATE (a)-[r:FOLLOWS]->(b)`, `MERGE (a)-[r:KNOWS]->(b)`.
   - **Patterns & Multi-Hop Traversal:** `MATCH (a:User)-[:FOLLOWS*1..3]->(b:User) WHERE a.age > 25 RETURN b.name` with GPU-accelerated BFS expansion.
   - **`WITH` Operator Pipeline:** Inter-clause pipeline transformations, intermediate projections, and aggregations.
   - **Cypher Mutations & Merges:** `MATCH (u:User {name: 'Alice'}) SET u.age = 31`, `DELETE`, `DETACH DELETE`, `MERGE`.
   - **Physical Execution Plan (EXPLAIN):** `EXPLAIN <query>` produces an optimized physical execution plan DAG.
   - **Nebula Enterprise Analytics & Graph ML Suite:** `CALL algo.node2vec(...)`, `CALL algo.pageRank(...)`, `CALL algo.louvain(...)`, `CALL algo.wcc(...)`, `CALL algo.triangleCount(...)`, `CALL algo.kCore(...)`, `CALL algo.betweenness(...)`, `CALL algo.sssp(...)`, `CALL algo.similarity(...)`.

3. **Native Vector Embeddings & Similarity Search:**
   - **`VECTOR(dim)` Type:** Apache Arrow `FixedSizeList` columnar representation with vector literal parser `[1.0, 2.0, 3.0]`.
   - **Vector Similarity Search:** `CALL vector.similaritySearch(label, property, query_vector, k, metric)` supporting Cosine, DotProduct, and Euclidean (L2) distance metrics.
   - **Cost-Based Optimization (CBO):** `ANALYZE GRAPH;` gathering degree statistics and label cardinalities.

4. **Hardware GPU Acceleration (Metal / CUDA) & Discrete VRAM Out-of-Core Paging:**
   - **Maximal GPU Graph Offload:** Automatic GPU offload for graph traversal (`VarLengthExpand` wavefront BFS), vector similarity comparisons, and graph analytics.
   - **Discrete GPU Memory Paging (Out-Of-Core):** Windowed Chunked CSR Streaming with Double-Buffering prevents VRAM Out-Of-Memory errors on discrete GPUs (NVIDIA CUDA) when graph topology exceeds physical VRAM (`--gpu-max-vram-mb`). Lightweight `offsets` stay in VRAM while sequential chunks of `targets` stream asynchronously across PCIe.
   - **Apple Silicon (M-Series):** **Unified Memory Architecture (UMA)** enables GPU cores to read CSR graph topology directly from RAM with **zero PCIe copy overhead (Zero-Copy)**.
   - **Linux NVIDIA (CUDA):** Native CudaComputeBackend in `gdb-gpu` for parallel BFS, PageRank, Louvain, WCC, and Triangle Counting on server GPUs.
   - **CPU SIMD Fallback:** When GPU is disabled or absent, computations automatically execute via a vectorized Rayon CPU backend.

5. **GDB Studio v0.5.1 (Interactive Web Workspace):**
   - **"Run All" Multi-Statement Execution:** Step-by-step progress indicator (`Step: X / Y`), automated batching, and error tracing.
   - **Vector Array Inspector:** Clean monospace display for high-dimensional vectors and embedding scores.
   - **Clean Catalog Boot & Interactive Schema Manager:** Inspect, create, alter, and drop vertex tags and edge types visually.
   - **Force-Directed Physics & DAG Viewer:** 60 FPS Canvas & Unity 3D WebGL, execution plan DAG viewer, real-time sparkline telemetry.

6. **Dedicated Python SDK & Arrow Flight Transport (`gdb-client`):**
   - **Official Python Package:** Standalone `gdb-client` package with Polars DataFrame and NetworkX Graph export.
   - **Automated Benchmark Suite:** `scripts/py_client_benchmark.py` testing QPS, batch inserts, and graph traversals.
   - **Port Isolation:** Internal MPP shuffle (`--port 8848+`) separated from client ingestion (`--client-flight-port 8860+`).

7. **Horizontal Scaling & Tiered S3 Persistence:**
   - Leaderless Hash Ring with configurable replication factor (`--replication-factor 1..N`) and mode (`--sync` / `--async`).
   - Asynchronous partition snapshot export to **S3 / MinIO** in compressed **Apache Parquet (ZSTD)** format.

---

## 🚀 Running a 3-Node Cluster (ARM Mac + GPU)

Pre-built binaries are available in [`bin/`](bin/) for ARM Mac (`Mach-O 64-bit arm64`):
- `bin/gdb-server` — Node server daemon (Arrow Flight + HTTP REST + Multi-Raft + Metal GPU).
- `bin/gdb-cli` — Interactive shell and batch query runner.
- `bin/gdb-studio` — Web UI and analytics workspace.

### Option 1: Quick Launch via Script (Recommended)

The startup script automatically locates binaries (`bin/` or `target/release/`) and supports replication and GPU options:

```bash
# 1. Full synchronous replication (3 nodes, RF=3, SYNC — default):
./scripts/start_cluster.sh --nodes 3 --rf 3 --sync

# 2. Custom node count (e.g. 5 nodes with RF=3 or 1 single node):
./scripts/start_cluster.sh --nodes 5 --rf 3 --sync
./scripts/start_cluster.sh --nodes 1

# 3. Enable GPU acceleration (Apple Metal / NVIDIA CUDA):
./scripts/start_cluster.sh --nodes 3 --rf 3 --sync --enable-gpu true --gpu-offload-threshold 10000

# 4. Asynchronous replication (RF=3, ASYNC — maximum write TPS):
./scripts/start_cluster.sh --nodes 3 --rf 3 --async
```

Output:
```
============================================================
       Starting 3-Node GDB Cluster (Leaderless Ring)        
       Replication Factor: RF=3 | Mode: SYNC       
============================================================
[+] Peer 1 started (PID 61877): Flight :8848 | HTTP :8847 | RF: 3 | Mode: sync | Role: Peer
[+] Peer 2 started (PID 61878): Flight :8849 | HTTP :8846 | RF: 3 | Mode: sync | Role: Peer
[+] Peer 3 started (PID 61879): Flight :8850 | HTTP :8845 | RF: 3 | Mode: sync | Role: Peer

[✓] 3-node cluster is healthy and ready for queries!
    CLI connect:   ./bin/gdb-cli
    Web Studio UI: ./scripts/start_studio.sh (http://localhost:3000)
    HTTP endpoint: http://localhost:8847/query
    Logs:          tail -f logs/node*.log
    Stop cluster:  ./scripts/stop_cluster.sh
```

Stop cluster:
```bash
./scripts/stop_cluster.sh
```

---

### Option 2: Manual Multi-Node Launch

#### Terminal 1: Peer 1 (:8847 / :8848)
```bash
./bin/gdb-server \
  --node-id 1 \
  --partitions 8 \
  --port 8848 \
  --http-port 8847 \
  --wal-dir ./data/node1/wal \
  --peers http://127.0.0.1:8846,http://127.0.0.1:8845 \
  --replication-factor 3 \
  --replication-mode sync \
  --enable-gpu false
```

#### Terminal 2: Peer 2 (:8846 / :8849)
```bash
./bin/gdb-server \
  --node-id 2 \
  --partitions 8 \
  --port 8849 \
  --http-port 8846 \
  --wal-dir ./data/node2/wal \
  --peers http://127.0.0.1:8847,http://127.0.0.1:8845 \
  --replication-factor 3 \
  --replication-mode sync \
  --enable-gpu false
```

#### Terminal 3: Peer 3 (:8845 / :8850)
```bash
./bin/gdb-server \
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

---

## 💻 Running on Linux AMD64 (x86_64)

Release binaries for AMD64 (`x86_64`) are located in [`bin/amd64/`](bin/amd64/):
- `bin/amd64/gdb-server` — Node server for x86_64.
- `bin/amd64/gdb-cli` — Interactive CLI shell for x86_64.
- `bin/amd64/gdb-studio` — Web Studio UI for x86_64.

### Launch AMD64 Cluster:

```bash
./scripts/start_cluster_amd64.sh --rf 3 --sync
```

Stop cluster:
```bash
./scripts/stop_cluster_amd64.sh
```

---

## 🌐 GDB Studio — Web UI & Interactive Workspace

**GDB Studio** is a self-contained web workspace compiled into a single binary (`bin/gdb-studio`):
- **Zero dependencies** — no Node.js, npm, or external web servers required.
- **`🕸️ Graph View`:** Force-directed 60 FPS Canvas graph rendering, entity inspector, and PNG/SVG/CSV/JSON exports.
- **`📊 Table View`:** Columnar tabular results for analytical queries.
- **`🔍 Plan / Explain`:** Physical execution plan DAG visualization with operator cost analysis.
- **`🌐 Cluster Ring`:** Cluster topology, peer roles, token ranges, and one-click session switching.
- **`⚡ Storage & Resources`:** Real-time RAM monitoring, S3 status, compaction triggers, and live telemetry sparklines (QPS, Latency, RAM).
- **`🎮 3D Unity View`:** 3D WebGL spatial graph visualizer.

### Launch GDB Studio:
```bash
./scripts/start_studio.sh
# Open in browser: http://localhost:3000
```

Stop GDB Studio:
```bash
./scripts/stop_studio.sh
```

---

## 🎮 Interactive CLI Shell (`gdb-cli`)

```bash
./bin/gdb-cli
```

### Example Queries:

```sql
-- 1. Schema Definition & Secondary Indexes
CREATE VERTEX User (name STRING, age INT64);
CREATE EDGE FOLLOWS ();
CREATE INDEX ON :User(name);

-- 2. Data Ingestion (single & batch)
INSERT VERTEX User (id, name, age) VALUES (1, 'Alice', 30);
INSERT VERTEX User (id, name, age) VALUES (2, 'Bob', 25), (3, 'Charlie', 35);
INSERT EDGE FOLLOWS VALUES (1, 2), (2, 3), (3, 1);

-- 3. Mutations & Upsert (Cypher DML)
MATCH (u:User {name: 'Alice'}) SET u.age = 31;
MERGE (u:User {name: 'Dave', age: 28});

-- 4. Traversals, Aggregations & Pagination
MATCH (a:User)-[:FOLLOWS]->(b:User)
RETURN a.name, COUNT(b) AS followers
ORDER BY followers DESC;

-- 5. Execution Plan Inspection (EXPLAIN)
EXPLAIN MATCH (a:User)-[:FOLLOWS]->(b:User) WHERE a.name = 'Alice' RETURN b.name;

-- 6. Enterprise Graph Analytics (GPU/SIMD)
CALL algo.pageRank({damping: 0.85, max_iter: 20}) YIELD vertex_id, score;
CALL algo.louvain({max_iter: 10}) YIELD vertex_id, community_id;
CALL algo.wcc() YIELD vertex_id, component_id;
CALL algo.triangleCount() YIELD vertex_id, triangles;

-- 7. Cluster Introspection
SHOW CLUSTER;
SHOW RESOURCES;
SHOW GPU;
```

---

## 🧪 Testing & Code Coverage

GDB includes comprehensive unit, integration, and end-to-end (E2E) test suites with continuous tracking:

```bash
# Run full workspace test suite
cargo test --workspace

# Run code coverage analysis pipeline (cargo-llvm-cov)
./scripts/coverage.sh

# Open interactive HTML coverage report
cargo llvm-cov --workspace --html --open
```

Detailed test suites and subsystem breakdown can be viewed in **[docs/COVERAGE.md](docs/COVERAGE.md)**.

---

## 📖 In-Depth Documentation

- 👉 **[Testing & Benchmarking Suite Guide (TESTING_AND_BENCHMARKS.md)](docs/TESTING_AND_BENCHMARKS.md)**
- 👉 **[Server Configuration & Query Reference (SERVER_AND_QUERY_GUIDE.md)](docs/SERVER_AND_QUERY_GUIDE.md)**
- 👉 **[Test Coverage & Reliability Report (COVERAGE.md)](docs/COVERAGE.md)**
- 👉 **[Operations Guide & Bulk Ingestion (OPERATIONS_GUIDE.md)](docs/OPERATIONS_GUIDE.md)**

---

## License
Apache-2.0
