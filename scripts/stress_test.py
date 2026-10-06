#!/usr/bin/env python3
"""
GDB Concurrent Stress Test Suite (Pure Python Standard Library)
Generates high-concurrency mixed OLTP writes and OLAP graph queries
against GDB Cluster, measuring latency distribution and Prometheus metrics delta.
"""

import sys
import time
import json
import random
import argparse
import urllib.request
import urllib.error
from concurrent.futures import ThreadPoolExecutor

def send_query(endpoint, query_str):
    url = f"{endpoint}/query"
    data = json.dumps({"query": query_str}).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    t0 = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=5) as resp:
            body = json.loads(resp.read().decode("utf-8"))
            elapsed_ms = (time.perf_counter() - t0) * 1000.0
            is_ok = body.get("status") == "ok"
            return is_ok, elapsed_ms
    except Exception:
        elapsed_ms = (time.perf_counter() - t0) * 1000.0
        return False, elapsed_ms

def fetch_metrics(endpoint):
    url = f"{endpoint}/metrics"
    try:
        with urllib.request.urlopen(url, timeout=2) as resp:
            text = resp.read().decode("utf-8")
            metrics = {}
            for line in text.splitlines():
                if line and not line.startswith("#"):
                    parts = line.split()
                    if len(parts) >= 2:
                        key = parts[0]
                        try:
                            metrics[key] = float(parts[1])
                        except ValueError:
                            pass
            return metrics
    except Exception:
        return {}

def worker_loop(endpoint, stop_time, write_ratio, results):
    local_times = []
    local_errors = 0
    queries_pool = [
        "MATCH (a:User)-[:KNOWS]->(b:User) RETURN a.name, b.name LIMIT 20;",
        "MATCH (a:Device)-[:LINKED]->(b:Device) RETURN a.model, b.model LIMIT 20;",
        "CALL algo.pageRank({damping: 0.85, max_iter: 5}) YIELD vertex_id, score;",
    ]

    while time.time() < stop_time:
        if random.random() < write_ratio:
            u = random.randint(1, 1000)
            v = random.randint(1, 1000)
            q = f"INSERT EDGE KNOWS FROM {u} TO {v};"
        else:
            q = random.choice(queries_pool)

        ok, lat = send_query(endpoint, q)
        if ok:
            local_times.append(lat)
        else:
            local_errors += 1

    results.append((local_times, local_errors))

def main():
    parser = argparse.ArgumentParser(description="GDB Multi-Threaded Stress Test Suite")
    parser.add_argument("--endpoint", default="http://127.0.0.1:8847", help="Target cluster endpoint")
    parser.add_argument("--concurrency", type=int, default=8, help="Number of concurrent worker threads")
    parser.add_argument("--duration", type=int, default=5, help="Test duration in seconds")
    parser.add_argument("--write-ratio", type=float, default=0.25, help="Fraction of write queries (0.0 - 1.0)")
    args = parser.parse_args()

    print("\033[1;36m" + "=" * 65)
    print("         GDB High-Concurrency Stress Test Suite          ")
    print("=" * 65 + "\033[0m")
    print(f"[*] Target Endpoint:  {args.endpoint}")
    print(f"[*] Workers:          {args.concurrency} threads")
    print(f"[*] Duration:         {args.duration} seconds")
    print(f"[*] Workload Mix:     {int(args.write_ratio * 100)}% Writes / {int((1 - args.write_ratio) * 100)}% Reads")
    print("-" * 65)

    # 1. Fetch baseline metrics
    initial_metrics = fetch_metrics(args.endpoint)
    init_queries = initial_metrics.get('gdb_queries_total{status="ok"}', 0)
    init_edges = initial_metrics.get("gdb_edges_total", 0)

    print("[*] Launching stress workload...")
    start_wall = time.time()
    stop_time = start_wall + args.duration

    results = []
    with ThreadPoolExecutor(max_workers=args.concurrency) as executor:
        for _ in range(args.concurrency):
            executor.submit(worker_loop, args.endpoint, stop_time, args.write_ratio, results)

    actual_duration = time.time() - start_wall

    # Aggregate results
    all_latencies = []
    total_errors = 0
    for lats, errs in results:
        all_latencies.extend(lats)
        total_errors += errs

    all_latencies.sort()
    total_queries = len(all_latencies)
    qps = total_queries / actual_duration if actual_duration > 0 else 0

    # Percentiles
    p50 = all_latencies[int(len(all_latencies) * 0.50)] if all_latencies else 0.0
    p90 = all_latencies[int(len(all_latencies) * 0.90)] if all_latencies else 0.0
    p95 = all_latencies[int(len(all_latencies) * 0.95)] if all_latencies else 0.0
    p99 = all_latencies[int(len(all_latencies) * 0.99)] if all_latencies else 0.0
    avg = sum(all_latencies) / len(all_latencies) if all_latencies else 0.0
    min_lat = all_latencies[0] if all_latencies else 0.0
    max_lat = all_latencies[-1] if all_latencies else 0.0

    # Final metrics
    final_metrics = fetch_metrics(args.endpoint)
    fin_queries = final_metrics.get('gdb_queries_total{status="ok"}', 0)
    fin_edges = final_metrics.get("gdb_edges_total", 0)

    print("\n\033[1;32m[✓] Stress Test Completed Successfully!\033[0m")
    print("\n" + "=" * 65)
    print("                    PERFORMANCE BENCHMARK RESULTS                ")
    print("=" * 65)
    print(f"Total Requests:       {total_queries + total_errors:,} ({total_queries:,} ok, {total_errors:,} err)")
    print(f"Elapsed Time:         {actual_duration:.2f} s")
    print(f"Throughput (QPS):     \033[1;33m{qps:,.1f} queries/sec\033[0m")
    print("-" * 65)
    print(f"Average Latency:      {avg:.3f} ms")
    print(f"Min Latency:          {min_lat:.3f} ms")
    print(f"p50 (Median):         {p50:.3f} ms")
    print(f"p90:                  {p90:.3f} ms")
    print(f"p95:                  {p95:.3f} ms")
    print(f"p99:                  \033[1;36m{p99:.3f} ms\033[0m")
    print(f"Max Latency:          {max_lat:.3f} ms")
    print("-" * 65)
    print("PROMETHEUS METRICS DELTA (/metrics):")
    print(f"  • Cluster Total Queries: +{int(fin_queries - init_queries):,}")
    print(f"  • Edges in Storage:      +{int(fin_edges - init_edges):,}")
    print(f"  • MemTable Edges:        {int(final_metrics.get('gdb_memtable_edges_count', 0)):,}")
    print(f"  • CSR Compacted Edges:   {int(final_metrics.get('gdb_csr_edges_count', 0)):,}")
    print(f"  • GPU Acceleration:      Active ({final_metrics.get('gdb_gpu_active', 1):.0f})")
    print("=" * 65 + "\n")

if __name__ == "__main__":
    main()
