# Changelog

All notable changes to **GDB** (Distributed High-Performance In-Memory Graph Database) and **gdb-client** will be documented in this file.

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
