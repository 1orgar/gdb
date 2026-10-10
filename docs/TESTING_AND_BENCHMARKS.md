# GDB Testing & Performance Benchmarking Guide

Comprehensive guide to executing stress testing, graph traversal benchmarks, GPU hardware acceleration evaluation, data loading, and mathematical validation for the distributed in-memory graph database **GDB**.

---

## 📑 Table of Contents
1. [Overview & Philosophy](#-overview--philosophy)
2. [Automated Schema Lifecycle](#-automated-schema-lifecycle)
3. [Script Catalog & Quick Reference](#-script-catalog--quick-reference)
4. [High-Concurrency Stress Testing (`stress_test.py`)](#-high-concurrency-stress-testing-stress_testpy)
5. [Enterprise Benchmark Suite (`benchmark_suite.py`)](#-enterprise-benchmark-suite-benchmark_suitepy)
6. [Hardware GPU Acceleration Benchmark (`gpu_benchmark.py`)](#-hardware-gpu-acceleration-benchmark-gpu_benchmarkpy)
7. [High-Throughput Synthetic Data Loader (`data_loader.py`)](#-high-throughput-synthetic-data-loader-data_loaderpy)
8. [Mathematical Algorithm Validation (`graph_analytics_validation.py`)](#-mathematical-algorithm-validation-graph_analytics_validationpy)
9. [Leaderless Ring Replication Verification (`test_replication.py`)](#-leaderless-ring-replication-verification-test_replicationpy)
10. [Prometheus Telemetry & Observability Delta](#-prometheus-telemetry--observability-delta)

---

## 🎯 Overview & Philosophy

The GDB testing suite is engineered around three foundational principles:
1. **Powered by Official `gdb-client` SDK**: All test and benchmark scripts are built on the high-performance `gdb-client` Python SDK (`pip install gdb-client[all]`), leveraging zero-copy Arrow Flight MPP scatter-ingest and vectorized Polars DataFrames.
2. **Dual Transport Modes (`--mode {http, flight, mpp}`)**: All scripts allow toggling between HTTP REST and high-throughput Apache Arrow Flight MPP streaming to validate both OLTP and distributed OLAP pipelines.
3. **Deterministic & Isolated Schema Lifecycle**: Scripts recreate clean schemas at startup, seed baseline data if needed, and cleanly drop all test tags and edge types upon exit.
4. **Hardware-Aware Verification**: Native detection and verification of Apple Metal UMA zero-copy memory and NVIDIA CUDA compute context allocation.

---

## 🔄 Automated Schema Lifecycle

In GDB v0.4.1+, the database engine initializes with a **pure clean catalog** (no predefined default labels). To guarantee seamless reproducibility, all test scripts conform to the following lifecycle:

```mermaid
graph TD
    A[Launch Script] --> B[Pre-Cleaning: DROP Old Test Labels & Edges]
    B --> C[Fresh DDL: CREATE Vertex Tags & Edge Types]
    C --> D[Seed Topology: Batch Insert Nodes & Edges]
    D --> E[Compact into Chunked-CSR]
    E --> F[Execute Benchmark / Test Workload]
    F --> G{--keep-schema Flag?}
    G -- No (Default) --> H[Teardown: DROP Test Tags & Edges]
    G -- Yes --> I[Preserve Schema & Entities in Database]
    H --> J[Print Summary & Exit 0]
    I --> J
```

* **Default Behavior**: Every test cleans up after itself. The database returns to its pre-test pristine state.
* **Preserving State**: Pass `--keep-schema` to inspect the generated vertices, edges, and schemas in GDB Studio or CLI after the test.

---

## 📦 Script Catalog & Quick Reference

| Script | Purpose | Workload Type | Schema Targets | Default Port |
| :--- | :--- | :--- | :--- | :--- |
| **[`scripts/stress_test.py`](file:///Users/kirill/Documents/projects/gdb/scripts/stress_test.py)** | Concurrency & Throughput Stress Test | 25% Writes / 75% Reads | `User`, `Device`, `KNOWS`, `LINKED` | `http://127.0.0.1:8847` |
| **[`scripts/benchmark_suite.py`](file:///Users/kirill/Documents/projects/gdb/scripts/benchmark_suite.py)** | Comprehensive 8-Stage Benchmark | 1-hop, 2-hop, 6 Graph Algorithms | `User`, `KNOWS`, `FOLLOWS` | `http://127.0.0.1:8847` |
| **[`scripts/gpu_benchmark.py`](file:///Users/kirill/Documents/projects/gdb/scripts/gpu_benchmark.py)** | GPU Hardware Acceleration Benchmark | High-Volume CSR GPU Offload | `GpuNode`, `GPU_EDGE` | `http://127.0.0.1:8847` |
| **[`scripts/data_loader.py`](file:///Users/kirill/Documents/projects/gdb/scripts/data_loader.py)** | Synthetic Scale-Free Graph Ingest | Bulk OLTP Ingestion & Batch Files | `User`, `FOLLOWS` | `http://127.0.0.1:8847` |
| **[`scripts/graph_analytics_validation.py`](file:///Users/kirill/Documents/projects/gdb/scripts/graph_analytics_validation.py)** | Ground-Truth Math Algorithm Validator | 12 Ground-Truth Graph Topologies | `Node`, `REL` | `http://127.0.0.1:8847` |
| **[`scripts/test_replication.py`](file:///Users/kirill/Documents/projects/gdb/scripts/test_replication.py)** | Symmetric Multi-Peer Ring Replication | Multi-Node Raft Ring Writes | `Device`, `LINKED` | `:8847`, `:8846`, `:8845` |
| **[`scripts/py_client_benchmark.py`](file:///Users/kirill/Documents/projects/gdb/scripts/py_client_benchmark.py)** | Python SDK & Client Ingest Benchmark | High-Speed Polars & Batch Ingest | `BenchUser`, `BENCH_KNOWS` | `http://127.0.0.1:8847` |

---

## ⚡ High-Concurrency Stress Testing (`stress_test.py`)

Generates mixed OLTP write transactions and OLAP read traversals across multiple concurrent worker threads, recording latency percentiles (p50, p90, p95, p99) and error distributions.

### Basic Execution
```bash
# HTTP REST Mode
python3 scripts/stress_test.py --endpoint http://127.0.0.1:8847 --threads 8 --duration 30 --mode http

# Arrow Flight MPP Mode
python3 scripts/stress_test.py --endpoint http://127.0.0.1:8847 --threads 8 --duration 30 --mode flight
```

### CLI Parameters
* `--endpoint`: Target GDB HTTP REST endpoint (default: `http://127.0.0.1:8847`).
* `--mode`: Query transport mode: `http` or `flight`/`mpp` (default: `http`).
* `--threads`: Number of concurrent worker threads (default: `4`).
* `--duration`: Workload duration in seconds (default: `5`).
* `--read-ratio`: Fraction of read queries between `0.0` and `1.0` (default: `0.7` for 70% reads / 30% writes).
* `--keep-schema`: Retain the test schema and data after the test concludes.

### Sample Output
```text
=================================================================
                    PERFORMANCE BENCHMARK RESULTS                
=================================================================
Total Requests:       228,491 (228,491 ok, 0 err)
Elapsed Time:         30.01 s
Throughput (QPS):     7,613.8 queries/sec
-----------------------------------------------------------------
Average Latency:      1.042 ms
Min Latency:          0.142 ms
p50 (Median):         0.985 ms
p90:                  1.481 ms
p95:                  1.650 ms
p99:                  2.031 ms
Max Latency:          14.280 ms
-----------------------------------------------------------------
PROMETHEUS METRICS DELTA (/metrics):
  • Cluster Total Queries: +228,491
  • Edges in Storage:      +57,122
  • MemTable Edges:        57,122
  • CSR Compacted Edges:   0
  • GPU Acceleration:      Active (1)
=================================================================
```

---

## 📊 Enterprise Benchmark Suite (`benchmark_suite.py`)

Executes an 8-stage enterprise benchmark measuring latencies across transactional multi-hop pattern traversals and global graph analytics algorithms:

### Execution
```bash
# HTTP Mode
python3 scripts/benchmark_suite.py --url http://127.0.0.1:8847 --oltp-ops 1000 --concurrency 4 --mode http

# Arrow Flight MPP Mode
python3 scripts/benchmark_suite.py --url http://127.0.0.1:8847 --oltp-ops 1000 --concurrency 4 --mode flight
```

### Measured Stages
1. **1-Hop Traversal**: `MATCH (a:User)-[:FOLLOWS]->(b:User) WHERE a.id = $id RETURN b.name`
2. **2-Hop Traversal**: `MATCH (a:User)-[:FOLLOWS*2..2]->(b:User) WHERE a.id = $id RETURN count(b)`
3. **PageRank Analytics**: 20 iterations with damping factor $0.85$
4. **Louvain Community Detection**: Modularity optimization
5. **Weakly Connected Components (WCC)**: Disjoint set union-find
6. **Triangle Counting & Clustering**: Local clustering coefficients
7. **Single-Source Shortest Path (SSSP)**: Weighted graph path finding
8. **Node2Vec Graph ML Embeddings**: High-dimensional graph representations

---

## 🚀 Hardware GPU Acceleration Benchmark (`gpu_benchmark.py`)

Specifically verifies hardware acceleration kernels on **Apple Metal** (UMA Zero-Copy) and **NVIDIA CUDA** (Linux SpMV Dedicated PCIe).

### Execution
```bash
# Start server with GPU acceleration enabled
./bin/gdb-server --enable-gpu &

# Run GPU Benchmark (HTTP batch ingest)
python3 scripts/gpu_benchmark.py --endpoint http://127.0.0.1:8847 --vertices 10000 --edges 20000 --runs 3 --mode http

# Run GPU Benchmark (Arrow Flight MPP scatter ingest)
python3 scripts/gpu_benchmark.py --endpoint http://127.0.0.1:8847 --vertices 10000 --edges 20000 --runs 3 --mode flight
```

### What It Verifies
1. **`/gpu` Detection**: Validates that hardware acceleration is active and inspects the device name.
2. **Offload Threshold**: Automatically loads vertices and edges exceeding the `--gpu-offload-threshold` (default: 10,000 edges).
3. **CSR Memory Compaction**: Compacts edges into Chunked-CSR so GPU compute shaders can access contiguous memory buffers.
4. **GPU Compute Kernels**: Runs PageRank, WCC, SSSP, Triangle Count, and Louvain, measuring throughput in **edges/second**.
5. **Prometheus Telemetry**: Confirms `gdb_gpu_active = 1`.

### Sample Output
```text
============================================================================
                   GPU ACCELERATION BENCHMARK RESULTS                    
============================================================================
Hardware Backend:   Apple Metal Compute (UMA Zero-Copy)
Graph in CSR:       20,000 compacted edges (10,000 vertices)
Ingest Mode:        Arrow Flight MPP
----------------------------------------------------------------------------
Kernel Algorithm                      | Avg Latency  | Min Latency  | Throughput  
----------------------------------------------------------------------------
Vectorized PageRank (20 iters)        |      5.82 ms  |      3.25 ms  |  3,438,396 e/s
Weakly Connected Components (WCC)     |      1.78 ms  |      1.59 ms  | 11,222,032 e/s
Triangle Counting & Clustering        |      2.47 ms  |      2.41 ms  |  8,087,479 e/s
Single Source Shortest Path (SSSP)    |      1.90 ms  |      1.86 ms  | 10,547,361 e/s
Louvain Community Detection           |      4.95 ms  |      4.81 ms  |  4,043,706 e/s
Multi-Hop Wavefront BFS (1..3)        |      0.66 ms  |      0.60 ms  | 30,489,757 e/s
Parallel Vector Similarity Search     |      0.51 ms  |      0.48 ms  | 39,545,269 e/s
Graph ML Node2Vec Embeddings          |     22.85 ms  |     22.77 ms  |    875,135 e/s
============================================================================
```

---

## 📥 High-Throughput Synthetic Data Loader (`data_loader.py`)

Populates the cluster with realistic social and interaction graphs using scale-free power-law degree distributions (preferential attachment).

```bash
# Ingest 10,000 vertices and 100,000 edges over HTTP Batch
python3 scripts/data_loader.py --url http://127.0.0.1:8847 --vertices 10000 --edges 100000 --mode http

# Ingest via Arrow Flight MPP parallel scatter across cluster nodes
python3 scripts/data_loader.py --url http://127.0.0.1:8847 --vertices 10000 --edges 100000 --mode flight

# Export directly to a high-speed .gdb batch file for gdb-cli
python3 scripts/data_loader.py --file data/social_100k.gdb --vertices 10000 --edges 100000

# Teardown / clean up loaded data
python3 scripts/data_loader.py --url http://127.0.0.1:8847 --teardown
```

---

## 📐 Mathematical Algorithm Validation (`graph_analytics_validation.py`)

Validates the numerical and topological accuracy of graph analytics algorithms against deterministic ground-truth topologies:
- **Triangle Counting**: Validates triangle count on 3-node cycle.
- **PageRank**: Validates convergence on directed cycles and sinks.
- **WCC & SCC**: Validates disjoint component and strongly connected cycle identification.
- **SSSP & Node2Vec**: Validates shortest path distances and dimensional vector embeddings.

```bash
# Validate via HTTP
python3 scripts/graph_analytics_validation.py --endpoint http://127.0.0.1:8847 --mode http

# Validate via Arrow Flight MPP
python3 scripts/graph_analytics_validation.py --endpoint http://127.0.0.1:8847 --mode flight
```

---

## 🔄 Leaderless Ring Replication Verification (`test_replication.py`)

Validates symmetric peer-to-peer data ingestion across a 3-node cluster ring:
1. Verifies that all 3 peers (`:8847`, `:8846`, `:8845`) are UP.
2. Creates schema broadcast from Peer 1.
3. Ingests vertices and edges symmetrically through different nodes (Peer 1, Peer 2, Peer 3).
4. Asserts that data converges identically across all replica sets and that reads return identical counts on every peer.

```bash
./scripts/start_cluster.sh
python3 scripts/test_replication.py --mode http
python3 scripts/test_replication.py --mode flight
```

---

## 🐍 Python SDK Benchmark (`py_client_benchmark.py`)

High-throughput client-side benchmark utilizing the official `gdb-client` package:
1. **Automated Schema Lifecycle**: Recreates `BenchUser` and `BENCH_KNOWS` with clean schema drop upon completion (retained with `--keep-schema`).
2. **Dual-Transport Ingestion**: Measures ingestion throughput utilizing HTTP batch inserts or Arrow Flight MPP scatter-ingest.
3. **Graph Analytics & Traversals**: Evaluates 1-hop and 2-hop traversals, PageRank, and Node2Vec embeddings from Python.
4. **NetworkX / Polars Export**: Verifies zero-copy conversion of query results into Polars DataFrames and NetworkX graphs.

### Dependencies
Dependencies are maintained in `scripts/requirements.txt`:
```bash
pip install -r scripts/requirements.txt
```

### Execution
```bash
# HTTP Batch Mode
python3 scripts/py_client_benchmark.py --endpoint http://127.0.0.1:8847 --vertices 5000 --edges 15000 --mode http

# Arrow Flight MPP Mode
python3 scripts/py_client_benchmark.py --endpoint http://127.0.0.1:8847 --vertices 5000 --edges 15000 --mode flight
```

### CLI Parameters
* `--endpoint`: Target GDB HTTP REST endpoint (default: `http://127.0.0.1:8847`).
* `--mode`: Ingest mode: `http` or `flight`/`mpp` (default: `http`).
* `--client-flight-port`: Flight client port (default: `8860`).
* `--vertices`: Number of synthetic vertices to ingest (default: `20000`).
* `--edges`: Number of synthetic edges to ingest (default: `60000`).
* `--queries`: Number of query iterations (default: `50`).
* `--keep-schema`: Retain the test schema and data after the test concludes.

---

## 📈 Prometheus Telemetry & Observability Delta

All test scripts report metrics delta from the `/metrics` endpoint:
* `gdb_queries_total{status="ok"}` / `gdb_queries_total{status="error"}`: Total query counter.
* `gdb_edges_total`: Total active edges across storage tiers.
* `gdb_memtable_edges_count`: Edges residing in active Delta MemTable.
* `gdb_csr_edges_count`: Edges compacted into read-optimized Chunked-CSR.
* `gdb_compactions_total`: Background compaction executions.
* `gdb_gpu_active`: Hardware accelerator activity flag (`1` for active, `0` for fallback).
