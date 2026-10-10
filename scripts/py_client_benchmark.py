#!/usr/bin/env python3
"""
GDB Python Client Benchmark Suite
==================================
Comprehensive performance, throughput, and latency percentile benchmark
evaluating the official `gdb-client` SDK, Polars batch ingest, multi-statement DML,
vector similarity search, and Graph ML algorithms against a running GDB cluster.

Usage:
    python3 scripts/py_client_benchmark.py [--endpoint http://localhost:8847] [--vertices 2000] [--edges 5000] [--keep-schema]
"""

import argparse
import os
import sys
import time
from typing import List, Tuple

# Import official gdb_client, with fallback to sibling repo for local dev
try:
    from gdb_client import GdbClient
except ImportError:
    current_dir = os.path.dirname(os.path.abspath(__file__))
    sibling_sdk = os.path.abspath(os.path.join(os.path.dirname(current_dir), "..", "gdb-py-client", "src"))
    if os.path.exists(sibling_sdk):
        sys.path.insert(0, sibling_sdk)
    try:
        from gdb_client import GdbClient
    except ImportError:
        print("[-] Could not load official `gdb_client` package.")
        print("    Please install it via: pip install gdb-client")
        print("    Or install locally: pip install ../gdb-py-client")
        sys.exit(1)

try:
    import polars as pl
except ImportError:
    pl = None

try:
    import numpy as np
except ImportError:
    np = None


def print_banner():
    print("=" * 70)
    print("           GDB PYTHON SDK & BATCH CLIENT BENCHMARK               ")
    print("=" * 70)


def setup_benchmark_schema(client: GdbClient):
    print("[*] Initializing benchmark schema via multi-statement script...")
    setup_script = """
        CREATE VERTEX PyBenchNode (name STRING, score FLOAT64, emb VECTOR(4));
        CREATE EDGE PY_LINK ();
    """
    try:
        client.execute_script(setup_script)
        print("[✓] Schema registered successfully.")
    except Exception as e:
        print(f"[*] Schema initialization note: {e}")


def teardown_benchmark_schema(client: GdbClient):
    print("[*] Cleaning up benchmark schema...")
    try:
        client.execute("DROP EDGE PY_LINK;")
    except Exception:
        pass
    try:
        client.execute("DROP VERTEX PyBenchNode;")
    except Exception:
        pass
    print("[✓] Benchmark schema dropped.")


def benchmark_vertex_ingest(client: GdbClient, count: int, batch_size: int = 500) -> Tuple[float, float]:
    print(f"\n[1/5] Benchmarking Vertex Batch Ingest ({count} vertices, batch_size={batch_size})...")
    
    # Generate data
    records = []
    for i in range(1, count + 1):
        emb = [float(i % 10) / 10.0, float((i * 2) % 10) / 10.0, 0.5, 0.1]
        records.append({
            "id": i,
            "name": f"node_{i}",
            "score": float(i * 1.5),
            "emb": emb,
        })

    # Test via Polars if available
    start = time.perf_counter()
    if pl is not None:
        df = pl.DataFrame(records)
        inserted = client.insert_vertices("PyBenchNode", df, batch_size=batch_size)
    else:
        inserted = client.insert_vertices("PyBenchNode", records, batch_size=batch_size)
    elapsed = time.perf_counter() - start
    
    throughput = count / elapsed if elapsed > 0 else 0
    print(f"      Inserted: {inserted} vertices in {elapsed:.3f}s ({throughput:,.1f} vertices/sec)")
    return elapsed, throughput


def benchmark_edge_ingest(client: GdbClient, count: int, total_vertices: int, batch_size: int = 500) -> Tuple[float, float]:
    print(f"\n[2/5] Benchmarking Edge Batch Ingest ({count} edges, batch_size={batch_size})...")
    edges = []
    for i in range(count):
        src = (i % total_vertices) + 1
        dst = ((i * 7 + 1) % total_vertices) + 1
        edges.append((src, dst))

    start = time.perf_counter()
    inserted = client.insert_edges("PY_LINK", edges, batch_size=batch_size)
    elapsed = time.perf_counter() - start
    throughput = count / elapsed if elapsed > 0 else 0
    print(f"      Inserted: {inserted} edges in {elapsed:.3f}s ({throughput:,.1f} edges/sec)")
    return elapsed, throughput


def benchmark_queries(client: GdbClient, iterations: int = 500) -> dict:
    print(f"\n[3/5] Benchmarking Traversal Latencies ({iterations} iterations)...")
    latencies_ms = []

    for i in range(1, iterations + 1):
        vid = (i % 100) + 1
        t0 = time.perf_counter()
        client.query(f"MATCH (a:PyBenchNode)-[:PY_LINK]->(b:PyBenchNode) WHERE a.id = {vid} RETURN b.name LIMIT 10;")
        t1 = time.perf_counter()
        latencies_ms.append((t1 - t0) * 1000.0)

    latencies_ms.sort()
    p50 = latencies_ms[int(len(latencies_ms) * 0.50)]
    p95 = latencies_ms[int(len(latencies_ms) * 0.95)]
    p99 = latencies_ms[int(len(latencies_ms) * 0.99)]
    avg_lat = sum(latencies_ms) / len(latencies_ms)
    qps = iterations / (sum(latencies_ms) / 1000.0) if sum(latencies_ms) > 0 else 0

    print(f"      Avg: {avg_lat:.2f}ms | P50: {p50:.2f}ms | P95: {p95:.2f}ms | P99: {p99:.2f}ms | QPS: {qps:,.1f}")
    return {"p50": p50, "p95": p95, "p99": p99, "avg": avg_lat, "qps": qps}


