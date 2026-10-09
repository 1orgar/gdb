#!/usr/bin/env python3
"""
GDB Hardware Acceleration & GPU Kernel Benchmark Suite (Pure Python Standard Library).

Validates and benchmarks GPU compute acceleration (Apple Metal UMA / NVIDIA CUDA):
- Probes /gpu and /metrics endpoints
- Recreates fresh GPU test schema (GpuNode, GPU_EDGE)
- Generates high-volume graph topology exceeding the GPU offload threshold (>= 10,000 edges)
- Triggers CSR compaction and executes GPU-accelerated graph algorithms (PageRank, WCC, SSSP, Triangles, Louvain)
- Compares latency, edges/sec processing throughput, and GPU telemetry
- Automatically cleans up and drops test schemas upon completion
"""

import sys
import time
import json
import random
import argparse
import urllib.request
import urllib.error

def send_query(base_url, query_str):
    url = f"{base_url.rstrip('/')}/query"
    data = json.dumps({"query": query_str}).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    t0 = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            body = json.loads(resp.read().decode("utf-8"))
            elapsed_ms = (time.perf_counter() - t0) * 1000.0
            return body, elapsed_ms
    except Exception as e:
        elapsed_ms = (time.perf_counter() - t0) * 1000.0
        return {"status": "error", "error": str(e)}, elapsed_ms

