# Changelog

All notable changes to **GDB** (Distributed High-Performance In-Memory Graph Database) and **gdb-client** will be documented in this file.

---

## [0.5.7] - 2026-10-15

### 🚀 New Features
- **Entity JSON Serialization (`RETURN a, b`):** Whole vertex references in query projections now serialize to structured JSON objects (`{"id": vid, "label": label, "properties": {...}}`) with full property metadata.
- **openCypher Built-in Functions:** Added first-class support for `id(n)`, `_id(n)`, `label(n)`, `labels(n)`, `n.id`, and `n.label`.
- **Dynamic Entity Parsing in GDB Studio:** Studio automatically deserializes entity JSON objects, populates the Inspector drawer with property key-values, and highlights vertices with dynamic color palettes based on their label.
- **Architecture Refactoring & Modularization:**
  - `gdb-server`: Refactored monolithic `lib.rs` into specialized submodules (`src/state.rs`, `src/cluster/topology.rs`, `src/cluster/resolver.rs`, `src/handlers/storage.rs`, `src/handlers/resources.rs`, `src/handlers/query.rs`).
  - `gdb-studio`: Split inline 3500-line UI into standalone modular assets under `web/` (`styles.css`, `body.html`, `graph.js`, `app.js`) compiled into the binary via `concat!` and `include_str!`.

### 🐛 Bug Fixes
- **RF < N Cross-Node Vertex Property Resolution:** Fixed target vertex properties returning `null` in multi-hop pattern queries (`MATCH (a:User)-[:FOLLOWS]->(b:User) RETURN a.name, b.name`) when $RF < N$. Introduced `RemotePropertyResolver` and `POST /storage/vertex_properties` to pre-fetch properties from replica-owning shards on demand.
- **Total Vertices Telemetry Accuracy:** Fixed vertex double-counting in CSR telemetry reporting 19,998 instead of 10,000 for $RF=1$ ingestion. CSR vertex count now prioritizes partition-owned properties (`prop_cnt > 0`) with accurate `source_vertex_count` fallback.
- **DDL Drop & Recreate Property Retention:** Fixed schema ID misalignment where re-created tables (`DROP VERTEX User; DROP EDGES FOLLOWS; CREATE VERTEX Users ...`) failed to return properties due to old schema ID references. Added plural/singular name tolerance (`User` $\leftrightarrow$ `Users`, `FOLLOW` $\leftrightarrow$ `FOLLOWS`) and resilient property table fallbacks.
- **Safari WebKit Canvas Distortion & Layout Jump:** Fixed layout blowup in Safari by adding `min-width: 0; min-height: 0;` on flex parents and switching the Inspector drawer transition from `display: none` to `margin-right: -280px; opacity: 0; pointer-events: none;`. Fixed retina display canvas zoom accumulation by resetting transform before DPR scaling.

---

## [0.5.6] - 2026-10-13

### Distributed MPP & Multi-Hop Traversal (Pure Sharding $RF=1$)
- **Sharded Cross-Node Multi-Hop Traversal:** Enabled seamless 2-hop, 3-hop, and $k$-hop path pattern matching (`MATCH (a)-[:REL]->(b)-[:REL]->(c)...`) across distributed cluster nodes when $RF=1$ (pure sharding, 0% RAM duplication) and arbitrary $RF \ge 1$.
- **Remote Edge Expander (`gdb-planner`):** Introduced `RemoteEdgeExpander` trait in `QueryExecutor` with batch pre-fetching of boundary vertices across shards in `ExpandEdges`, `VarLengthExpand`, and `execute_next_match`.
- **Peer Batch Edge Lookup (`gdb-server`):** Added high-throughput internal endpoint `POST /storage/out_edges` and `ClusterEdgeExpander` with concurrency-safe thread runtime handling and local LRU caching.
- **Cluster Storage Telemetry Aggregation:** `/resources` now queries cluster peers to aggregate cluster totals (`cluster_total_edges`, `cluster_total_vertices`), dynamically calculating the sum across shards when $RF=1$ and cluster max when $RF \ge N$, alongside per-node breakdown metrics.

### Python SDK & Flight Client (`gdb-client 0.5.6`)
- **Replication-Factor-Aware Ingest:** Updated `scatter_ingest_edges` and `scatter_ingest_vertices` to strictly respect cluster `replication_factor`:
  - When $RF=1$, each partition is streamed to exactly one primary shard ($src \pmod N$) with 0% memory duplication across nodes.
  - When $RF > 1$, streams to all $RF$ nodes in the replica set.