def benchmark_vector_similarity(client: GdbClient, iterations: int = 50) -> float:
    print(f"\n[4/5] Benchmarking Vector Similarity Search ({iterations} queries)...")
    t0 = time.perf_counter()
    for _ in range(iterations):
        client.query("CALL vector.similaritySearch('PyBenchNode', 'emb', [0.5, 0.5, 0.5, 0.1], 5, 'cosine') YIELD vertex_id, score;")
    elapsed = time.perf_counter() - t0
    qps = iterations / elapsed if elapsed > 0 else 0
    print(f"      Executed {iterations} vector similarity queries in {elapsed:.3f}s ({qps:,.1f} QPS)")
    return qps


def benchmark_cbo_and_analytics(client: GdbClient):
    print("\n[5/5] Benchmarking CBO Graph Analysis & In-DB Graph ML...")
    # 1. Analyze Graph
    t0 = time.perf_counter()
    res = client.analyze()
    t_cbo = time.perf_counter() - t0
    print(f"      [✓] ANALYZE GRAPH completed in {t_cbo * 1000:.2f}ms: {res.message}")

    # 2. PageRank
    t0 = time.perf_counter()
    pr = client.query("CALL algo.pageRank({damping: 0.85, max_iterations: 10}) YIELD vertex_id, score;")
    t_pr = time.perf_counter() - t0
    print(f"      [✓] PageRank (10 iter) completed in {t_pr * 1000:.2f}ms ({len(pr)} vertices scored)")

    # 3. Node2Vec
    t0 = time.perf_counter()
    n2v = client.query("CALL algo.node2vec({walk_length: 5, walks_per_vertex: 2, dimensions: 16}) YIELD vertex_id, embedding;")
    t_n2v = time.perf_counter() - t0
    print(f"      [✓] Node2Vec embeddings generated in {t_n2v * 1000:.2f}ms ({len(n2v)} embeddings)")


def main():
    parser = argparse.ArgumentParser(description="GDB Python SDK Client Benchmark")
    parser.add_argument("--endpoint", default="http://localhost:8847", help="GDB HTTP endpoint")
    parser.add_argument("--vertices", type=int, default=2000, help="Number of vertices to benchmark")
    parser.add_argument("--edges", type=int, default=5000, help="Number of edges to benchmark")
    parser.add_argument("--queries", type=int, default=200, help="Number of query iterations")
    parser.add_argument("--keep-schema", action="store_true", help="Keep benchmark schema after completion")
    args = parser.parse_args()

    print_banner()

    client = GdbClient(endpoint=args.endpoint)

    try:
        health = client.health()
        print(f"[+] Cluster Status: {health.get('status')} | Version: {health.get('version')} | Node: #{health.get('node_id')}")
    except Exception as e:
        print(f"[-] Could not connect to GDB cluster at {args.endpoint}: {e}")
        print("    Please start the cluster: ./scripts/start_cluster.sh")
        sys.exit(1)

    gpu = client.gpu()
    print(f"[+] Hardware Accelerator: {gpu.get('backend', 'CPU Fallback')} (Active: {gpu.get('enabled', False)})")

    # Schema setup (clean recreate)
    teardown_benchmark_schema(client)
    setup_benchmark_schema(client)

    try:
        v_elapsed, v_qps = benchmark_vertex_ingest(client, args.vertices)
        e_elapsed, e_qps = benchmark_edge_ingest(client, args.edges, args.vertices)

        client.compact()
        print("[✓] Chunked-CSR Compaction triggered.")

        q_stats = benchmark_queries(client, args.queries)
        vec_qps = benchmark_vector_similarity(client, 50)
        benchmark_cbo_and_analytics(client)

        print("\n" + "=" * 70)
        print("                       BENCHMARK SUMMARY                          ")
        print("=" * 70)
        print(f"  Vertex Batch Throughput:    {v_qps:,.1f} vertices/sec")
        print(f"  Edge Batch Throughput:      {e_qps:,.1f} edges/sec")
        print(f"  Traversal Latency (P50):    {q_stats['p50']:.2f} ms")
        print(f"  Traversal Latency (P95):    {q_stats['p95']:.2f} ms")
        print(f"  Traversal Latency (P99):    {q_stats['p99']:.2f} ms")
        print(f"  Traversal Query QPS:        {q_stats['qps']:,.1f} queries/sec")
        print(f"  Vector Similarity QPS:      {vec_qps:,.1f} queries/sec")
        print("=" * 70)
        print("\x1b[1;32m[✓] Python SDK Benchmark completed successfully.\x1b[0m\n")

    finally:
        if not args.keep_schema:
            teardown_benchmark_schema(client)
        else:
            print("[*] --keep-schema specified: benchmark schema preserved.")
        client.close()


if __name__ == "__main__":
    main()
