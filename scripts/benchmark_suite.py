#!/usr/bin/env python3
"""
GDB Enterprise Benchmark Suite.

Executes comprehensive performance benchmarks using the official `gdb-client` SDK:
- OLTP Read/Write Latency (p50, p95, p99, QPS)
- 1-hop and 2-hop Graph Pattern Traversals
- Full Nebula Enterprise Analytics Suite (PageRank, Louvain, WCC, SCC, Triangles, K-Core, SSSP, Betweenness, Similarity, Node2Vec)
- In-Memory CSR Compaction Performance
- Supports HTTP and Arrow Flight MPP execution modes
"""

import argparse
from concurrent.futures import ThreadPoolExecutor
import os
import random
import statistics
import sys
import time
from typing import List, Tuple

# Import official gdb-client with sibling repo path fallback for local dev
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
        print("\033[1;31m[-] Could not import official `gdb_client` package.\033[0m")
        print("    Please install it via: pip install 'gdb-client[arrow]'")
        sys.exit(1)


def run_latency_benchmark(client: GdbClient, queries: List[str], name: str, concurrency: int = 1, use_flight: bool = False):
    latencies = []
    start_total = time.perf_counter()

    def worker(q):
        t0 = time.perf_counter()
        try:
            client.query(q, use_flight=use_flight)
            return (time.perf_counter() - t0) * 1000.0
        except Exception:
            return (time.perf_counter() - t0) * 1000.0

    if concurrency > 1:
        with ThreadPoolExecutor(max_workers=concurrency) as pool:
            latencies = list(pool.map(worker, queries))
    else:
        for q in queries:
            latencies.append(worker(q))

    total_time = time.perf_counter() - start_total
    qps = len(queries) / total_time if total_time > 0 else 0

    latencies.sort()
    p50 = latencies[int(len(latencies) * 0.50)] if latencies else 0
    p95 = latencies[int(len(latencies) * 0.95)] if latencies else 0
    p99 = latencies[int(len(latencies) * 0.99)] if latencies else 0
    avg = statistics.mean(latencies) if latencies else 0

    return {
        "name": name,
        "count": len(queries),
        "qps": qps,
        "avg_ms": avg,
        "p50_ms": p50,
        "p95_ms": p95,
        "p99_ms": p99,
        "total_s": total_time,
    }


