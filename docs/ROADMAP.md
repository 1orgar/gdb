# GDB: Development Roadmap

This document outlines strategic milestones, architectural priorities, and future release plans for **GDB**.

---

## 📌 Current Status (Version v0.5.3 — Huygens+)

- ✅ **Algorithm Subgraph Filtering by Edge Type & Vertex Label (v0.5.3)**:
  - Subgraph CSR projection via `ChunkedCsr::filter_by_edge_type(edge_type)` maintaining consistent vertex identity.
  - Cypher support for `CALL algo.*({edge_type: 'TYPE', label: 'LABEL'})` allowing isolated analytics execution (PageRank, WCC, Louvain, SSSP, etc.) on heterogeneous graphs.
  - Descriptive schema errors when invalid edge types or vertex labels are supplied.
- ✅ **Keyword Identifiers & Backtick Escaping (v0.5.3)**:
  - Schema labels and relationship types can match Cypher reserved keywords (`DROP VERTEX VERTEX;`, `DROP EDGE EDGE;`, `RANK`, `ORDER`).
  - openCypher backtick escaping support (``` `VERTEX` ```).
- ✅ **Official Python SDK (`gdb-client` v0.5.3 on PyPI)**:
  - Published to PyPI (`gdb-client==0.5.3`).
  - Streaming partition ingestion in 64k chunks with 256MB Arrow Flight message limits.
  - Cluster topology auto-discovery (`flight_port`, `client_flight_port`) via REST API.
- ✅ **Query Planner Primary Key Pushdown (v0.5.2)**:
  - Fast-path pushdown of `WHERE id = <val>` into `PhysicalOperator::ScanVertices { id_filter: Some(vid) }`.
  - Added $O(1)$ point-lookup vertex existence checks (`has_vertex`) across StorageEngine, VertexPropertyTable, and DeltaMemTable.
  - Eliminates multi-hop wavefront BFS traversal timeouts and redundant full-graph scans (>5,000x speedup).
- ✅ **GDB Studio Web UI & Startup Script Overhaul (v0.5.2)**:
  - Updated `scripts/start_studio.sh` with robust CLI flag parsing (`--port`, `--cluster-url`, `--host`, `--no-browser`) and positional argument compatibility.
  - Resilient health check retry loop with automatic diagnosis and log dumping on failure.
- ✅ **Discrete GPU Out-of-Core Memory Paging (v0.5.1)**:
  - High-performance **Windowed Chunked CSR Streaming with Double-Buffering** for discrete GPUs (NVIDIA CUDA / non-UMA architectures).
  - Keeps lightweight `offsets` array in GPU VRAM while streaming sequential chunks of `targets` across PCIe asynchronously, eliminating PCIe page-fault thrashing and preventing GPU OOM on large graphs.
  - Server configuration `--gpu-max-vram-mb` (env: `GDB_GPU_MAX_VRAM_MB`, default: 2048 MB).
- ✅ **Multi-Statement & Relationship DML**:
  - Lexical script splitter (`split_statements`) respecting comments and string quotes.
  - Multi-statement execution via `/query` and dedicated `POST /batch` API.
  - Cypher relationship DML: `MATCH (a), (b) CREATE (a)-[r:TYPE]->(b)` and `MERGE (a)-[r:TYPE]->(b)`.
  - GDB Studio UI: "Run All" multi-statement execution with step-by-step progress indicator (`Step: X / Y`).
- ✅ **Native Vector Embeddings & Similarity Search**:
  - `VECTOR(dim)` native data type with Apache Arrow `FixedSizeList` columnar storage.
  - `CALL vector.similaritySearch(label, property, query_vector, k, metric)` (Cosine, DotProduct, Euclidean L2).
- ✅ **Quality & Code Coverage**:
  - 100% test pass rate across all workspace crates; workspace test coverage $\ge 80\%$.

---

## 🏗️ 4-Component Roadmap Matrix

### 1. `gdb-server` (Storage Engine, Distributed Multi-Raft & Compute)
* **[v0.6.0] Graph Spaces / Namespaces (Multi-Tenancy & Multi-Graph)**:
  * DDL operations: `CREATE GRAPH <name>;`, `SHOW GRAPHS;`, `USE GRAPH <name>;`, `DROP GRAPH <name>;`.
  * Physical & logical isolation: each space owns dedicated `GraphSchema`, partitioned `PartitionStorageEngine`, WAL directories, and CSR topology buffers.
  * Multi-space routing via `X-GDB-Graph: <name>` HTTP header and Arrow Flight ticket descriptors (`gdb://<graph>/...`).
