# GDB: Development Roadmap

This document outlines strategic milestones, architectural priorities, and future release plans for **GDB**.

---

## 📌 Current Status (Version v0.5.6 — Release)

- ✅ **Cross-Node Multi-Hop Traversal & Sharded $RF=1$ Support (v0.5.6)**:
  - Introduced `RemoteEdgeExpander` trait in `gdb-planner` with batch pre-fetching of boundary vertex edges (`batch_get_out_edges`).
  - Added high-throughput internal HTTP endpoint `POST /storage/out_edges` and `ClusterEdgeExpander` in `gdb-server`.
  - Full support for pure distributed sharding ($RF=1$, 0% RAM duplication) for multi-hop paths (`MATCH (a)-[:REL*1..k]->(b)`).
  - Cluster telemetry aggregation in `/resources`: accurate `cluster_total_edges` (sum for $RF=1$, max for $RF \ge N$) and `cluster_total_vertices`.
- ✅ **Python SDK (`gdb-client` v0.5.6 on PyPI)**:
  - `scatter_ingest_edges` and `scatter_ingest_vertices` strictly respect `replication_factor` (streams to exactly 1 primary node $src \pmod N$ when $RF=1$).
  - Added `target_replica_nodes_for_key` helper to `ClusterTopology`.
- ✅ **GDB Studio UI (v0.5.6)**:
  - Fixed blocking semi-transparent blur overlay in Unity 3D Spatial Canvas, restoring interactive mouse controls.
  - Display cluster totals and local sharded node breakdown.
- ✅ **Cluster-Wide MPP Scatter-Gather & Vector Search (v0.5.5)**:
  - Scatter-gather execution of Cypher read queries and analytical procedures with stream merging.
  - Distributed `CALL vector.similaritySearch` with global Top-K merging across `cosine`, `dot`, and `l2` metrics.
  - Explicit routing control via `local_only: true` parameter.
- ✅ **Announced Flight Addresses (v0.5.4)**:
  - `--flight-address` (internal Raft/Flight inter-node gossip) and `--client-flight-address` (external client MPP ingest behind Docker/NAT/LB).
- ✅ **Algorithm Subgraph Filtering by Edge Type & Vertex Label (v0.5.3)**:
  - Subgraph CSR projection via `ChunkedCsr::filter_by_edge_type(edge_type)` maintaining consistent vertex identity.
  - Cypher support for `CALL algo.*({edge_type: 'TYPE', label: 'LABEL'})` allowing isolated analytics execution (PageRank, WCC, Louvain, SSSP, etc.) on heterogeneous graphs.
  - Keyword identifiers and backtick escaping (``` `VERTEX` ```).
- ✅ **Planner Primary Key Pushdown & Out-of-Core GPU Paging (v0.5.1–v0.5.2)**:
  - Pushdown of `WHERE id = <val>` into vertex scan ($O(1)$ existence checks).
  - Windowed Chunked CSR Streaming with Double-Buffering for discrete GPUs preventing VRAM OOM.

---

## 🏗️ 4-Component Roadmap Matrix

### 1. `gdb-server` (Storage Engine, Distributed Multi-Raft & Compute)
* **[v0.6.0] Distributed Pregel / BSP Analytics Framework ($RF < N$ & $RF=1$)**:
  * Iterative **Bulk Synchronous Parallel (BSP) / Pregel** computation engine for global graph algorithms (`CALL algo.pageRank`, `CALL algo.wcc`, `CALL algo.louvain`, `CALL algo.sssp`, `CALL algo.betweenness`) over distributed shards when $RF=1$ or arbitrary $RF < N$.
  * Inter-shard Superstep Message Shuffle: boundary vertex delta propagation across cluster nodes via high-speed **Arrow Flight Shuffle Streams** at the end of each superstep.
  * Global convergence: local delta tracking converges to global `tolerance`, providing 100% mathematical parity with single-node full-graph execution while maintaining a minimal RAM footprint ($1 \times \text{RAM}$).
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
* **[v0.6.0] Distributed Analytics Monitor (Pregel Superstep Tracker)**:
  * Real-time superstep progress tracker for distributed analytics (iteration progress, volume of Arrow Flight messages exchanged across shards).
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
| **v0.5.6** | **Huygens+** *(Current)* | **Cross-Node Multi-Hop Traversal (RF=1), Telemetry Aggregation, Studio UI Fix** | 1. 2/3-hop traversals over pure sharding ($RF=1$) via `RemoteEdgeExpander`.<br>2. Cluster telemetry aggregation in `/resources`.<br>3. $RF$-aware `scatter_ingest` in Python SDK.<br>4. Fixed Unity 3D Spatial Canvas overlay. |
| **v0.6.0** | **Hals** | **Distributed Pregel/BSP ($RF=1$), Graph Spaces & AI Integrations** | 1. **Distributed Pregel/BSP Analytics**: global PageRank, WCC, Louvain over sharded graphs ($RF=1$) via Arrow Flight Shuffle.<br>2. **Multi-Graph**: `CREATE GRAPH` / `USE GRAPH` / `SHOW GRAPHS` (schema & CSR isolation).<br>3. Graph space selectors across GDB Studio, REPL CLI, and Python SDK.<br>4. LangChain & LangGraph connectors (`langchain-gdb`).<br>5. Native HNSW Vector Graph Index and hybrid Graph RAG. |
| **v0.7.0** | **Steen** | **Distributed ACID Transactions & Advanced Analytics** | 1. Distributed multi-partition ACID transactions (2PC, Snapshot Isolation).<br>2. Weighted shortest path algorithms (Dijkstra, A*).<br>3. Real-time topology and GPU telemetry in Studio. |
| **v1.0.0** | **Erasmus** | **Enterprise Security & LTS** | 1. TLS/mTLS, JWT authentication, and granular RBAC per graph/label/property.<br>2. Streaming Raft Snapshots over Arrow Flight RPC.<br>3. Long-Term Support (LTS) release with strict API stability guarantees. |