def get_json(url, timeout=5):
    try:
        with urllib.request.urlopen(url, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except Exception:
        return None

def fetch_metrics(endpoint):
    url = f"{endpoint.rstrip('/')}/metrics"
    try:
        with urllib.request.urlopen(url, timeout=3) as resp:
            text = resp.read().decode("utf-8")
            metrics = {}
            for line in text.splitlines():
                if line and not line.startswith("#"):
                    parts = line.split()
                    if len(parts) >= 2:
                        try:
                            metrics[parts[0]] = float(parts[1])
                        except ValueError:
                            pass
            return metrics
    except Exception:
        return {}

def teardown_gpu_schema(endpoint, silent=False):
    if not silent:
        print("[*] Cleaning up GPU test schemas (GpuNode, GPU_EDGE)...")
    send_query(endpoint, "DROP VERTEX GpuNode;")
    send_query(endpoint, "DROP EDGE GPU_EDGE;")

def setup_gpu_schema(endpoint):
    print("[*] Recreating fresh GPU benchmark schema (GpuNode, GPU_EDGE)...")
    teardown_gpu_schema(endpoint, silent=True)
    res_v, _ = send_query(endpoint, "CREATE VERTEX GpuNode (score FLOAT64);")
    res_e, _ = send_query(endpoint, "CREATE EDGE GPU_EDGE ();")
    if res_v.get("status") != "ok" or res_e.get("status") != "ok":
        print(f"\033[1;31m[!] Schema setup error: V={res_v.get('error')} | E={res_e.get('error')}\033[0m")
        sys.exit(1)

def ingest_gpu_graph(endpoint, num_vertices, num_edges):
    print(f"[*] Ingesting {num_vertices:,} vertices and {num_edges:,} edges for GPU offload...")
    t0 = time.time()

    # Batch insert vertices
    batch_size = 500
    for start_id in range(1, num_vertices + 1, batch_size):
        end_id = min(start_id + batch_size, num_vertices + 1)
        vals = ", ".join([f"({vid}, {random.random():.4f})" for vid in range(start_id, end_id)])
        q = f"INSERT VERTEX GpuNode (id, score) VALUES {vals};"
        send_query(endpoint, q)

    # Ingest edges (scale-free power law attachment)
    edge_batch_size = 1000
    edges_created = 0
    while edges_created < num_edges:
        chunk = min(edge_batch_size, num_edges - edges_created)
        items = []
        for _ in range(chunk):
            u = random.randint(1, num_vertices)
            v = random.randint(1, num_vertices)
            items.append(f"({u}, {v})")
        q = f"INSERT EDGE GPU_EDGE VALUES {', '.join(items)};"
        send_query(endpoint, q)
        edges_created += chunk

    elapsed = time.time() - t0
    print(f"\033[1;32m[✓] Graph generated in {elapsed:.2f}s ({num_edges / elapsed:,.0f} edges/sec)\033[0m")

    # Compact into CSR
    print("[*] Triggering Chunked-CSR compaction for zero-copy GPU memory layout...")
    c_res, c_ms = send_query(endpoint, "compact;")
    print(f"\033[1;32m[✓] CSR Compaction finished in {c_ms:.2f} ms ({c_res.get('message', 'ok')})\033[0m\n")

def main():
    parser = argparse.ArgumentParser(description="GDB Hardware Acceleration & GPU Benchmark")
    parser.add_argument("--endpoint", default="http://127.0.0.1:8847", help="GDB HTTP endpoint (default: http://127.0.0.1:8847)")
    parser.add_argument("--vertices", type=int, default=5000, help="Number of vertices to generate (default: 5,000)")
    parser.add_argument("--edges", type=int, default=25000, help="Number of edges to generate (default: 25,000, exceeds 10k GPU threshold)")
    parser.add_argument("--iterations", type=int, default=3, help="Benchmark iterations per algorithm kernel (default: 3)")
    parser.add_argument("--keep-schema", action="store_true", help="Preserve test schemas after benchmark completion")
    args = parser.parse_args()

    print("\033[1;36m" + "=" * 70)
    print("      GDB HARDWARE ACCELERATION & GPU COMPUTE BENCHMARK SUITE          ")
    print("=" * 70 + "\033[0m")
    print(f"[*] Target Endpoint:  {args.endpoint}")
    print(f"[*] Test Graph:       {args.vertices:,} Vertices | {args.edges:,} Edges")
    print(f"[*] Kernel Runs:      {args.iterations} iterations per algorithm")
    print("-" * 70)

    # 1. Health check
    health = get_json(f"{args.endpoint}/health")
    if not health or health.get("status") != "UP":
        print(f"\033[1;31m[!] Failed to connect to GDB at {args.endpoint}\033[0m")
        print("    Please start the server first via: ./bin/gdb-server --enable-gpu")
        sys.exit(1)

    # 2. Inspect GPU Hardware Acceleration
    gpu_info = get_json(f"{args.endpoint}/gpu")
    if not gpu_info:
        print("\033[1;31m[!] Unable to retrieve GPU status from /gpu endpoint\033[0m")
        sys.exit(1)

    backend = gpu_info.get("backend", "Unknown")
    is_active = gpu_info.get("active", False)
    mem_model = gpu_info.get("memory_model", "Unknown")
    threshold = gpu_info.get("threshold_edges", 10000)

    print(f"[*] Compute Backend:   \033[1;33m{backend}\033[0m")
    print(f"[*] GPU Active:        {'✅ YES (Active Hardware Offload)' if is_active else '⚠️ NO (CPU Vectorized Fallback)'}")
    print(f"[*] Memory Model:      {mem_model}")
    print(f"[*] Offload Threshold: {threshold:,} edges")
    if not is_active:
        print("\n\033[1;33m[Notice] Server is running without active GPU acceleration.")
        print("         To enable GPU offload: restart server with '--enable-gpu'\033[0m")
    print("-" * 70 + "\n")

    # 3. Schema Setup & Ingestion
    setup_gpu_schema(args.endpoint)
    ingest_gpu_graph(args.endpoint, args.vertices, args.edges)

    # 4. GPU Graph Analytics Benchmark Suite
    kernels = [
        ("Vectorized PageRank (20 iters)", "CALL algo.pageRank({damping: 0.85, max_iter: 20}) YIELD vertex_id, score;"),
        ("Weakly Connected Components (WCC)", "CALL algo.wcc() YIELD vertex_id, component_id;"),
        ("Triangle Counting & Clustering", "CALL algo.triangleCount() YIELD vertex_id, triangles;"),
        ("Single Source Shortest Path (SSSP)", "CALL algo.sssp({source: 1}) YIELD vertex_id, distance;"),
        ("Louvain Community Detection", "CALL algo.louvain({max_iter: 10}) YIELD vertex_id, community_id;"),
    ]

    print("\033[1;33m[*] Executing GPU-Accelerated Analytics Kernels...\033[0m\n")
    results = []

    for name, query_str in kernels:
        latencies = []
        rows_count = 0
        for _ in range(args.iterations):
            body, lat_ms = send_query(args.endpoint, query_str)
            if body.get("status") == "ok":
                latencies.append(lat_ms)
                rows_count = body.get("num_rows", len(body.get("rows", [])))
            else:
                print(f"  \033[1;31m[!] Kernel Error in {name}: {body.get('error')}\033[0m")
                break

        if latencies:
            avg_ms = sum(latencies) / len(latencies)
            min_ms = min(latencies)
            throughput = (args.edges / (avg_ms / 1000.0)) if avg_ms > 0 else 0
            results.append({
                "name": name,
                "avg_ms": avg_ms,
                "min_ms": min_ms,
                "rows": rows_count,
                "throughput": throughput,
            })
            print(f"  \033[1;32m[✓]\033[0m {name:<36} : \033[1;36m{avg_ms:>7.2f} ms\033[0m (min: {min_ms:.2f} ms | {throughput:>10,.0f} edges/s)")

    # 5. Fetch telemetry metrics
    metrics = fetch_metrics(args.endpoint)
    csr_edges = int(metrics.get("gdb_csr_edges_count", args.edges))
    gpu_active_val = metrics.get("gdb_gpu_active", 1.0 if is_active else 0.0)

    # 6. Performance Summary Table
    print("\n\033[1;36m" + "=" * 76)
    print("                   GPU ACCELERATION BENCHMARK RESULTS                    ")
    print("=" * 76)
    print(f"Hardware Backend:   {backend}")
    print(f"Graph in CSR:       {csr_edges:,} compacted edges ({args.vertices:,} vertices)")
    print(f"GPU Telemetry:      Active Flag = {gpu_active_val:.0f} (Prometheus: gdb_gpu_active)")
    print("-" * 76)
    print(f"{'Kernel Algorithm':<36} | {'Avg Latency':<12} | {'Min Latency':<12} | {'Throughput':<12}")
    print("-" * 76)
    for r in results:
        print(f"{r['name']:<36} | {r['avg_ms']:>8.2f} ms  | {r['min_ms']:>8.2f} ms  | {r['throughput']:>8,.0f} e/s")
    print("=" * 76 + "\033[0m\n")

    # 7. Teardown
    if not args.keep_schema:
        teardown_gpu_schema(args.endpoint)
    else:
        print("[*] Preserving GPU benchmark schema (--keep-schema specified).\n")

if __name__ == "__main__":
    main()