- **Topology Helpers:** Added `target_replica_nodes_for_key(key)` to `ClusterTopology`.

### GDB Studio UI (`gdb-studio`)
- **Unity 3D Canvas Viewport Fix:** Fixed semi-transparent HUD overlay panel that blocked mouse controls and obscured the graph by adjusting `.graph-controls` sizing (`fit-content`) and adding `pointer-events: none` to `#unity-hud-bar`.
- **Telemetry UI:** Cluster telemetry in the status bar now renders both total cluster edges/vertices and the local node's sharded partition breakdown.
- **Version Bump:** Updated UI footer to `v0.5.6`.

---

## [0.5.5] - 2026-10-13

### Distributed MPP & Scatter-Gather
- **Cluster-Wide MPP Query Execution:** Automatically scatters openCypher queries (`MATCH`, traversal paths, pattern matching) across all active nodes in the leaderless cluster ring, gathering and merging partial entity streams.
- **Distributed Vector Similarity Search:** `CALL vector.similaritySearch` now queries all cluster shards concurrently, merging Top-$K$ nearest neighbors across nodes with metric-aware ranking (cosine, dot, L2) and vertex deduplication.
- **Distributed Graph Algorithms:** `CALL algo.*` (PageRank, WCC, Louvain, Betweenness, etc.) automatically execute across distributed shards in cluster mode with score aggregation.
- **`local_only` Query Control:** Added optional `local_only: bool` parameter to `/query` and `/batch` payloads to restrict execution to the local node without cluster dispatch when needed.

### Arrow Flight Ingestion Engine (`gdb-flight`)
- **Descriptor Command Parsing:** Extracted Flight descriptor commands from stream initiation (`FlightDescriptor` JSON command and `app_metadata`), correctly binding target vertex labels and edge types.
- **Dynamic Property Type Inference:** Added `infer_properties_from_schema` and `arrow_to_gdb_type` to dynamically map incoming Arrow batch schema types to GDB schema types, preventing empty schemas and unindexed properties during bulk ingestion.
- **Catalog Preservation:** Ingestion preserves existing registered schemas and registers uncataloged properties automatically.

### Python SDK & Flight Client (`gdb-client 0.5.5`)
- **Schema Validation Before Ingestion:** `scatter_ingest_vertices` and `scatter_ingest_edges` now validate vertex labels and edge types against `self.schema()` before streaming, throwing informative `ValueError` if entities are missing from the catalog.
- **Column Name Normalization:** Automatically aliases user-specified column names (`id_col` to `id`, `src_col` to `src`, `dst_col` to `dst`, `rank_col` to `rank`) for seamless Arrow Flight ingest.
- **Bypass Option:** Supported `validate_schema=False` parameter to bypass catalog validation for ultra-high-speed pipelines.

### GDB Studio UI (`gdb-studio`)
- **Left Sidebar Redesign:** Widened sidebar to 330px, removed obsolete Cluster and Schema tab panels, and significantly expanded the Query Templates section into 5 categorized blocks:
  1. Pattern Matching & Paths
  2. Vector Similarity Search
  3. Graph Analytics & Subgraphs
  4. Schema Catalog & DDL
  5. Operations & Batch Scripts
- **Canvas Auto-Clear:** Fixed canvas staying in stale state after deleting or recreating entities; canvas now cleans and redraws immediately on empty responses (`ctx.clearRect()`).
- **Multi-Hop Path Rendering:** Fixed 3-hop and 4-hop path rendering in `extractGraphFromRows`: now chains consecutive projection columns (`row[i] -> row[i+1]`) as continuous paths instead of disconnected fragments.
- **3D Unity Spatial Canvas:** Fixed canvas dimension locking and inactivity by dynamically observing parent container bounding rectangles, adding a demo graph fallback on empty state, and supporting touch events.

### Networking & Configuration
- **Announced Flight Addresses:** Supported `--flight-address` (internal Raft/Flight inter-node gossip) and `--client-flight-address` (external client MPP ingest), resolving behind Docker NAT, Kubernetes services, and load balancers.
- **Documentation & Benchmarks:** Updated `SERVER_AND_QUERY_GUIDE.md` (EN & RU), benchmark scripts, and cluster startup scripts.