* **[v0.6.0] HNSW Vector Graph Index**:
  * Native HNSW indexing for sub-millisecond approximate nearest neighbor (ANN) search on multi-million node graphs.
* **[v0.7.0] Distributed Multi-Partition ACID Transactions**:
  * Two-Phase Commit (2PC) and Snapshot Isolation for distributed writes (`BEGIN`, `COMMIT`, `ROLLBACK`).
* **[v0.7.0] Advanced Graph Analytics Kernels**:
  * Weighted shortest paths (Dijkstra, A*), K-hop neighborhood analytics, and closeness expansion.
* **[v1.0.0] Enterprise Security & Multi-Raft Streaming Snapshots**:
  * TLS / mTLS transport encryption, JWT authentication, and RBAC at graph, label, and property granularity.
  * Streaming `InstallSnapshot` over Arrow Flight RPC in Multi-Raft.

### 2. `gdb-studio` (Web UI & Analytical Workspace)
* **[v0.6.0] Graph Spaces Switcher**:
  * Dropdown selector for active graph spaces in the Studio top header (`USE GRAPH`) with instant session switching.
* **[v0.6.0] Interactive Multi-Graph Schema Explorer**:
  * Visual entity relationship graph showing active vertex labels, edge types, indexes, and live vertex counters.
* **[v0.6.0] Visual Graph Analytics Panel**:
  * One-click execution of PageRank, Louvain, and WCC with visual edge type filtering (`edge_type`) and community coloring.
* **[v0.7.0] Cluster & GPU Telemetry Dashboard**:
  * Live cluster ring map displaying partition assignments, replication sync health, and real-time GPU VRAM telemetry.

### 3. `gdb-cli` (Terminal Client & Scripting)
* **[v0.6.0] Session Context & Graph Space REPL**:
  * `USE GRAPH <name>;` command tracking active graph space in prompt: `gdb (default_graph)>`.
* **[v0.6.0] Interactive REPL with Tab-Completion**:
  * Smart tab-completion for Cypher keywords, vertex labels, relationship types, and properties.
* **[v0.7.0] Backup & Restore Tools**:
  * CLI tools `gdb-cli dump --graph <name>` and `gdb-cli restore` supporting Apache Parquet and Arrow IPC streaming formats.

### 4. `gdb-py-client` (Python SDK)
* **[v0.6.0] Graph Spaces Support**:
  * Target graph parameter: `client = GdbClient(endpoint="...", graph="epinions")`.
  * Space management: `client.list_graphs()`, `client.create_graph("...")`, `client.use_graph("...")`.
* **[v0.6.0] AI Agent Ecosystem Integrations**:
  * Official `langchain-gdb` (VectorStore and GraphStore) and `langgraph-gdb` connectors for Graph RAG agentic workflows.
* **[v0.7.0] Streaming Generators & Zero-Copy Polars Traversal**:
  * High-performance chunked generators streaming directly into Polars LazyFrames.

---

## 📅 Milestone Schedule

| Release | Codename | Primary Focus | Key Deliverables |
| :---: | :---: | :--- | :--- |
| **v0.5.3** | **Huygens+** *(Current)* | **Flight Streaming, Keywords, Algorithm Filtering** | Subgraph `edge_type` filtering in `CALL algo.*`, backtick identifier quoting, PyPI `gdb-client==0.5.3`, Flight 256MB limit. |
| **v0.6.0** | **Hals** | **Graph Spaces (Namespaces) & AI Integrations** | 1. Multi-Graph: `CREATE GRAPH` / `USE GRAPH` / `SHOW GRAPHS` (schema & CSR isolation).<br>2. Graph space selectors across GDB Studio, REPL CLI, and Python SDK.<br>3. LangChain & LangGraph connectors (`langchain-gdb`).<br>4. Native HNSW Vector Graph Index and hybrid Graph RAG. |
| **v0.7.0** | **Steen** | **Distributed ACID Transactions & Advanced Analytics** | 1. Distributed multi-partition ACID transactions (2PC, Snapshot Isolation).<br>2. Weighted shortest path algorithms (Dijkstra, A*).<br>3. Real-time topology and GPU telemetry in Studio. |
| **v1.0.0** | **Erasmus** | **Enterprise Security & LTS** | 1. TLS/mTLS, JWT authentication, and granular RBAC per graph/label/property.<br>2. Streaming Raft Snapshots over Arrow Flight RPC.<br>3. Long-Term Support (LTS) release with strict API stability guarantees. |
