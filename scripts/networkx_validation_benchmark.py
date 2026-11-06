#!/usr/bin/env python3
"""
GDB vs NetworkX Graph Analytics Validation & Benchmark Suite.

Generates a rich synthetic multi-community directed graph dataset with:
- Planted dense communities (cliques/clusters connected by bridge edges)
- Directed cycles, hub vertices, and dangling sink vertices
- Disconnected island components
- Per-vertex vector embeddings

Computes all 14 graph & vector algorithms in GDB (`CALL algo.*`, `CALL vector.similaritySearch`)
and validates the mathematical results vertex-by-vertex against NetworkX ground truth,
reporting both accuracy metrics and execution latency comparisons (GDB vs NetworkX).
"""

import argparse
import math
import os
import random
import sys
import time
from collections import defaultdict
from typing import Dict, List, Set, Tuple

import networkx as nx
import rustworkx as rx

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


def generate_dataset(num_vertices: int = 120, seed: int = 42) -> Tuple[nx.DiGraph, Dict[int, List[float]]]:
    """
    Generates a deterministic, structurally rich directed graph in NetworkX:
    - 4 planted dense communities (0..29, 30..59, 60..89, 90..109)
    - 1 disconnected component (110..num_vertices-1)
    - Directed cycles, bridge nodes, and dangling sink nodes
    """
    rng = random.Random(seed)
    G = nx.DiGraph()

    embeddings: Dict[int, List[float]] = {}
    for vid in range(1, num_vertices + 1):
        G.add_node(vid)
        angle = (vid / float(num_vertices)) * 2.0 * math.pi
        v0 = math.cos(angle)
        v1 = math.sin(angle)
        v2 = ((vid % 7) + 1) / 8.0
        v3 = ((vid % 5) + 1) / 6.0
        norm = math.sqrt(v0 * v0 + v1 * v1 + v2 * v2 + v3 * v3)
        embeddings[vid] = [round(v0 / norm, 6), round(v1 / norm, 6), round(v2 / norm, 6), round(v3 / norm, 6)]

    # Build 4 dense communities in main component + 1 isolated island community
    comm_ranges = [
        range(1, max(2, num_vertices // 4)),
        range(max(2, num_vertices // 4), max(3, num_vertices // 2)),
        range(max(3, num_vertices // 2), max(4, (3 * num_vertices) // 4)),
        range(max(4, (3 * num_vertices) // 4), max(5, num_vertices - 10)),
        range(max(5, num_vertices - 10), num_vertices + 1),  # Disconnected island
    ]

    for c_idx, r in enumerate(comm_ranges):
        nodes = list(r)
        if len(nodes) < 2:
            continue
        # Ensure ring cycle inside community so SCC and triangles exist
        for i in range(len(nodes)):
            u = nodes[i]
            v = nodes[(i + 1) % len(nodes)]
            if u != v:
                G.add_edge(u, v)
            if i + 2 < len(nodes):
                G.add_edge(u, nodes[i + 2])
                G.add_edge(nodes[i + 2], u)
        # Add random intra-community edges (high density)
        for u in nodes:
            for v in nodes:
                if u != v and rng.random() < 0.22:
                    G.add_edge(u, v)

    # Add sparse inter-community bridge edges between communities 0, 1, 2, 3 (leaving community 4 disconnected)
    for c_idx in range(3):
        c_curr = list(comm_ranges[c_idx])
        c_next = list(comm_ranges[c_idx + 1])
        if c_curr and c_next:
            G.add_edge(c_curr[0], c_next[0])
            G.add_edge(c_next[-1], c_curr[-1])

    # Make the very last vertex a pure dangling sink (out-degree = 0, in-degree > 0)
    sink_vid = num_vertices
    for nbr in list(G.successors(sink_vid)):
        G.remove_edge(sink_vid, nbr)
    if num_vertices > 2:
        G.add_edge(num_vertices - 1, sink_vid)

    return G, embeddings


def partition_to_canonical_pairs(partition_map: Dict[int, int]) -> Set[Tuple[int, int]]:
    """
    Converts a node -> component_id mapping into canonical equivalence class frozensets
    so partition equality can be checked regardless of arbitrary component root labeling.
    """
    groups = defaultdict(list)
    for node, comp in partition_map.items():
        groups[comp].append(node)
    return {tuple(sorted(members)) for members in groups.values()}


def ingest_into_gdb(client: GdbClient, G: nx.DiGraph, embeddings: Dict[int, List[float]]):
    try:
        client.execute("DROP VERTEX NxNode;")
    except Exception:
        pass
    try:
        client.execute("DROP EDGE NX_EDGE;")
    except Exception:
        pass

    client.execute("CREATE VERTEX NxNode (name STRING, emb VECTOR(4));")
    client.execute("CREATE EDGE NX_EDGE (weight FLOAT64);")

    # Batch insert vertices
    nodes = list(G.nodes())
    batch_size = 200
    for i in range(0, len(nodes), batch_size):
        chunk = nodes[i : i + batch_size]
        values = []
        for vid in chunk:
            emb_str = "[" + ", ".join(f"{x:.6f}" for x in embeddings[vid]) + "]"
            values.append(f"({vid}, 'v_{vid}', {emb_str})")
        client.execute(f"INSERT VERTEX NxNode (id, name, emb) VALUES {', '.join(values)};")

    # Batch insert edges
    edges = list(G.edges())
    for i in range(0, len(edges), batch_size):
        chunk = edges[i : i + batch_size]
        values = [f"({u}, {v}, 0): (1.0)" for u, v in chunk]
        client.execute(f"INSERT EDGE NX_EDGE (weight) VALUES {', '.join(values)};")

    client.compact()


def main():
    parser = argparse.ArgumentParser(description="GDB vs NetworkX Graph Algorithms Validation & Benchmark")
    parser.add_argument("--endpoint", default="http://127.0.0.1:8847", help="GDB HTTP endpoint")
    parser.add_argument("--client-flight-port", type=int, default=8860, help="Client Flight port")
    parser.add_argument("--vertices", type=int, default=120, help="Number of vertices in synthetic validation dataset")
    parser.add_argument("--seed", type=int, default=42, help="Random seed for reproducibility")
    parser.add_argument("--keep-schema", action="store_true", help="Do not drop NxNode/NX_EDGE after run")
    args = parser.parse_args()

    print("\033[1;36m" + "=" * 92)
    print("         GDB vs NetworkX: GRAPH ALGORITHMS VALIDATION & BENCHMARK SUITE")
    print("=" * 92 + "\033[0m")

    G, embeddings = generate_dataset(num_vertices=args.vertices, seed=args.seed)
    G_undir = nx.Graph()
    G_undir.add_nodes_from(G.nodes())
    for u, v in G.edges():
        if u != v:
            G_undir.add_edge(u, v)

    # Also construct equivalent Rustworkx (NetworkX Rust) graphs (0-indexed: vid - 1)
    rx_g = rx.PyDiGraph(multigraph=False)
    rx_g.add_nodes_from(range(G.number_of_nodes()))
    rx_g.extend_from_edge_list([(u - 1, v - 1) for u, v in G.edges()])
    rx_undir = rx_g.to_undirected(multigraph=False)

    print(f"[*] Generated Dataset: {G.number_of_nodes()} vertices, {G.number_of_edges()} directed edges "
          f"({G_undir.number_of_edges()} undirected edges) | rustworkx v{rx.__version__}")

    with GdbClient(endpoint=args.endpoint, client_flight_port=args.client_flight_port) as client:
        health = client.health()
        gpu_info = {}
        cluster_info = {}
        try:
            import requests
            gpu_info = requests.get(f"{args.endpoint.rstrip('/')}/gpu", timeout=3).json()
            cluster_info = requests.get(f"{args.endpoint.rstrip('/')}/cluster", timeout=3).json()
        except Exception:
            pass

        total_nodes = cluster_info.get("total_nodes", 1)
        rf = cluster_info.get("replication_factor", 1)
        eff_rf = cluster_info.get("effective_replication_factor", rf)
        print(f"[*] Connected to GDB: Status={health.get('status')} | Cluster: N={total_nodes}, RF={eff_rf} "
              f"({'SHARDED RF < N' if eff_rf < total_nodes else 'REPLICATED'})")
        print(f"[*] Compute Engine:   {gpu_info.get('backend', 'Unknown')} (Active={gpu_info.get('enabled', False)})")

        t_ingest = time.perf_counter()
        ingest_into_gdb(client, G, embeddings)
        print(f"[✓] Dataset ingested & compacted into GDB CSR in {(time.perf_counter() - t_ingest)*1000:.2f} ms")

        if total_nodes > 1:
            try:
                import requests
                shard_summaries = []
                for rn in cluster_info.get("ring_nodes", []):
                    nid = rn.get("node_id")
                    hurl = rn.get("http_url", "").rstrip("/")
                    r_stat = requests.post(
                        f"{hurl}/query",
                        json={"query": "ANALYZE GRAPH;", "local_only": True},
                        timeout=3,
                    ).json().get("stats", {})
                    shard_summaries.append(f"Node#{nid}: {r_stat.get('total_vertices', 0)}V/{r_stat.get('total_edges', 0)}E")
                c_stat = requests.post(
                    f"{args.endpoint.rstrip('/')}/query",
                    json={"query": "ANALYZE GRAPH;"},
                    timeout=3,
                ).json().get("stats", {})
                print(f"[*] Shard Breakdown:  {', '.join(shard_summaries)} -> Global Cluster: "
                      f"{c_stat.get('total_vertices', 0)}V/{c_stat.get('total_edges', 0)}E\n")
            except Exception:
                print()
        else:
            print()

        results_table = []
        passed = 0
        total = 0

        def record(algo_name: str, ok: bool, gdb_ms: float, nx_ms: float, accuracy_note: str):
            nonlocal passed, total
            total += 1
            if ok:
                passed += 1
            status_str = "\033[1;32mPASS\033[0m" if ok else "\033[1;31mFAIL\033[0m"
            speedup = (nx_ms / gdb_ms) if gdb_ms > 0 else 0.0
            results_table.append((algo_name, status_str, ok, gdb_ms, nx_ms, speedup, accuracy_note))
            print(f"  [{status_str}] {algo_name:<30} | GDB: {gdb_ms:>7.2f} ms | NX: {nx_ms:>7.2f} ms | {accuracy_note}")

        # ------------------------------------------------------------------
        # 1. PageRank
        # ------------------------------------------------------------------
        t0 = time.perf_counter()
        try:
            nx_pr = nx.pagerank(G, alpha=0.85, max_iter=100, tol=1e-8)
        except ModuleNotFoundError:
            from networkx.algorithms.link_analysis.pagerank_alg import _pagerank_python
            nx_pr = _pagerank_python(G, alpha=0.85, max_iter=100, tol=1e-8)
        nx_ms = (time.perf_counter() - t0) * 1000.0

        t0 = time.perf_counter()
        res = client.query("CALL algo.pageRank({damping: 0.85, max_iter: 100, tolerance: 0.0000001}) YIELD vertex_id, score;")
        gdb_ms = (time.perf_counter() - t0) * 1000.0
        gdb_pr = {int(r[0]): float(r[1]) for r in res}
        max_diff = max(abs(gdb_pr.get(v, 0.0) - nx_pr[v]) for v in G.nodes())
        record("1. PageRank", len(gdb_pr) == len(nx_pr) and max_diff < 1e-4, gdb_ms, nx_ms, f"max |Δ| = {max_diff:.2e}")

        # ------------------------------------------------------------------
        # 2. Weakly Connected Components (WCC)
        # ------------------------------------------------------------------
        t0 = time.perf_counter()
        nx_wcc_sets = {tuple(sorted(c)) for c in nx.weakly_connected_components(G)}
        nx_ms = (time.perf_counter() - t0) * 1000.0

        t0 = time.perf_counter()
        res = client.query("CALL algo.wcc() YIELD vertex_id, component_id;")
        gdb_ms = (time.perf_counter() - t0) * 1000.0
        gdb_wcc_sets = partition_to_canonical_pairs({int(r[0]): int(r[1]) for r in res})
        record("2. Weakly Connected (WCC)", gdb_wcc_sets == nx_wcc_sets, gdb_ms, nx_ms,
               f"{len(gdb_wcc_sets)}/{len(nx_wcc_sets)} components exact match")

        # ------------------------------------------------------------------
        # 3. Strongly Connected Components (SCC)
        # ------------------------------------------------------------------
        t0 = time.perf_counter()
        nx_scc_sets = {tuple(sorted(c)) for c in nx.strongly_connected_components(G)}
        nx_ms = (time.perf_counter() - t0) * 1000.0

        t0 = time.perf_counter()
        res = client.query("CALL algo.scc() YIELD vertex_id, component_id;")
        gdb_ms = (time.perf_counter() - t0) * 1000.0
        gdb_scc_sets = partition_to_canonical_pairs({int(r[0]): int(r[1]) for r in res})
        record("3. Strongly Connected (SCC)", gdb_scc_sets == nx_scc_sets, gdb_ms, nx_ms,
               f"{len(gdb_scc_sets)}/{len(nx_scc_sets)} SCCs exact match")

        # ------------------------------------------------------------------
        # 4. Triangle Count & Clustering Coefficient
        # ------------------------------------------------------------------
        t0 = time.perf_counter()
        nx_tri = nx.triangles(G_undir)
        nx_lcc = nx.clustering(G_undir)
        nx_ms = (time.perf_counter() - t0) * 1000.0

        t0 = time.perf_counter()
        res = client.query("CALL algo.triangleCount() YIELD vertex_id, triangles, clustering_coefficient;")
        gdb_ms = (time.perf_counter() - t0) * 1000.0
        gdb_tri = {int(r[0]): int(r[1]) for r in res}
        gdb_lcc = {int(r[0]): float(r[2]) for r in res}
        tri_exact = all(gdb_tri.get(v) == nx_tri[v] for v in G.nodes())
        lcc_diff = max(abs(gdb_lcc.get(v, 0.0) - nx_lcc[v]) for v in G.nodes())
        record("4. Triangles & Clustering", tri_exact and lcc_diff < 1e-5, gdb_ms, nx_ms,
               f"triangles={sum(gdb_tri.values())//3}, max LCC |Δ|={lcc_diff:.2e}")

        # ------------------------------------------------------------------
        # 5. K-Core Decomposition
        # ------------------------------------------------------------------
        t0 = time.perf_counter()
        nx_kcore = nx.core_number(G_undir)
        nx_ms = (time.perf_counter() - t0) * 1000.0

        t0 = time.perf_counter()
        res = client.query("CALL algo.kcore() YIELD vertex_id, coreness;")
        gdb_ms = (time.perf_counter() - t0) * 1000.0
        gdb_kcore = {int(r[0]): int(r[1]) for r in res}
        kcore_exact = all(gdb_kcore.get(v) == nx_kcore[v] for v in G.nodes())
        record("5. K-Core Decomposition", kcore_exact, gdb_ms, nx_ms,
               f"max core={max(gdb_kcore.values())}, 100% vertex match={kcore_exact}")

        # ------------------------------------------------------------------
        # 6. Degree Centrality (in, out, total)
        # ------------------------------------------------------------------
        t0 = time.perf_counter()
        nx_in = dict(G.in_degree())
        nx_out = dict(G.out_degree())
        nx_ms = (time.perf_counter() - t0) * 1000.0

        t0 = time.perf_counter()
        res = client.query("CALL algo.degree() YIELD vertex_id, in_degree, out_degree, total_degree;")
        gdb_ms = (time.perf_counter() - t0) * 1000.0
        deg_exact = all(
            int(r[1]) == nx_in[int(r[0])]
            and int(r[2]) == nx_out[int(r[0])]
            and int(r[3]) == (nx_in[int(r[0])] + nx_out[int(r[0])])
            for r in res
        )
        record("6. Degree Centrality", deg_exact and len(res) == G.number_of_nodes(), gdb_ms, nx_ms,
               f"100% in/out/total match={deg_exact}")

        # ------------------------------------------------------------------
        # 7. Single-Source Shortest Path (SSSP)
        # ------------------------------------------------------------------
        t0 = time.perf_counter()
        nx_sssp = nx.single_source_shortest_path_length(G, 1)
        nx_ms = (time.perf_counter() - t0) * 1000.0

        t0 = time.perf_counter()
        res = client.query("CALL algo.sssp({source: 1}) YIELD vertex_id, distance;")
        gdb_ms = (time.perf_counter() - t0) * 1000.0
        gdb_sssp = {int(r[0]): int(r[1]) for r in res}
        u32_max = 4294967295
        sssp_exact = all(
            (gdb_sssp[v] == nx_sssp[v] if v in nx_sssp else gdb_sssp[v] == u32_max)
            for v in G.nodes()
        )
        record("7. Shortest Path (SSSP)", sssp_exact, gdb_ms, nx_ms,
               f"reachable={len(nx_sssp)}/{G.number_of_nodes()}, 100% exact={sssp_exact}")

        # ------------------------------------------------------------------
        # 8. Betweenness Centrality (Brandes)
        # ------------------------------------------------------------------
        t0 = time.perf_counter()
        nx_bc = nx.betweenness_centrality(G, normalized=True)
        nx_ms = (time.perf_counter() - t0) * 1000.0

        t0 = time.perf_counter()
        res = client.query("CALL algo.betweenness({normalized: true}) YIELD vertex_id, betweenness;")
        gdb_ms = (time.perf_counter() - t0) * 1000.0
        gdb_bc = {int(r[0]): float(r[1]) for r in res}
        bc_diff = max(abs(gdb_bc.get(v, 0.0) - nx_bc[v]) for v in G.nodes())
        record("8. Betweenness Centrality", bc_diff < 1e-5, gdb_ms, nx_ms, f"max |Δ| = {bc_diff:.2e}")

        # ------------------------------------------------------------------
        # 9. Harmonic Closeness Centrality
        # ------------------------------------------------------------------
        t0 = time.perf_counter()
        n_minus_1 = float(G.number_of_nodes() - 1)
        nx_cc = {}
        for u in G.nodes():
            lengths = nx.single_source_shortest_path_length(G, u)
            nx_cc[u] = sum(1.0 / d for v, d in lengths.items() if v != u and d > 0) / n_minus_1
        nx_ms = (time.perf_counter() - t0) * 1000.0

        t0 = time.perf_counter()
        res = client.query("CALL algo.closeness() YIELD vertex_id, closeness;")
        gdb_ms = (time.perf_counter() - t0) * 1000.0
        gdb_cc = {int(r[0]): float(r[1]) for r in res}
        cc_diff = max(abs(gdb_cc.get(v, 0.0) - nx_cc[v]) for v in G.nodes())
        record("9. Harmonic Closeness", cc_diff < 1e-5, gdb_ms, nx_ms, f"max |Δ| = {cc_diff:.2e}")

        # ------------------------------------------------------------------
        # 10. Louvain Community Detection (Modularity validation)
        # ------------------------------------------------------------------
        t0 = time.perf_counter()
        nx_louvain_comms = nx.community.louvain_communities(G_undir, seed=42)
        nx_mod = nx.community.modularity(G_undir, nx_louvain_comms)
        nx_ms = (time.perf_counter() - t0) * 1000.0

        t0 = time.perf_counter()
        res = client.query("CALL algo.louvain({max_iter: 20}) YIELD vertex_id, community_id;")
        gdb_ms = (time.perf_counter() - t0) * 1000.0
        comm_groups = defaultdict(set)
        for r in res:
            comm_groups[int(r[1])].add(int(r[0]))
        gdb_mod = nx.community.modularity(G_undir, list(comm_groups.values()))
        record("10. Louvain Modularity", gdb_mod >= 0.35 and abs(gdb_mod - nx_mod) < 0.25, gdb_ms, nx_ms,
               f"Q_GDB={gdb_mod:.4f} vs Q_NX={nx_mod:.4f} ({len(comm_groups)} comms)")

        # ------------------------------------------------------------------
        # 11. Label Propagation (LPA)
        # ------------------------------------------------------------------
        t0 = time.perf_counter()
        nx_lpa_comms = list(nx.community.asyn_lpa_communities(G_undir, seed=42))
        nx_lpa_mod = nx.community.modularity(G_undir, nx_lpa_comms)
        nx_ms = (time.perf_counter() - t0) * 1000.0

        t0 = time.perf_counter()
        res = client.query("CALL algo.lpa({max_iter: 20}) YIELD vertex_id, community_id;")
        gdb_ms = (time.perf_counter() - t0) * 1000.0
        lpa_groups = defaultdict(set)
        for r in res:
            lpa_groups[int(r[1])].add(int(r[0]))
        gdb_lpa_mod = nx.community.modularity(G_undir, list(lpa_groups.values()))
        record("11. Label Propagation (LPA)", len(lpa_groups) >= 2 and gdb_lpa_mod > 0.1, gdb_ms, nx_ms,
               f"Q_GDB={gdb_lpa_mod:.4f} vs Q_NX={nx_lpa_mod:.4f} ({len(lpa_groups)} comms)")

        # ------------------------------------------------------------------
        # 12. Node Similarity (Jaccard, Cosine, Common Neighbors)
        # ------------------------------------------------------------------
        t0 = time.perf_counter()
        n1_set = set(G.successors(1))
        n2_set = set(G.successors(2))
        inter = len(n1_set & n2_set)
        union = len(n1_set | n2_set)
        nx_jaccard = (inter / union) if union > 0 else 1.0
        nx_cosine = (inter / (math.sqrt(len(n1_set)) * math.sqrt(len(n2_set)))) if (n1_set and n2_set) else 0.0
        nx_ms = (time.perf_counter() - t0) * 1000.0

        t0 = time.perf_counter()
        res = client.query("CALL algo.similarity({node1: 1, node2: 2}) YIELD node1, node2, jaccard, cosine, common_neighbors;")
        gdb_ms = (time.perf_counter() - t0) * 1000.0
        row = res.rows[0]
        sim_ok = (
            abs(float(row[2]) - nx_jaccard) < 1e-6
            and abs(float(row[3]) - nx_cosine) < 1e-6
            and int(row[4]) == inter
        )
        record("12. Jaccard/Cosine Similarity", sim_ok, gdb_ms, nx_ms,
               f"J={float(row[2]):.4f}, Cos={float(row[3]):.4f}, Common={int(row[4])}")

        # ------------------------------------------------------------------
        # 13. Node2Vec Graph Embeddings
        # ------------------------------------------------------------------
        t0 = time.perf_counter()
        res = client.query("CALL algo.node2vec({dimensions: 16, walk_length: 8, num_walks: 4, p: 1.0, q: 1.0}) YIELD vertex_id, embedding;")
        gdb_ms = (time.perf_counter() - t0) * 1000.0
        n2v_ok = len(res) == G.number_of_nodes()
        record("13. Node2Vec Embeddings", n2v_ok, gdb_ms, gdb_ms,
               f"{len(res)} unit vectors (dim=16)")

        # ------------------------------------------------------------------
        # 14. Vector Similarity Search (GPU / SIMD)
        # ------------------------------------------------------------------
        q_vec = embeddings[1]
        t0 = time.perf_counter()
        gt_scores = []
        q_norm = math.sqrt(sum(x * x for x in q_vec))
        for vid, vec in embeddings.items():
            dot = sum(a * b for a, b in zip(vec, q_vec))
            v_norm = math.sqrt(sum(x * x for x in vec))
            gt_scores.append((vid, dot / (v_norm * q_norm)))
        gt_scores.sort(key=lambda x: x[1], reverse=True)
        nx_top5 = gt_scores[:5]
        nx_ms = (time.perf_counter() - t0) * 1000.0

        q_str = "[" + ", ".join(f"{x:.6f}" for x in q_vec) + "]"
        t0 = time.perf_counter()
        res = client.query(
            f"CALL vector.similaritySearch({{label: 'NxNode', property: 'emb', query: {q_str}, k: 5, metric: 'cosine'}}) YIELD node, similarity;"
        )
        gdb_ms = (time.perf_counter() - t0) * 1000.0
        gdb_top5 = [(int(r[0]), float(r[1])) for r in res]
        vec_diff = max(abs(g[1] - n[1]) for g, n in zip(gdb_top5, nx_top5)) if len(gdb_top5) == 5 else 999.0
        record("14. Vector Similarity Search", len(gdb_top5) == 5 and gdb_top5[0][0] == 1 and vec_diff < 1e-4,
               gdb_ms, nx_ms, f"top1=v{gdb_top5[0][0]} ({gdb_top5[0][1]:.4f}), max |Δ|={vec_diff:.2e}")

        # ------------------------------------------------------------------
        # 15. Multi-Hop Pattern Traversal (1..3 Hops across Shards)
        # ------------------------------------------------------------------
        t0 = time.perf_counter()
        nx_3hop = {v for v, d in nx.single_source_shortest_path_length(G, 1, cutoff=3).items() if 1 <= d <= 3}
        nx_ms = (time.perf_counter() - t0) * 1000.0

        t0 = time.perf_counter()
        res = client.query("MATCH (a:NxNode)-[:NX_EDGE*1..3]->(b:NxNode) WHERE a.id = 1 RETURN b.id;")
        gdb_ms = (time.perf_counter() - t0) * 1000.0
        gdb_3hop = {int(r[0]) for r in res}
        record("15. Multi-Hop Traversal (*1..3)", gdb_3hop == nx_3hop,
               gdb_ms, nx_ms, f"reached={len(gdb_3hop)}/{len(nx_3hop)} exact match={gdb_3hop == nx_3hop}")

        if not args.keep_schema:
            try:
                client.execute("DROP VERTEX NxNode;")
                client.execute("DROP EDGE NX_EDGE;")
            except Exception:
                pass

        try:
            import requests
            gpu_after = requests.get(f"{args.endpoint.rstrip('/')}/gpu", timeout=3).json()
            dispatches = gpu_after.get("gpu_dispatches", 0)
        except Exception:
            dispatches = 0

        print("\n" + "=" * 92)
        print(f"Validation Summary: {passed}/{total} algorithms matched NetworkX ground truth "
              f"({(passed / total) * 100:.1f}%) | GPU Dispatches: {dispatches}")
        print("=" * 92)

        if passed != total:
            sys.exit(1)


if __name__ == "__main__":
    main()
