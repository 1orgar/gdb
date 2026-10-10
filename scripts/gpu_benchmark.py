#!/usr/bin/env python3
"""
GDB Hardware Acceleration & GPU Kernel Benchmark Suite.

Validates and benchmarks GPU compute acceleration (Apple Metal UMA / NVIDIA CUDA):
- Uses official `gdb-client` SDK for cluster communication
- Supports HTTP batch ingestion and Arrow Flight MPP scatter ingestion
- Recreates fresh GPU test schema (GpuNode, GPU_EDGE)
- Generates high-volume graph topology exceeding the GPU offload threshold (>= 10,000 edges)
- Triggers CSR compaction and computes CBO graph statistics
- Executes GPU-accelerated graph algorithms (PageRank, WCC, SSSP, Triangles, Louvain, Multi-Hop Wavefront BFS, Vector Search, Node2Vec)
- Compares latency, edges/sec processing throughput, and GPU telemetry
- Automatically cleans up and drops test schemas upon completion
"""

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import os
import random
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

try:
    import polars as pl
except ImportError:
    pl = None


def teardown_gpu_schema(client: GdbClient, silent: bool = False):
    if not silent:
        print("[*] Cleaning up GPU test schemas (GpuNode, GPU_EDGE)...")
    try:
        client.execute("DROP VERTEX GpuNode;")
    except Exception:
        pass
    try:
        client.execute("DROP EDGE GPU_EDGE;")
    except Exception:
        pass


def setup_gpu_schema(client: GdbClient):
    print("[*] Recreating fresh GPU benchmark schema (GpuNode, GPU_EDGE)...")
    teardown_gpu_schema(client, silent=True)
    try:
        client.execute("CREATE VERTEX GpuNode (score FLOAT64, emb VECTOR(4));")
    except Exception as e:
        print(f"\033[1;31m[!] Schema setup error (Vertex): {e}\033[0m")
        sys.exit(1)
    try:
        client.execute("CREATE EDGE GPU_EDGE ();")
    except Exception as e:
        print(f"\033[1;31m[!] Schema setup error (Edge): {e}\033[0m")
        sys.exit(1)


