# GDB: Development Roadmap

This document outlines strategic milestones, architectural priorities, and future release plans for **GDB**.

---

## 📌 Current Status (Version v0.4.2 — Huygens)

- ✅ **openCypher `WITH` Pipeline**: intermediate projections, aggregations (`count`, `sum`, `avg`, `min`, `max`), `WHERE` filters, `ORDER BY` sorting, `SKIP`/`LIMIT` pagination.
- ✅ **Schema Manager & Clean Catalog in GDB Studio**: zero forced default vertices, dynamic DDL schema introspection, dropping/altering labels and edge types (`DROP`/`ALTER`).
- ✅ **NVIDIA GPU `nvidia-smi` Registration**: dynamic CUDA Driver API context allocation and VRAM buffer reservation (`Type: C`).
- ✅ **Workspace Test Coverage >= 80% in CI**: **81.62%** line coverage across the entire workspace (`cargo llvm-cov`).
- ✅ **Automated Test Schema Lifecycle**: automated setup and teardown for test topologies across all benchmark/stress scripts with `--keep-schema` support.
- ✅ **Dedicated GPU Benchmark Suite**: `scripts/gpu_benchmark.py` (Metal UMA / NVIDIA CUDA / CPU fallback).
- ✅ **Documentation**: comprehensive testing and benchmarking guides in `docs/TESTING_AND_BENCHMARKS.md` and `docs/TESTING_AND_BENCHMARKS_RU.md`.

---

## 📅 Release Roadmap

| Release | Codename | Primary Focus | Key Deliverables |
| :---: | :---: | :--- | :--- |
| **v0.5.0** | **Vermeer** | **Multi-Statement & Relationship DML** | 1. Multi-statement execution delimited by `;` in Studio, CLI, and `POST /batch`.<br>2. `MATCH ... CREATE (a)-[r:TYPE]->(b)` with edge properties.<br>3. `MERGE (a)-[r:TYPE]->(b) ON CREATE SET ...`.<br>4. Batch-insert optimizations in `data_loader.py` and test seeders (`VALUES (...), (...)`). |
| **v0.6.0** | **Hals** | **GPU Search Offload & Vector Embeddings** | 1. **GPU Search Offload**: multi-hop path traversal (`VarLengthExpand` / BFS) on GPU (Metal UMA / CUDA) and parallel columnar predicate filtering.<br>2. Native `VECTOR(dim)` data type in schema & Arrow storage.<br>3. HNSW vector index with SIMD/GPU acceleration.<br>4. Hybrid graph-vector search procedure `CALL vector.similaritySearch(...)`. |
| **v0.7.0** | **Steen** | **Python SDK & Client Benchmark** | 1. Official `gdb-py-client` library with Arrow Flight RPC and Polars support.<br>2. High-throughput benchmark script `scripts/py_client_benchmark.py` (Flight vs HTTP, latency percentiles).<br>3. Zero-Copy export to PyTorch Geometric and NetworkX.<br>4. ACID Transactions `BEGIN` / `COMMIT` / `ROLLBACK` with Snapshot Isolation. |
| **v0.8.0** | **Ruisdael** | **CBO Optimizer & Graph ML** | 1. Cost-Based Optimizer backed by `ANALYZE GRAPH;` statistics.<br>2. In-database GPU Node2Vec and GraphSAGE training/inference.<br>3. Weighted `shortestPath` (Dijkstra, A*).<br>4. GPU Memory Paging (UVM) for graphs exceeding discrete VRAM. |
| **v1.0.0** | **Erasmus** | **Enterprise Security & LTS** | 1. TLS / mTLS and JWT authentication.<br>2. Role-Based Access Control (RBAC).<br>3. Streaming `InstallSnapshot` over Flight RPC in Multi-Raft.<br>4. Long-Term Support (LTS) release with strict API compatibility. |

---

## 🎯 Key Strategic Initiatives

### 1. Multi-Statement Script Execution Delimited by `;` (v0.5.0)
- **Lexical Script Splitter (`ScriptSplitter`)**: robust statement splitting ignoring semicolons in string literals, escapes, and comments (`//`, `--`, `#`).
- **GDB Studio UI**:
  - *Run All* and *Run Selected* actions.
  - Real-time step progress indicator (e.g. `Step 4 of 18`).
  - Fail-fast with highlighting on error line.
  - Summary execution card: total duration, `rows_affected`, final graph visualization.
- **Server API**: dedicated `POST /batch` endpoint for atomic/sequential execution.

### 2. GPU Search Offload & Traversal (v0.6.0)
- **Executor & GPU Dispatcher Link**: integrate `GpuDispatcher` directly into `QueryExecutor`.
- **Adaptive `VarLengthExpand` Operator**:
  - For paths deeper than 1 hop (`MATCH (a)-[*1..5]->(b)`) exceeding `--gpu-threshold` (10,000 edges default), dispatch to `parallel_bfs_step` kernel on Metal UMA / CUDA.
  - Wavefront frontier expansion executed in parallel by thousands of GPU threads in single-digit milliseconds.
- **Parallel Predicate Filtering (Columnar Scan Pushdown)**:
  - Scanning Apache Arrow property columns on GPU with bitmask selection vector generation at memory bus speeds (>800 GB/s on Mac UMA, >1 TB/s on CUDA).
- **Subgraph & Motif Search**:
  - GPU warp-cooperative adjacency list intersection for fast clique and cycle discovery (`(a)->(b)->(c)->(a)`).

### 3. Native Vector Embeddings & HNSW Indexing (v0.6.0)
- **Data Type**: `VECTOR(dim)` (`f32` fixed-dimension: 384, 768, 1536, 3072).
- **Storage**: Apache Arrow `FixedSizeListArray<Float32>` for zero-copy memory layout.
- **HNSW Index**:
  - Distance metrics: `Cosine Similarity`, `Dot Product`, `Euclidean (L2)`.
  - Hardware acceleration: SIMD (AVX2/AVX-512, NEON) and GPU tensor cores / Metal MPS.
- **Cypher Procedure**:
  ```sql
  CALL vector.similaritySearch('Document', 'embedding', [0.024, -0.198, ...], 10)
  YIELD node, similarity
  MATCH (node)-[:AUTHORED_BY]->(author:Person)
  RETURN node.title, author.name, similarity;
  ```

### 4. High-Performance Python SDK (`gdb-py-client`) & Benchmark (v0.7.0)
- **Benchmark Suite `scripts/py_client_benchmark.py`**:
  - Compares ingestion throughput: Arrow Flight RPC vs HTTP REST API.
  - High-volume batch insertions using Polars DataFrames.
  - Latency percentiles (P50, P95, P99) under concurrent worker workloads.
  - Automated schema lifecycle management (`--keep-schema`).
- **ML Ecosystem**: Zero-Copy export into `torch_geometric.data.Data` and `networkx.DiGraph`.
