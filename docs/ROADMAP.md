# GDB: Development Roadmap

This document outlines strategic milestones, architectural priorities, and future release plans for **GDB**.

---

## 📌 Current Status (Version v0.5.2 — Huygens)

- ✅ **Query Planner Primary Key Pushdown (v0.5.2)**:
  - Fast-path pushdown of `WHERE id = <val>` into `PhysicalOperator::ScanVertices { id_filter: Some(vid) }`.
  - Added $O(1)$ point-lookup vertex existence checks (`has_vertex`) across StorageEngine, VertexPropertyTable, and DeltaMemTable.
  - Eliminates multi-hop wavefront BFS traversal timeouts and redundant full-graph scans (>5,000x speedup).
- ✅ **GDB Studio Web UI & Startup Script Overhaul (v0.5.2)**:
  - Updated `scripts/start_studio.sh` with robust CLI flag parsing (`--port`, `--cluster-url`, `--host`, `--no-browser`) and legacy positional argument compatibility.
  - Resilient health check retry loop with automatic diagnosis and log dumping on failure.
- ✅ **Discrete GPU Out-of-Core Memory Paging (v0.5.1)**:
  - High-performance **Windowed Chunked CSR Streaming with Double-Buffering** for discrete GPUs (NVIDIA CUDA / non-UMA architectures).
  - Keeps lightweight `offsets` array in GPU VRAM while streaming sequential chunks of `targets` across PCIe asynchronously, eliminating PCIe page-fault thrashing and preventing GPU OOM on large graphs.
  - Server configuration `--gpu-max-vram-mb` (env: `GDB_GPU_MAX_VRAM_MB`, default: 2048 MB).
  - Extended REST telemetry: `GET /gpu` reporting `max_vram_mb` and active `paging_strategy`.
- ✅ **Multi-Statement & Relationship DML**:
  - Lexical script splitter (`split_statements`) respecting comments and string quotes.
  - Multi-statement execution via `/query` and dedicated `POST /batch` API.
  - Cypher relationship DML: `MATCH (a:Tag), (b:Tag) CREATE (a)-[r:TYPE]->(b)` and `MERGE (a)-[r:TYPE]->(b)`.
  - GDB Studio UI: "Run All" multi-statement execution with step-by-step progress indicator (`Step: X / Y`) and syntax error tracing.
  - GDB CLI: Multi-statement execution in both local in-memory and cluster modes.
- ✅ **Native Vector Embeddings & Similarity Search**:
  - `VECTOR(dim)` native data type with Apache Arrow `FixedSizeList` columnar storage.
  - Vector literal parser `[1.0, 2.0, 3.0]`.
  - `CALL vector.similaritySearch(label, property, query_vector, k, metric)` supporting Cosine, DotProduct, and Euclidean (L2) metrics.
- ✅ **GPU Search Offload & Graph Acceleration**:
  - Wavefront BFS path expansion (`VarLengthExpand`) offloaded to GPU (Apple Metal UMA / NVIDIA CUDA) with Rayon CPU fallback.
  - Parallel vector similarity search accelerated on SIMD and GPU.
  - Comprehensive GPU graph analytics suite (PageRank, Louvain, WCC, Triangle Counting, SSSP).
- ✅ **CBO Optimizer & Graph ML**:
  - Cost-Based Optimization statistics gathering via `ANALYZE GRAPH;`.
  - In-database random walk & Skip-Gram Node2Vec embedding generation (`CALL algo.node2vec(...)`).
- ✅ **Official Python Client (`gdb-client` on PyPI)**:
  - Unified into standalone `gdb-py-client` project published to PyPI (`gdb-client>=0.5.1`).
  - Zero-copy Polars, Arrow, Pandas, and NetworkX exports.
  - Parallel Arrow Flight scatter-ingest and REST batch APIs.
- ✅ **Quality & Code Coverage**:
  - 100% test pass rate across all 13 workspace crates.
  - Workspace test coverage $\ge 80\%$ (**80.75% lines / 80.84% regions**).

---

## 📅 Release Roadmap

| Release | Codename | Primary Focus | Key Deliverables |
| :---: | :---: | :--- | :--- |
| **v0.5.2** | **Huygens** *(Current)* | **Planner Pushdown & Studio Overhaul** | Primary ID pushdown in `ScanVertices`, robust `start_studio.sh` CLI parsing, and PyPI `gdb-client` unification. |
| **v0.6.0** | **Hals** | **AI Agent Integrations & Advanced Indexing** | 1. Official **LangChain** and **LangGraph** ecosystem integration modules (`langchain-gdb`, GraphVectorStore).<br>2. HNSW Vector Graph Index for sub-millisecond approximate nearest neighbors on billion-scale vector datasets.<br>3. Graph RAG hybrid retrieval pipelines combining multi-hop graph traversals with vector semantic scoring. |
| **v0.7.0** | **Steen** | **Distributed ACID Transactions & Advanced Analytics** | 1. Distributed multi-partition ACID transactions (`BEGIN`, `COMMIT`, `ROLLBACK`) with 2PC and Snapshot Isolation.<br>2. Weighted shortest path algorithms (Dijkstra, A*). |
| **v1.0.0** | **Erasmus** | **Enterprise Security & LTS** | 1. TLS / mTLS transport encryption and JWT authentication.<br>2. Fine-grained Role-Based Access Control (RBAC) per label and property.<br>3. Streaming `InstallSnapshot` over Arrow Flight RPC in Multi-Raft.<br>4. Long-Term Support (LTS) release with strict API compatibility guarantees. |

---

## 🎯 Architectural Highlights (v0.5.1)

### 1. Vector Search & Graph Hybrid Traversal
Vector properties are stored directly in Apache Arrow `FixedSizeList` columns alongside regular vertex properties. This allows seamless blending of Cypher graph pattern matching and vector similarity:
```cypher
MATCH (u:User {dept: 'Research'})-[:AUTHORED]->(d:Document)
CALL vector.similaritySearch('Document', 'embedding', [0.12, 0.45, -0.33], 5, 'cosine')
YIELD vertex_id, score
RETURN d.title, score ORDER BY score DESC;
```

### 2. Maximal GPU Offload for Graph Operations
Graph operations automatically route through `QueryExecutor::with_gpu`. When topology size exceeds the configured threshold, variable-length path expansions (`VarLengthExpand`) run wavefront BFS kernels directly on GPU cores, keeping data in UMA Zero-Copy buffers on Apple Silicon.

### 3. Python SDK (`gdb-client`)
```python
from gdb_client import GdbClient

client = GdbClient(endpoint="http://127.0.0.1:8847")
df = client.query_df("MATCH (p:Person) RETURN p.name, p.age;")
G = client.query_graph("MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a.id, b.id, r.weight;")
```