def ingest_gpu_graph(client: GdbClient, num_vertices: int, num_edges: int, mode: str = "http", workers: int = 4):
    use_flight = mode in ("flight", "mpp")
    print(f"[*] Ingesting {num_vertices:,} vertices and {num_edges:,} edges (mode={mode}, workers={workers})...")
    start_t = time.perf_counter()

    if use_flight:
        if pl is None:
            print("\033[1;31m[!] Polars is required for Arrow Flight mode.\033[0m")
            sys.exit(1)

        # 1. Vertices
        vids = list(range(1, num_vertices + 1))
        scores = [float(i * 1.5) for i in vids]
        embs = [[0.1, 0.2, 0.3, 0.4] for _ in vids]
        df_v = pl.DataFrame({"id": vids, "score": scores, "emb": embs})
        client.scatter_ingest_vertices(df_v, label="GpuNode", id_col="id", max_workers=workers)

        # 2. Edges
        influencer_bound = max(1, num_vertices // 10)
        srcs = [random.randint(1, num_vertices) for _ in range(num_edges)]
        dsts = []
        for s in srcs:
            d = random.randint(1, influencer_bound) if random.random() < 0.6 else random.randint(1, num_vertices)
            while d == s:
                d = random.randint(1, num_vertices)
            dsts.append(d)

        df_e = pl.DataFrame({"src": srcs, "dst": dsts})
        client.scatter_ingest_edges(df_e, edge_type="GPU_EDGE", src_col="src", dst_col="dst", max_workers=workers)
    else:
        # HTTP Batch Mode
        # Vertices in parallel chunks
        v_batch_size = 5000
        v_chunks = [(i, min(i + v_batch_size, num_vertices + 1)) for i in range(1, num_vertices + 1, v_batch_size)]

        def send_v_chunk(start_idx, end_idx):
            rows = []
            for vid in range(start_idx, end_idx):
                rows.append(f"({vid}, 1.5, [0.1, 0.2, 0.3, 0.4])")
            client.execute(f"INSERT VERTEX GpuNode (id, score, emb) VALUES {', '.join(rows)};")

        with ThreadPoolExecutor(max_workers=workers) as pool:
            futures = [pool.submit(send_v_chunk, s, e) for s, e in v_chunks]
            for f in as_completed(futures):
                f.result()

        # Edges in parallel chunks
        e_batch_size = 2000
        influencer_bound = max(1, num_vertices // 10)
        e_chunks = []
        for i in range(0, num_edges, e_batch_size):
            count = min(e_batch_size, num_edges - i)
            e_chunks.append(count)

        def send_e_chunk(count):
            stmts = []
            for _ in range(count):
                src = random.randint(1, num_vertices)
                dst = random.randint(1, influencer_bound) if random.random() < 0.6 else random.randint(1, num_vertices)
                while dst == src:
                    dst = random.randint(1, num_vertices)
                stmts.append(f"INSERT EDGE GPU_EDGE FROM {src} TO {dst};")
            client.execute(" ".join(stmts))

        with ThreadPoolExecutor(max_workers=workers) as pool:
            futures = [pool.submit(send_e_chunk, c) for c in e_chunks]
            for f in as_completed(futures):
                f.result()

    elapsed = time.perf_counter() - start_t
    print(f"\033[1;32m[✓] Graph generated in {elapsed:.2f}s ({num_edges / elapsed:,.0f} edges/sec)\033[0m")


def main():
    parser = argparse.ArgumentParser(description="GDB GPU Acceleration Benchmark Suite")
    parser.add_argument("--endpoint", default="http://127.0.0.1:8847", help="Target server endpoint")
    parser.add_argument("--mode", choices=["http", "flight", "mpp"], default="http", help="Ingest mode: 'http' or 'flight'/'mpp'")
    parser.add_argument("--client-flight-port", type=int, default=8860, help="Initial Flight client port")
    parser.add_argument("--vertices", type=int, default=20000, help="Number of vertices to generate")
    parser.add_argument("--edges", type=int, default=60000, help="Number of edges to generate")
    parser.add_argument("--workers", type=int, default=4, help="Parallel ingest workers (default: 4)")
    parser.add_argument("--runs", type=int, default=3, help="Benchmark iterations per algorithm")
    parser.add_argument("--timeout", type=float, default=60.0, help="HTTP/query timeout in seconds (default: 60.0)")
    parser.add_argument("--keep-schema", action="store_true", help="Preserve test schemas after benchmark")
    args = parser.parse_args()

    print("\033[1;36m" + "=" * 70)
    print("      GDB HARDWARE ACCELERATION & GPU COMPUTE BENCHMARK SUITE          ")
    print("=" * 70 + "\033[0m")
    print(f"[*] Target Endpoint:  {args.endpoint}")
    print(f"[*] Ingest Mode:      {'Arrow Flight MPP' if args.mode in ('flight', 'mpp') else 'HTTP Batch'}")
    print(f"[*] Test Graph:       {args.vertices:,} Vertices | {args.edges:,} Edges")
    print(f"[*] Kernel Runs:      {args.runs} iterations per algorithm")
    print(f"[*] Ingest Workers:   {args.workers}")
    print("----------------------------------------------------------------------")

    with GdbClient(endpoint=args.endpoint, client_flight_port=args.client_flight_port, timeout=args.timeout) as client:
        # Probe GPU status
        try:
            gpu_info = client.gpu()
        except Exception as e:
            print(f"\033[1;31m[!] Failed to connect to GDB at {args.endpoint}: {e}\033[0m")
            print("    Please start the server first via: ./bin/gdb-server --enable-gpu")
            sys.exit(1)

        is_gpu = gpu_info.get("enabled", False)
        backend = gpu_info.get("backend", "Unknown")
        memory_model = gpu_info.get("memory_model", "N/A")
        threshold = gpu_info.get("threshold_edges", 10000)

        print(f"[*] Compute Backend:   {backend}")
        print(f"[*] GPU Active:        {'✅ YES (Active Hardware Offload)' if is_gpu else '❌ NO (CPU Fallback)'}")
        print(f"[*] Memory Model:      {memory_model}")
        print(f"[*] Offload Threshold: {threshold:,} edges")
        print("----------------------------------------------------------------------\n")

        # 1. Setup & Ingestion
        setup_gpu_schema(client)
        ingest_gpu_graph(client, args.vertices, args.edges, mode=args.mode, workers=args.workers)

        # 2. Compaction into CSR
        print("[*] Triggering Chunked-CSR compaction for zero-copy GPU memory layout...")
        comp_t0 = time.perf_counter()
        comp_res = client.compact()
        comp_time = (time.perf_counter() - comp_t0) * 1000.0
        print(f"\033[1;32m[✓] CSR Compaction finished in {comp_time:.2f} ms ({comp_res.get('message', 'Done')})\033[0m")

        # 3. Analyze Graph for CBO
        print("[*] Running ANALYZE GRAPH for Cost-Based Optimizer (CBO)...")
        an_t0 = time.perf_counter()
        an_res = client.analyze()
        an_time = (time.perf_counter() - an_t0) * 1000.0
        an_msg = getattr(an_res, "message", None) or (an_res.get("message", "Done") if isinstance(an_res, dict) else "Done")
        print(f"\033[1;32m[✓] CBO Analyzed in {an_time:.2f} ms ({an_msg})\033[0m\n")

        # 4. Analytics Benchmarking Suite
        benchmarks = [
            ("Vectorized PageRank (20 iters)", "CALL algo.pageRank({max_iter: 20}) YIELD vertex_id, score;"),
            ("Weakly Connected Components (WCC)", "CALL algo.wcc() YIELD vertex_id, component_id;"),
            ("Triangle Counting & Clustering", "CALL algo.triangleCount() YIELD vertex_id, triangles;"),
            ("Single Source Shortest Path (SSSP)", "CALL algo.sssp(1) YIELD vertex_id, distance;"),
            ("Louvain Community Detection", "CALL algo.louvain({max_iter: 10}) YIELD vertex_id, community_id;"),
            ("Multi-Hop Wavefront BFS (1..3)", "MATCH (a:GpuNode)-[:GPU_EDGE*1..3]->(b:GpuNode) WHERE a.id = 1 RETURN count(b);"),
            ("Parallel Vector Similarity Search", "CALL vector.similaritySearch('GpuNode', 'emb', [0.1, 0.2, 0.3, 0.4], 10, 'cosine') YIELD vertex_id, score;"),
            ("Graph ML Node2Vec Embeddings", "CALL algo.node2vec({walk_length: 5, walks_per_vertex: 2, dimensions: 16}) YIELD vertex_id, embedding;"),
        ]

        print("[*] Executing GPU-Accelerated Analytics Kernels...\n")
        results = []

        for name, query_str in benchmarks:
            latencies = []
            err_msg = None

            for run_i in range(args.runs):
                t0 = time.perf_counter()
                try:
                    res = client.query(query_str)
                    elapsed = (time.perf_counter() - t0) * 1000.0
                    if res.is_ok:
                        latencies.append(elapsed)
                    else:
                        err_msg = res.error
                        break
                except Exception as e:
                    err_msg = str(e)
                    break

            if latencies:
                avg_lat = sum(latencies) / len(latencies)
                min_lat = min(latencies)
                throughput = (args.edges / (avg_lat / 1000.0)) if avg_lat > 0 else 0
                results.append((name, avg_lat, min_lat, throughput))
                print(f"  \033[1;32m[✓]\033[0m {name:<36} : {avg_lat:7.2f} ms (min: {min_lat:6.2f} ms | {throughput:>10,.0f} edges/s)")
            else:
                print(f"  \033[1;31m[!]\033[0m Kernel Error in {name}: {err_msg}")

        # 5. Teardown
        if not args.keep_schema:
            teardown_gpu_schema(client)

        print("\n" + "=" * 76)
        print("                   GPU ACCELERATION BENCHMARK RESULTS                    ")
        print("=" * 76)
        print(f"Hardware Backend:   {backend}")
        print(f"Graph in CSR:       {args.edges:,} compacted edges ({args.vertices:,} vertices)")
        print(f"Ingest Mode:        {'Arrow Flight MPP' if args.mode in ('flight', 'mpp') else 'HTTP Batch'}")
        print("-" * 76)
        print(f"{'Kernel Algorithm':<37} | {'Avg Latency':<12} | {'Min Latency':<12} | {'Throughput':<12}")
        print("-" * 76)
        for name, avg_l, min_l, tp in results:
            print(f"{name:<37} | {avg_l:9.2f} ms  | {min_l:9.2f} ms  | {tp:>10,.0f} e/s")
        print("=" * 76 + "\n")


if __name__ == "__main__":
    main()