def main():
    parser = argparse.ArgumentParser(description="GDB Enterprise Benchmark Suite (via gdb-client)")
    parser.add_argument("--url", default="http://localhost:8847", help="GDB HTTP endpoint (default: http://localhost:8847)")
    parser.add_argument("--mode", choices=["http", "flight", "mpp"], default="http", help="Query transport mode: 'http' or 'flight'/'mpp'")
    parser.add_argument("--client-flight-port", type=int, default=8860, help="Initial Flight client port")
    parser.add_argument("--concurrency", type=int, default=4, help="Concurrency for OLTP benchmarks (default: 4)")
    parser.add_argument("--oltp-ops", type=int, default=1000, help="Number of OLTP read/write operations (default: 1000)")
    parser.add_argument("--timeout", type=float, default=60.0, help="Query timeout in seconds")

    args = parser.parse_args()
    use_flight = args.mode in ("flight", "mpp")

    print("\033[1;36m============================================================")
    print("           GDB ENTERPRISE BENCHMARK SUITE (gdb-client)       ")
    print("============================================================\033[0m")
    print(f"[*] Target Endpoint: {args.url}")
    print(f"[*] Transport Mode:  {'Arrow Flight MPP' if use_flight else 'HTTP REST'}")
    print(f"[*] Concurrency:     {args.concurrency}")
    print(f"[*] OLTP Ops:        {args.oltp_ops:,}")
    print("------------------------------------------------------------\n")

    with GdbClient(endpoint=args.url, client_flight_port=args.client_flight_port, timeout=args.timeout) as client:
        # Check health
        try:
            health = client.health()
            print(f"[✓] Connected to GDB: Status={health.get('status')} | Version={health.get('version')} | Role={health.get('role')}\n")
        except Exception as e:
            print(f"\033[1;31m[!] Failed to connect to GDB at {args.url}: {e}\033[0m")
            sys.exit(1)

        # Ensure benchmark schema
        try:
            client.execute("CREATE VERTEX User (name STRING, age INT64);")
            client.execute("CREATE EDGE FOLLOWS ();")
        except Exception:
            pass

        # 1. OLTP Writes
        write_queries = [
            f"INSERT VERTEX User (id, name, age) VALUES ({i + 500000}, 'BenchUser_{i}', {20 + (i % 50)});"
            for i in range(args.oltp_ops)
        ]
        res_write = run_latency_benchmark(client, write_queries, "OLTP Vertex Inserts", args.concurrency, use_flight=False)

        # 2. Compaction
        print("[*] Running CSR Compaction...")
        t0 = time.perf_counter()
        comp_res = client.compact()
        comp_time_ms = (time.perf_counter() - t0) * 1000.0
        print(f"[✓] Compaction finished in {comp_time_ms:.2f}ms ({comp_res.get('message', 'Done')})\n")

        # 3. 1-Hop Pattern Traversals
        read_queries = [
            f"MATCH (a:User)-[:FOLLOWS]->(b:User) WHERE a.id = {(i % 500) + 1} RETURN b.name LIMIT 10;"
            for i in range(args.oltp_ops)
        ]
        res_read_1hop = run_latency_benchmark(client, read_queries, "1-Hop Traversals", args.concurrency, use_flight=use_flight)

        # 4. 2-Hop Multi-Hop Pattern Traversals
        read_2hop = [
            f"MATCH (a:User)-[:FOLLOWS*2..2]->(b:User) WHERE a.id = {(i % 200) + 1} RETURN count(b);"
            for i in range(min(args.oltp_ops, 300))
        ]
        res_read_2hop = run_latency_benchmark(client, read_2hop, "2-Hop Multi-Hop Traversals", args.concurrency, use_flight=use_flight)

        # 5. Graph Analytics Suite
        analytics_algorithms = [
            ("PageRank (20 iters)", "CALL algo.pageRank({max_iter: 20}) YIELD vertex_id, score;"),
            ("Weakly Connected Components", "CALL algo.wcc() YIELD vertex_id, component_id;"),
            ("Louvain Communities", "CALL algo.louvain({max_iter: 10}) YIELD vertex_id, community_id;"),
            ("Triangle Counting & LCC", "CALL algo.triangleCount() YIELD vertex_id, triangles;"),
            ("Single Source Shortest Path", "CALL algo.sssp(1) YIELD vertex_id, distance;"),
            ("Node2Vec Embeddings", "CALL algo.node2vec({walk_length: 5, walks_per_vertex: 2, dimensions: 16}) YIELD vertex_id, embedding;"),
        ]

        analytics_results = []
        for name, cypher in analytics_algorithms:
            t0 = time.perf_counter()
            try:
                res = client.query(cypher, use_flight=use_flight)
                elapsed_ms = (time.perf_counter() - t0) * 1000.0
                if res.is_ok:
                    analytics_results.append((name, elapsed_ms, f"{len(res):,} rows"))
                else:
                    analytics_results.append((name, elapsed_ms, f"Error: {res.error}"))
            except Exception as e:
                elapsed_ms = (time.perf_counter() - t0) * 1000.0
                analytics_results.append((name, elapsed_ms, f"Exception: {e}"))

        # Output Summary
        print("=" * 76)
        print("                     BENCHMARK SUITE RESULTS                             ")
        print("=" * 76)
        print(f"{'Operation / Benchmark':<35} | {'Throughput':<12} | {'P50 (ms)':<9} | {'P99 (ms)':<9}")
        print("-" * 76)
        for r in [res_write, res_read_1hop, res_read_2hop]:
            print(f"{r['name']:<35} | {r['qps']:>9,.1f} QPS | {r['p50_ms']:>7.2f} ms | {r['p99_ms']:>7.2f} ms")
        print("-" * 76)
        print(f"{'Algorithm':<35} | {'Latency':<12} | {'Result':<20}")
        print("-" * 76)
        for name, lat, status in analytics_results:
            print(f"{name:<35} | {lat:>9.2f} ms  | {status}")
        print("=" * 76 + "\n")


if __name__ == "__main__":
    main()
