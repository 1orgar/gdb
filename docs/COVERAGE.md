# GDB Test Suite & Code Coverage Report

This document details the test coverage, integration suites, and reliability guarantees for GDB across all workspace crates.

## Coverage Summary Table

| Subsystem / Crate | Total Lines | Covered Lines | Line Coverage | Function Coverage |
| :--- | :--- | :--- | :--- | :--- |
| **gdb-storage** (Chunked-CSR, MemTable, MVCC, Indices) | 713 | 660 | **92.6%** | **95.2%** |
| **gdb-analytics** (WCC, PageRank, SSSP, Triangles, K-Core, Louvain, LPA, Centrality) | 730 | 707 | **96.8%** | **100.0%** |
| **gdb-planner** (CBO Physical Plans, `WITH` Pipelines, DDL/DML Executors) | 1,749 | 1,403 | **80.2%** | **77.8%** |
| **gdb-parser** (openCypher, GQL, DDL AST & Tokenizer) | 1,371 | 1,129 | **82.3%** | **87.2%** |
| **gdb-studio** (Web UI, Interactive Graph Visualization, Reverse Proxy) | 511 | 447 | **87.5%** | **86.0%** |
| **gdb-s3** (Tiered Parquet Snapshots & Restore) | 156 | 143 | **91.7%** | **66.7%** |
| **gdb-core** (Schema Metadata, Arrow Conversions, Types) | 385 | 345 | **89.6%** | **83.1%** |
| **gdb-raft** (Multi-Raft Consensus, Wal Replay) | 103 | 95 | **92.2%** | **72.7%** |
| **gdb-wal** (NVMe Write-Ahead Log, CRC32 Checksums) | 105 | 80 | **76.2%** | **76.2%** |
| **gdb-flight** (Arrow Flight RPC, MPP Partitioning, Streams) | 354 | 266 | **75.1%** | **77.6%** |
| **gdb-cli** (Interactive REPL, Standalone Mode, Formatter) | 572 | 419 | **73.3%** | **74.1%** |
| **gdb-gpu** (Metal & CUDA Compute Kernels, Offloader) | 595 | 429 | **72.1%** | **76.7%** |
| **gdb-server** (REST API, Ring Sharding, Observability) | 755 | 464 | **61.5%** | **80.4%** |
| **TOTAL WORKSPACE (Strict Threshold Enforced in CI)** | **8,253** | **6,734** | **81.59%** | **78.79%** |

> *Note: Enforced in GitHub Actions CI via `cargo llvm-cov --workspace --summary-only --fail-under-lines 80`.*


## Integration & E2E Test Suites

1. **`crates/gdb-server/tests/e2e_http_server.rs`**
   - Clean catalog initialization (zero default vertex tags or edge types).
   - Dynamic DDL lifecycle (`CREATE VERTEX`, `CREATE EDGE`, `ALTER VERTEX ADD/DROP`, `CREATE INDEX`, `DROP INDEX`, `DROP VERTEX`, `DROP EDGE`).
   - openCypher `WITH` operator chaining with intermediate aggregations and filtering.
   - Graph algorithms via `CALL algo.<name>()` (PageRank, WCC).
   - Online storage compaction via `POST /compact`.
   - Cluster topology inspection via `GET /cluster` and `GET /resources`.
   - Prometheus metrics validation via `GET /metrics`.

2. **`crates/gdb-flight/tests/flight_integration.rs`**
   - Streaming Arrow RecordBatch ingestion into DeltaMemTable via `DoPut`.
   - Ticket-based openCypher query streaming via `DoGet`.
   - MPP `ShufflePartitioner` verifying hash-distribution of RecordBatches across 2, 4, and 8 partitions.
   - gRPC Handshake authentication protocol verification.

3. **`crates/gdb-planner/tests/planner_e2e.rs`**
   - Multi-hop variable length graph traversal (`*1..2`, `*1..4`).
   - Complex `WITH` grouping, aggregation (`count()`, `avg()`), filtering, and ordering.
   - Cost-based physical plan tree generation via `EXPLAIN` queries.
   - Secondary index lookups vs full vertex scans.

4. **`crates/gdb-storage/tests/storage_lifecycle.rs`**
   - High-volume MemTable insertion and compaction into Chunked-CSR.
   - MVCC snapshot isolation and commit version resolution.
   - Cascade vertex deletions removing vertices and all incident edges.
   - Multi-value secondary index point lookups and property mutation tracking.

## Running Coverage Locally

```bash
# Run quick text summary
./scripts/coverage.sh

# Generate interactive HTML coverage report
cargo llvm-cov --workspace --html --open

# Generate LCOV file for CI/CD integrations
cargo llvm-cov --workspace --lcov --output-path lcov.info
```
