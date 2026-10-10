# GDB Test Suite & Code Coverage Report

This document details the test coverage, integration suites, and reliability guarantees for GDB across all workspace crates.

## Coverage Summary Table

| Subsystem / Crate | Total Lines | Covered Lines | Line Coverage | Function Coverage |
| :--- | :--- | :--- | :--- | :--- |
| **gdb-storage** (Chunked-CSR, MemTable, MVCC, Arrow, Vectors) | 1,058 | 938 | **88.7%** | **94.1%** |
| **gdb-analytics** (Node2Vec, PageRank, Louvain, WCC, SSSP, Triangles) | 730 | 707 | **96.8%** | **100.0%** |
| **gdb-cli** (REPL, Standalone & Online Cluster, ASCII Table) | 1,319 | 1,196 | **90.7%** | **93.5%** |
| **gdb-studio** (Web UI, Run All Batching, Reverse Proxy) | 943 | 841 | **89.2%** | **86.4%** |
| **gdb-s3** (Tiered Parquet Snapshots & Restore) | 355 | 310 | **87.3%** | **62.5%** |
| **gdb-core** (Schema Metadata, Arrow Conversions, Vector Types) | 868 | 767 | **88.4%** | **83.1%** |
| **gdb-wal** (NVMe Write-Ahead Log, Truncate, CRC32 Checksums) | 241 | 198 | **82.2%** | **57.1%** |
| **gdb-raft** (Multi-Raft Consensus, WAL Replay) | 176 | 154 | **87.5%** | **72.7%** |
| **gdb-parser** (openCypher, GQL, VECTOR, Semicolon Splitter) | 2,927 | 2,189 | **74.8%** | **83.3%** |
| **gdb-gpu** (Metal UMA, CUDA Kernels, Out-Of-Core Paging) | 1,503 | 1,229 | **81.8%** | **81.1%** |
| **gdb-flight** (Arrow Flight RPC, MPP Partitioning, Streams) | 683 | 526 | **77.0%** | **78.9%** |
| **gdb-planner** (CBO Optimizer, Node2Vec, Vector Similarity, DML) | 4,175 | 3,171 | **76.0%** | **74.7%** |
| **gdb-server** (REST API, Batch Endpoint, Multi-Statement, Ring) | 1,488 | 981 | **65.9%** | **83.3%** |
| **TOTAL WORKSPACE (Strict Threshold Enforced in CI)** | **18,379** | **14,841** | **80.75%** | **79.85%** |

> *Note: Enforced in GitHub Actions CI via `cargo llvm-cov --workspace --summary-only --fail-under-lines 80` (80.75% lines / 80.84% regions).*


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
