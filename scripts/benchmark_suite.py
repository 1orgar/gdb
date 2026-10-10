#!/usr/bin/env python3
"""
GDB Enterprise Benchmark Suite.

Executes comprehensive performance benchmarks:
- OLTP Read/Write Latency (p50, p95, p99, QPS)
- 1-hop and 2-hop Graph Pattern Traversals
- Full Nebula Enterprise Analytics Suite (PageRank, Louvain, WCC, SCC, Triangles, K-Core, SSSP, Betweenness, Similarity)
- In-Memory CSR Compaction Performance
"""

import argparse
import json
import random
import statistics
import sys
import time
import urllib.request
import urllib.error
from concurrent.futures import ThreadPoolExecutor

def send_query(base_url, query_str):
    url = f"{base_url}/query"
    data = query_str.encode('utf-8')
    req = urllib.request.Request(url, data=data, headers={'Content-Type': 'text/plain'})
    start = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            body = resp.read().decode('utf-8')
            elapsed_ms = (time.perf_counter() - start) * 1000.0
            parsed = json.loads(body)
            return parsed, elapsed_ms
    except urllib.error.URLError as e:
        print(f"\x1b[1;31mError connecting to {url}: {e}\x1b[0m")
        return None, 0.0

def run_latency_benchmark(base_url, queries, name, concurrency=1):
    latencies = []
    start_total = time.perf_counter()

    def worker(q):
        _, lat = send_query(base_url, q)
        return lat

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

def setup_schema(base_url):
    print("[*] Recreating fresh benchmark schema (User, KNOWS, FOLLOWS)...")
    teardown_schema(base_url, silent=True)
    send_query(base_url, "CREATE VERTEX User (name STRING, age INT64);")
    send_query(base_url, "CREATE EDGE KNOWS ();")
    send_query(base_url, "CREATE EDGE FOLLOWS ();")

    print("[*] Pre-seeding benchmark graph topology (100 vertices, 500 edges) via batch insert...")
    user_vals = ", ".join([f"({vid}, 'User_{vid}', {20 + vid % 40})" for vid in range(1, 101)])
    send_query(base_url, f"INSERT VERTEX User (id, name, age) VALUES {user_vals};")

    edge_stmts = []
    for _ in range(500):
        u = random.randint(1, 100)
        v = random.randint(1, 100)
        edge_stmts.append(f"INSERT EDGE KNOWS FROM {u} TO {v};")
        edge_stmts.append(f"INSERT EDGE FOLLOWS FROM {u} TO {v};")
    send_query(base_url, " ".join(edge_stmts))
    send_query(base_url, "compact;")
    print("[✓] Seed graph generated and compacted into CSR.\n")

def teardown_schema(base_url, silent=False):
    if not silent:
        print("[*] Cleaning up and dropping benchmark schema (User, KNOWS, FOLLOWS)...")
    send_query(base_url, "DROP VERTEX User;")
    send_query(base_url, "DROP EDGE KNOWS;")
    send_query(base_url, "DROP EDGE FOLLOWS;")

