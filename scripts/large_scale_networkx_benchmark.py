#!/usr/bin/env python3
"""
GDB vs Rustworkx (NetworkX Rust) vs NetworkX (Python) Large-Scale Benchmark Suite (>1M Vertices, >2M Edges).

Benchmarks:
1. Single-Node GDB (--enable-gpu, Apple Metal UMA + Rayon)
2. Multi-Node 3-Shard GDB Cluster (N=3, RF=1, Sharded RF < N)
3. Rustworkx 0.17.1 (Rust-backed Python Graph Library / "NetworkX Rust")
4. NetworkX 3.2.1 (Pure Python Reference)

Dataset:
- 1,050,000 Vertices
- 2,100,000 Directed Edges
"""

import argparse
import json
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from typing import Dict, List, Tuple

import networkx as nx
import pyarrow as pa
import pyarrow.flight as flight
import requests
import rustworkx as rx


def generate_edges_arrays(num_vertices: int) -> Tuple[List[int], List[int], List[int]]:
    """
    Generates 1.05M vertex IDs and 2.1M directed edges with planted triangles and small-world shortcuts.
    """
    print(f"[*] Generating {num_vertices:,} vertices and {num_vertices * 2:,} directed edges...")
    t0 = time.perf_counter()
    vids = list(range(1, num_vertices + 1))
    srcs = [0] * (num_vertices * 2)
    dsts = [0] * (num_vertices * 2)

    idx = 0
    for u in range(1, num_vertices + 1):
        block_base = ((u - 1) // 25) * 25 + 1
        local_offset = (u - 1) % 25
        v1 = block_base + ((local_offset + 1) % 25)
        if local_offset % 3 != 0:
            v2 = block_base + ((local_offset + 2) % 25)
        else:
            v2 = ((u * 2654435761) % num_vertices) + 1
            if v2 == u or v2 == v1:
                v2 = ((u + 25) % num_vertices) + 1
        srcs[idx] = u
        dsts[idx] = v1
        srcs[idx + 1] = u
        dsts[idx + 1] = v2
        idx += 2

    print(f"[✓] Generated dataset arrays in {time.perf_counter() - t0:.2f} s")
    return vids, srcs, dsts


def flight_ingest_single_node(
    http_url: str,
    flight_port: int,
    vids: List[int],
    srcs: List[int],
    dsts: List[int],
) -> float:
    t0 = time.perf_counter()
    requests.post(f"{http_url}/query", json={"query": "CREATE VERTEX BenchNode (val INT64);"}, timeout=10)
    requests.post(f"{http_url}/query", json={"query": "CREATE EDGE BENCH_EDGE ();"}, timeout=10)

    client = flight.FlightClient(f"grpc://127.0.0.1:{flight_port}")

    # 1. Stream vertices via Arrow Flight do_put
    v_table = pa.table({"id": pa.array(vids, type=pa.int64())})
    v_desc = flight.FlightDescriptor.for_command(json.dumps({"type": "vertex", "label": "BenchNode"}).encode("utf-8"))
    writer, reader = client.do_put(v_desc, v_table.schema)
    writer.write_table(v_table, max_chunksize=262144)
    writer.close()
    try:
        reader.read()
    except Exception:
        pass

    # 2. Stream edges via Arrow Flight do_put
    e_table = pa.table({
        "src": pa.array(srcs, type=pa.int64()),
        "dst": pa.array(dsts, type=pa.int64()),
    })
    e_desc = flight.FlightDescriptor.for_command(json.dumps({"type": "edge", "edge_type": "BENCH_EDGE"}).encode("utf-8"))
    writer, reader = client.do_put(e_desc, e_table.schema)
    writer.write_table(e_table, max_chunksize=262144)
    writer.close()
    try:
        reader.read()
    except Exception:
        pass

    # 3. Compact CSR
    requests.post(f"{http_url}/compact", timeout=60)
    return (time.perf_counter() - t0) * 1000.0


def flight_ingest_sharded_cluster(
    http_urls: List[str],
    flight_ports: List[int],
    vids: List[int],
    srcs: List[int],
    dsts: List[int],
) -> float:
    t0 = time.perf_counter()
    num_nodes = len(flight_ports)
    requests.post(f"{http_urls[0]}/query", json={"query": "CREATE VERTEX BenchNode (val INT64);"}, timeout=10)
    requests.post(f"{http_urls[0]}/query", json={"query": "CREATE EDGE BENCH_EDGE ();"}, timeout=10)

    # Partition vertices by vid % num_nodes (matching calculate_replica_set for RF=1)
    v_parts: List[List[int]] = [[] for _ in range(num_nodes)]
    for v in vids:
        v_parts[v % num_nodes].append(v)

    # Partition edges by src % num_nodes
    e_src_parts: List[List[int]] = [[] for _ in range(num_nodes)]
    e_dst_parts: List[List[int]] = [[] for _ in range(num_nodes)]
    for s, d in zip(srcs, dsts):
        p = s % num_nodes
        e_src_parts[p].append(s)
        e_dst_parts[p].append(d)

    def push_shard(node_idx: int):
        fport = flight_ports[node_idx]
        client = flight.FlightClient(f"grpc://127.0.0.1:{fport}")

        vt = pa.table({"id": pa.array(v_parts[node_idx], type=pa.int64())})
        vd = flight.FlightDescriptor.for_command(json.dumps({"type": "vertex", "label": "BenchNode"}).encode("utf-8"))
        w, r = client.do_put(vd, vt.schema)
        w.write_table(vt, max_chunksize=262144)
        w.close()
        try:
            r.read()
        except Exception:
            pass

        et = pa.table({
            "src": pa.array(e_src_parts[node_idx], type=pa.int64()),
            "dst": pa.array(e_dst_parts[node_idx], type=pa.int64()),
        })
        ed = flight.FlightDescriptor.for_command(json.dumps({"type": "edge", "edge_type": "BENCH_EDGE"}).encode("utf-8"))
        w, r = client.do_put(ed, et.schema)
        w.write_table(et, max_chunksize=262144)
        w.close()
        try:
            r.read()
        except Exception:
            pass

    with ThreadPoolExecutor(max_workers=num_nodes) as pool:
        list(pool.map(push_shard, range(num_nodes)))

    requests.post(f"{http_urls[0]}/compact", timeout=120)
    return (time.perf_counter() - t0) * 1000.0


def run_gdb_suite(http_url: str, warmup_csr: bool = False) -> Dict[str, Tuple[float, int]]:
    """
    Executes all algorithms on the target GDB endpoint and returns {algo_name: (latency_ms, rows_affected)}.
    """
    session = requests.Session()

    if warmup_csr:
        session.post(
            f"{http_url}/query",
            json={"query": "CALL algo.similarity({node1: 1, node2: 2}) YIELD node1, node2, jaccard;"},
            timeout=120,
        )

    queries = [
        ("1. PageRank (20 iters)", "CALL algo.pageRank({max_iter: 20, tolerance: 0.0001, limit: 10}) YIELD vertex_id, score;"),
        ("2. Weakly Connected (WCC)", "CALL algo.wcc({limit: 10}) YIELD vertex_id, component_id;"),
        ("3. Strongly Connected (SCC)", "CALL algo.scc({limit: 10}) YIELD vertex_id, component_id;"),
        ("4. Triangles & Clustering", "CALL algo.triangleCount({limit: 10}) YIELD vertex_id, triangles, clustering_coefficient;"),
        ("5. K-Core Decomposition", "CALL algo.kcore({limit: 10}) YIELD vertex_id, coreness;"),
        ("6. Degree Centrality", "CALL algo.degree({limit: 10}) YIELD vertex_id, in_degree, out_degree, total_degree;"),
        ("7. Shortest Path (SSSP)", "CALL algo.sssp({source: 1, limit: 10}) YIELD vertex_id, distance;"),
        ("8. Betweenness (k=32 pivots)", "CALL algo.betweenness({normalized: true, limit: 10}) YIELD vertex_id, betweenness;"),
        ("9. Harmonic Closeness (k=32)", "CALL algo.closeness({limit: 10}) YIELD vertex_id, closeness;"),
        ("10. Louvain Modularity", "CALL algo.louvain({max_iter: 5, limit: 10}) YIELD vertex_id, community_id;"),
        ("11. Label Propagation (LPA)", "CALL algo.lpa({max_iter: 5, limit: 10}) YIELD vertex_id, community_id;"),
        ("12. Jaccard/Cosine Similarity", "CALL algo.similarity({node1: 1, node2: 2}) YIELD node1, node2, jaccard, cosine, common_neighbors;"),
        ("13. Node2Vec Embeddings", "CALL algo.node2vec({dimensions: 8, walk_length: 5, num_walks: 1, limit: 10}) YIELD vertex_id, embedding;"),
        ("14. Multi-Hop Traversal (*1..3)", "MATCH (a:BenchNode)-[:BENCH_EDGE*1..3]->(b:BenchNode) WHERE a.id = 1 RETURN b.id;"),
    ]

    results = {}
    for name, q in queries:
        t0 = time.perf_counter()
        resp = session.post(f"{http_url}/query", json={"query": q}, timeout=300).json()
        wall_ms = (time.perf_counter() - t0) * 1000.0
        if resp.get("status") != "ok":
            print(f"  [!] Error on {name}: {resp}", file=sys.stderr)
            results[name] = (-1.0, 0)
        else:
            rows_aff = resp.get("rows_affected", resp.get("num_rows", 0))
            results[name] = (wall_ms, rows_aff)
            print(f"  [GDB] {name:<32} : {wall_ms:>8.2f} ms (rows_processed={rows_aff:,})")

    return results


def run_rustworkx_suite(vids: List[int], srcs: List[int], dsts: List[int]) -> Tuple[float, Dict[str, float]]:
    """
    Executes the benchmark suite using rustworkx (Rust-backed NetworkX replacement).
    """
    num_v = len(vids)
    edge_tuples = [(s - 1, d - 1) for s, d in zip(srcs, dsts)]

    t0 = time.perf_counter()
    rx_g = rx.PyDiGraph(multigraph=False)
    rx_g.add_nodes_from(range(num_v))
    rx_g.extend_from_edge_list(edge_tuples)
    rx_build_ms = (time.perf_counter() - t0) * 1000.0
    print(f"[✓] Rustworkx PyDiGraph construction: {rx_build_ms:.2f} ms")

    t_undir = time.perf_counter()
    rx_undir = rx_g.to_undirected(multigraph=False)
    rx_undir_ms = (time.perf_counter() - t_undir) * 1000.0
    print(f"[✓] Rustworkx Undirected projection:  {rx_undir_ms:.2f} ms")

    rx_results: Dict[str, float] = {}

    def bench_rx(name: str, fn):
        t_start = time.perf_counter()
        fn()
        ms = (time.perf_counter() - t_start) * 1000.0
        rx_results[name] = ms
        print(f"  [RX ] {name:<32} : {ms:>8.2f} ms")

    bench_rx("1. PageRank (20 iters)", lambda: rx.pagerank(rx_g, alpha=0.85, max_iter=20, tol=1e-4))
    bench_rx("2. Weakly Connected (WCC)", lambda: rx.weakly_connected_components(rx_g))
    bench_rx("3. Strongly Connected (SCC)", lambda: rx.strongly_connected_components(rx_g))
    bench_rx("4. Triangles & Clustering", lambda: rx.transitivity(rx_undir))
    bench_rx("5. K-Core Decomposition", lambda: rx.core_number(rx_undir))
    bench_rx("6. Degree Centrality", lambda: (rx.in_degree_centrality(rx_g), rx.out_degree_centrality(rx_g)))
    bench_rx("7. Shortest Path (SSSP)", lambda: rx.dijkstra_shortest_path_lengths(rx_g, 0, lambda _: 1.0))

    # Note: rustworkx.betweenness_centrality does not support k-pivot sampling (runs full O(V*E) all-pairs).
    # We measure k=32 single-source shortest path trees in rustworkx for an apples-to-apples k=32 pivot comparison.
    def rx_pivots_32():
        pivots = [(i * num_v) // 32 for i in range(32)]
        for p in pivots:
            rx.dijkstra_shortest_path_lengths(rx_g, p, lambda _: 1.0)

    bench_rx("8. Betweenness (k=32 pivots)", rx_pivots_32)
    bench_rx("9. Harmonic Closeness (k=32)", rx_pivots_32)
    rx_results["10. Louvain Modularity"] = -1.0  # Not supported in rustworkx
    rx_results["11. Label Propagation (LPA)"] = -1.0  # Not supported in rustworkx

    def rx_sim():
        s1 = set(rx_g.successor_indices(0))
        s2 = set(rx_g.successor_indices(1))
        _ = len(s1 & s2) / max(1, len(s1 | s2))

    bench_rx("12. Jaccard/Cosine Similarity", rx_sim)
    rx_results["13. Node2Vec Embeddings"] = -1.0  # Not supported in rustworkx

    def rx_3hop():
        visited = {0}
        frontier = [0]
        for _ in range(3):
            nxt = []
            for u in frontier:
                for v in rx_g.successor_indices(u):
                    if v not in visited:
                        visited.add(v)
                        nxt.append(v)
            frontier = nxt

    bench_rx("14. Multi-Hop Traversal (*1..3)", rx_3hop)
    return rx_build_ms, rx_results


def main():
    parser = argparse.ArgumentParser(description="GDB vs Rustworkx vs NetworkX Large-Scale >1M V / >2M E Benchmark")
    parser.add_argument("--vertices", type=int, default=1_050_000, help="Number of vertices (default: 1,050,000)")
    parser.add_argument("--single-url", default="http://127.0.0.1:18840", help="Single-node HTTP URL")
    parser.add_argument("--single-flight-port", type=int, default=18860, help="Single-node Client Flight port")
    parser.add_argument("--cluster-urls", default="http://127.0.0.1:18841,http://127.0.0.1:18842,http://127.0.0.1:18843")
    parser.add_argument("--cluster-flight-ports", default="18861,18862,18863")
    parser.add_argument("--skip-nx", action="store_true", help="Skip pure-Python NetworkX suite")
    parser.add_argument("--skip-nx-slow", action="store_true", help="Skip Louvain/LPA/Betweenness in pure-Python NetworkX")
    args = parser.parse_args()

    vids, srcs, dsts = generate_edges_arrays(args.vertices)
    num_e = len(srcs)

    # 1. Single-Node GDB Benchmark
    print("\n" + "=" * 96)
    print(f" [1/4] SINGLE-NODE GDB BENCHMARK ({args.vertices:,} V, {num_e:,} E)")
    print("=" * 96)
    sn_ingest_ms = flight_ingest_single_node(args.single_url, args.single_flight_port, vids, srcs, dsts)
    print(f"[✓] Single-Node Arrow Flight Ingest + CSR Compaction: {sn_ingest_ms:.2f} ms")
    sn_results = run_gdb_suite(args.single_url, warmup_csr=False)

    # 2. Multi-Node 3-Shard RF=1 GDB Benchmark
    c_urls = [u.strip() for u in args.cluster_urls.split(",")]
    c_ports = [int(p.strip()) for p in args.cluster_flight_ports.split(",")]
    print("\n" + "=" * 96)
    print(f" [2/4] MULTI-NODE GDB CLUSTER (N={len(c_urls)}, RF=1) BENCHMARK ({args.vertices:,} V, {num_e:,} E)")
    print("=" * 96)
    mn_ingest_ms = flight_ingest_sharded_cluster(c_urls, c_ports, vids, srcs, dsts)
    print(f"[✓] 3-Node Sharded (RF=1) Arrow Flight Ingest + Compaction: {mn_ingest_ms:.2f} ms")

    for idx, u in enumerate(c_urls):
        st = requests.post(f"{u}/query", json={"query": "ANALYZE GRAPH;", "local_only": True}, timeout=30).json().get("stats", {})
        print(f"    Shard Node #{idx+1}: {st.get('total_vertices', 0):,} vertices, {st.get('total_edges', 0):,} edges")
    c_st = requests.post(f"{c_urls[0]}/query", json={"query": "ANALYZE GRAPH;"}, timeout=30).json().get("stats", {})
    print(f"    Cluster Global: {c_st.get('total_vertices', 0):,} vertices, {c_st.get('total_edges', 0):,} edges")

    t_csr = time.perf_counter()
    requests.post(
        f"{c_urls[0]}/query",
        json={"query": "CALL algo.similarity({node1: 1, node2: 2}) YIELD node1, node2, jaccard;"},
        timeout=120,
    )
    csr_sync_ms = (time.perf_counter() - t_csr) * 1000.0
    print(f"[✓] Initial Global CSR Assembly across 3 shards (RF=1): {csr_sync_ms:.2f} ms")
    mn_results = run_gdb_suite(c_urls[0], warmup_csr=False)

    # 3. Rustworkx ("NetworkX Rust") Benchmark
    print("\n" + "=" * 96)
    print(f" [3/4] RUSTWORKX (NETWORKX RUST v{rx.__version__}) BENCHMARK ({args.vertices:,} V, {num_e:,} E)")
    print("=" * 96)
    rx_build_ms, rx_results = run_rustworkx_suite(vids, srcs, dsts)

    # 4. Pure-Python NetworkX Benchmark
    nx_build_ms = -1.0
    nx_results: Dict[str, float] = {}
    if not args.skip_nx:
        print("\n" + "=" * 96)
        print(f" [4/4] NETWORKX (PURE PYTHON v{nx.__version__}) BENCHMARK ({args.vertices:,} V, {num_e:,} E)")
        print("=" * 96)
        t_nx_build = time.perf_counter()
        G = nx.DiGraph()
        G.add_nodes_from(vids)
        G.add_edges_from(zip(srcs, dsts))
        nx_build_ms = (time.perf_counter() - t_nx_build) * 1000.0
        print(f"[✓] NetworkX DiGraph construction: {nx_build_ms:.2f} ms")

        t_undir = time.perf_counter()
        G_undir = G.to_undirected()
        print(f"[✓] NetworkX Undirected Graph projection: {(time.perf_counter() - t_undir)*1000.0:.2f} ms")

        def bench_nx(name: str, fn):
            t0 = time.perf_counter()
            fn()
            ms = (time.perf_counter() - t0) * 1000.0
            nx_results[name] = ms
            print(f"  [NX ] {name:<32} : {ms:>8.2f} ms")

        from networkx.algorithms.link_analysis.pagerank_alg import _pagerank_python
        bench_nx("1. PageRank (20 iters)", lambda: _pagerank_python(G, alpha=0.85, max_iter=20, tol=1e-4))
        bench_nx("2. Weakly Connected (WCC)", lambda: list(nx.weakly_connected_components(G)))
        bench_nx("3. Strongly Connected (SCC)", lambda: list(nx.strongly_connected_components(G)))
        bench_nx("4. Triangles & Clustering", lambda: nx.triangles(G_undir))
        bench_nx("5. K-Core Decomposition", lambda: nx.core_number(G_undir))
        bench_nx("6. Degree Centrality", lambda: (dict(G.in_degree()), dict(G.out_degree())))
        bench_nx("7. Shortest Path (SSSP)", lambda: nx.single_source_shortest_path_length(G, 1))

        def nx_closeness_pivots():
            pivots = [(i * args.vertices) // 32 + 1 for i in range(32)]
            for p in pivots:
                nx.single_source_shortest_path_length(G, p)

        bench_nx("9. Harmonic Closeness (k=32)", nx_closeness_pivots)

        if not args.skip_nx_slow:
            bench_nx("8. Betweenness (k=32 pivots)", lambda: nx.betweenness_centrality(G, k=32, normalized=True, seed=42))
            bench_nx("10. Louvain Modularity", lambda: nx.community.louvain_communities(G_undir, seed=42))
            bench_nx("11. Label Propagation (LPA)", lambda: list(nx.community.asyn_lpa_communities(G_undir, seed=42)))

        def nx_sim():
            s1 = set(G.successors(1))
            s2 = set(G.successors(2))
            _ = len(s1 & s2) / max(1, len(s1 | s2))

        bench_nx("12. Jaccard/Cosine Similarity", nx_sim)
        nx_results["13. Node2Vec Embeddings"] = -1.0
        bench_nx("14. Multi-Hop Traversal (*1..3)", lambda: nx.single_source_shortest_path_length(G, 1, cutoff=3))

    # Print Comparison Table
    print("\n" + "=" * 124)
    print(f" LARGE-SCALE BENCHMARK SUMMARY ({args.vertices:,} VERTICES, {num_e:,} EDGES)")
    print("=" * 124)
    print(f"{'Algorithm / Operation':<31} | {'GDB Single':>12} | {'GDB 3N RF=1':>12} | {'Rustworkx':>12} | {'NetworkX':>12} | {'GDB vs RX':>11} | {'GDB vs NX':>11}")
    print("-" * 124)
    nx_b_str = f"{nx_build_ms:>9.1f} ms" if nx_build_ms > 0 else "      N/A   "
    print(
        f"{'Graph Ingest / Build':<31} | {sn_ingest_ms:>9.1f} ms | {mn_ingest_ms:>9.1f} ms | "
        f"{rx_build_ms:>9.1f} ms | {nx_b_str} | {rx_build_ms/mn_ingest_ms:>10.2f}x | "
        f"{(nx_build_ms/mn_ingest_ms if nx_build_ms > 0 else 0.0):>10.1f}x"
    )
    print("-" * 124)
    for name in sn_results.keys():
        sn_ms, _ = sn_results.get(name, (0.0, 0))
        mn_ms, _ = mn_results.get(name, (0.0, 0))
        rx_ms = rx_results.get(name, -1.0)
        nx_ms = nx_results.get(name, -1.0)
        rx_str = f"{rx_ms:>9.1f} ms" if rx_ms > 0 else "      N/A   "
        nx_str = f"{nx_ms:>9.1f} ms" if nx_ms > 0 else "      N/A   "
        sp_rx = f"{rx_ms / sn_ms:>10.2f}x" if (rx_ms > 0 and sn_ms > 0) else "        N/A"
        sp_nx = f"{nx_ms / sn_ms:>10.1f}x" if (nx_ms > 0 and sn_ms > 0) else "        N/A"
        print(f"{name:<31} | {sn_ms:>9.1f} ms | {mn_ms:>9.1f} ms | {rx_str} | {nx_str} | {sp_rx} | {sp_nx}")
    print("=" * 124)


if __name__ == "__main__":
    main()