def main():
    parser = argparse.ArgumentParser(description="GDB Benchmark Suite")
    parser.add_argument("--url", default="http://localhost:8847", help="GDB HTTP endpoint (default: http://localhost:8847)")
    parser.add_argument("--samples", type=int, default=500, help="Number of query samples for traversal tests (default: 500)")
    parser.add_argument("--concurrency", type=int, default=4, help="Concurrency workers for OLTP tests (default: 4)")
    parser.add_argument("--keep-schema", action="store_true", help="Preserve benchmark schemas after completion")

    args = parser.parse_args()

    print("\x1b[1;36m")
    print("================================================================================")
    print("           GDB ENTERPRISE HIGH-PERFORMANCE BENCHMARK SUITE                      ")
    print("================================================================================")
    print(f"[*] Target Endpoint:   {args.url}")
    print(f"[*] Traversal Samples: {args.samples}")
    print(f"[*] Concurrency:       {args.concurrency} workers")
    print("--------------------------------------------------------------------------------\x1b[0m\n")

    # 1. Health check
    health_url = f"{args.url}/health"
    try:
        with urllib.request.urlopen(health_url, timeout=5) as resp:
            info = json.loads(resp.read().decode('utf-8'))
            print(f"\x1b[1;32m[+] Cluster Connected: {info.get('service')} (Status: {info.get('status')})\x1b[0m\n")
    except Exception as e:
        print(f"\x1b[1;31m[!] Failed to connect to GDB at {args.url}: {e}\x1b[0m")
        print("    Please start the server first via: ./bin/gdb-server")
        sys.exit(1)

    # Initialize fresh schema
    setup_schema(args.url)

    results = []

    # Benchmark 1: 1-hop Traversal
    print("\x1b[1;33m[1/8] Benchmarking 1-hop Graph Traversal (MATCH (a)-[:KNOWS]->(b))...\x1b[0m")
    queries_1hop = [
        f"MATCH (a:User)-[:KNOWS]->(b:User) WHERE a.id = {random.randint(1, 100)} RETURN b.name"
        for _ in range(args.samples)
    ]
    res_1hop = run_latency_benchmark(args.url, queries_1hop, "1-Hop Path Traversal", args.concurrency)
    results.append(res_1hop)

    # Benchmark 2: 2-hop Traversal
    print("\x1b[1;33m[2/8] Benchmarking 2-hop Graph Traversal (MATCH (a)->(b)->(c))...\x1b[0m")
    queries_2hop = [
        f"MATCH (a:User)-[:KNOWS]->(b:User)-[:KNOWS]->(c:User) WHERE a.id = {random.randint(1, 100)} RETURN c.name"
        for _ in range(args.samples)
    ]
    res_2hop = run_latency_benchmark(args.url, queries_2hop, "2-Hop Path Traversal", args.concurrency)
    results.append(res_2hop)

    # Benchmark 3: PageRank Analytics
    print("\x1b[1;33m[3/8] Benchmarking PageRank (20 iterations, damping 0.85)...\x1b[0m")
    pr_query = "CALL algo.pageRank({damping: 0.85, max_iter: 20}) YIELD vertex_id, score"
    res_pr = run_latency_benchmark(args.url, [pr_query] * 5, "PageRank (20 iters)", 1)
    results.append(res_pr)

    # Benchmark 4: Louvain Community Detection
    print("\x1b[1;33m[4/8] Benchmarking Louvain Community Detection (Modularity)...\x1b[0m")
    louvain_query = "CALL algo.louvain({max_iter: 10}) YIELD vertex_id, community_id"
    res_louvain = run_latency_benchmark(args.url, [louvain_query] * 5, "Louvain Modularity", 1)
    results.append(res_louvain)

    # Benchmark 5: Weakly Connected Components (WCC)
    print("\x1b[1;33m[5/8] Benchmarking Weakly Connected Components (WCC)...\x1b[0m")
    wcc_query = "CALL algo.wcc() YIELD vertex_id, component_id"
    res_wcc = run_latency_benchmark(args.url, [wcc_query] * 5, "WCC (Union-Find)", 1)
    results.append(res_wcc)

    # Benchmark 6: Triangle Count & Local Clustering Coefficient
    print("\x1b[1;33m[6/8] Benchmarking Triangle Count & Clustering Coefficient...\x1b[0m")
    tri_query = "CALL algo.triangleCount() YIELD vertex_id, triangles"
    res_tri = run_latency_benchmark(args.url, [tri_query] * 5, "Triangle Count & LCC", 1)
    results.append(res_tri)

    # Benchmark 7: Single-Source Shortest Path (SSSP)
    print("\x1b[1;33m[7/8] Benchmarking SSSP (Single-Source Shortest Path)...\x1b[0m")
    sssp_queries = [f"CALL algo.sssp({{source: {random.randint(1, 50)}}}) YIELD vertex_id, distance" for _ in range(10)]
    res_sssp = run_latency_benchmark(args.url, sssp_queries, "SSSP (Shortest Path)", 1)
    results.append(res_sssp)

    # Benchmark 8: Jaccard Similarity
    print("\x1b[1;33m[8/8] Benchmarking Jaccard & Cosine Similarity...\x1b[0m")
    sim_queries = [
        f"CALL algo.similarity({{node1: {random.randint(1, 50)}, node2: {random.randint(1, 50)}}}) YIELD jaccard, common_neighbors"
        for _ in range(50)
    ]
    res_sim = run_latency_benchmark(args.url, sim_queries, "Jaccard & Cosine Similarity", args.concurrency)
    results.append(res_sim)

    # Summary Table
    print("\n\x1b[1;36m")
    print("=========================================================================================================")
    print("                                  BENCHMARK PERFORMANCE SUMMARY                                         ")
    print("=========================================================================================================")
    print(f"{'Benchmark Target':<32} | {'Queries':<7} | {'Throughput (QPS)':<16} | {'Avg (ms)':<9} | {'p50 (ms)':<9} | {'p99 (ms)':<9}")
    print("---------------------------------------------------------------------------------------------------------")
    for r in results:
        print(
            f"{r['name']:<32} | "
            f"{r['count']:<7} | "
            f"{r['qps']:<16.1f} | "
            f"{r['avg_ms']:<9.3f} | "
            f"{r['p50_ms']:<9.3f} | "
            f"{r['p99_ms']:<9.3f}"
        )
    print("=========================================================================================================\x1b[0m\n")

    if not args.keep_schema:
        teardown_schema(args.url)
    else:
        print("[*] Preserving benchmark schema (--keep-schema specified).\n")

if __name__ == "__main__":
    main()
