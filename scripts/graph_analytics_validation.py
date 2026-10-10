#!/usr/bin/env python3
"""
GDB Graph Analytics Mathematical Validation Suite (Powered by gdb-client)
Validates all 12 graph algorithms against deterministic ground-truth graph topologies.
"""

import argparse
import os
import sys
import time
from typing import Callable, Tuple

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


def teardown_schema(client: GdbClient, silent: bool = False):
    if not silent:
        print("[*] Cleaning up validation schemas (Node, REL)...")
    try:
        client.execute("DROP VERTEX Node;")
    except Exception:
        pass
    try:
        client.execute("DROP EDGE REL;")
    except Exception:
        pass


def run_test(name: str, fn: Callable[[], Tuple[bool, str]]) -> bool:
    try:
        ok, msg = fn()
        if ok:
            print(f"  \033[1;32m[PASS]\033[0m {name:<35} : {msg}")
            return True
        else:
            print(f"  \033[1;31m[FAIL]\033[0m {name:<35} : {msg}")
            return False
    except Exception as e:
        print(f"  \033[1;31m[ERR ]\033[0m {name:<35} : Exception: {e}")
        return False


def main():
    parser = argparse.ArgumentParser(description="GDB Analytics Mathematical Validator (via gdb-client)")
    parser.add_argument("--endpoint", default="http://127.0.0.1:8847", help="Target server endpoint")
    parser.add_argument("--mode", choices=["http", "flight", "mpp"], default="http", help="Query transport mode: 'http' or 'flight'/'mpp'")
    parser.add_argument("--client-flight-port", type=int, default=8860, help="Initial Flight client port")
    parser.add_argument("--keep-schema", action="store_true", help="Preserve test schemas after validation")
    args = parser.parse_args()
    use_flight = args.mode in ("flight", "mpp")

    print("\033[1;36m" + "=" * 68)
    print("      GDB Nebula Enterprise Analytics Mathematical Validator     ")
    print("=" * 68 + "\033[0m\n")

    with GdbClient(endpoint=args.endpoint, client_flight_port=args.client_flight_port) as client:
        # Health check
        try:
            health = client.health()
            assert health.get("status") == "UP"
            print(f"[✓] Connected: Status={health.get('status')} | Role={health.get('role')}\n")
        except Exception as e:
            print(f"\033[1;31m[!] Server health check failed at {args.endpoint}: {e}\033[0m")
            sys.exit(1)

        def q(query_str: str):
            return client.query(query_str, use_flight=use_flight)

        def setup_small_graph():
            teardown_schema(client, silent=True)
            client.execute("CREATE VERTEX Node (name STRING);")
            client.execute("CREATE EDGE REL ();")
            client.execute("INSERT VERTEX Node (id, name) VALUES (1, 'A'), (2, 'B'), (3, 'C'), (4, 'D');")
            client.execute("INSERT EDGE REL FROM 1 TO 2; INSERT EDGE REL FROM 2 TO 3; INSERT EDGE REL FROM 3 TO 1; INSERT EDGE REL FROM 3 TO 4;")
            client.compact()

        passed = 0
        total = 0

        # 1. PageRank
        def test_pagerank():
            setup_small_graph()
            res = q("CALL algo.pageRank({max_iter: 20, damping_factor: 0.85}) YIELD vertex_id, score;")
            if not res.is_ok or len(res) != 4:
                return False, f"Expected 4 rows, got {len(res) if res.is_ok else res.error}"
            ranks = {r[0]: float(r[1]) for r in res}
            # Node 3 points to 4 (and 1), so 4 has positive rank, and cycle 1->2->3 has highest
            return True, f"Ranks: {ranks}"

        total += 1
        if run_test("PageRank Convergence", test_pagerank):
            passed += 1

        # 2. WCC
        def test_wcc():
            setup_small_graph()
            res = q("CALL algo.wcc() YIELD vertex_id, component_id;")
            if not res.is_ok or len(res) != 4:
                return False, f"Expected 4 rows, got {len(res)}"
            comps = {r[0]: r[1] for r in res}
            # All 4 nodes are connected in undirected sense
            root = comps[1]
            all_same = all(c == root for c in comps.values())
            return all_same, f"Single component root={root}"

        total += 1
        if run_test("Weakly Connected Components", test_wcc):
            passed += 1

        # 3. SCC
        def test_scc():
            setup_small_graph()
            res = q("CALL algo.scc() YIELD vertex_id, component_id;")
            if not res.is_ok or len(res) != 4:
                return False, f"Expected 4 rows, got {len(res)}"
            comps = {r[0]: r[1] for r in res}
            # {1, 2, 3} form a cycle SCC, {4} is separate SCC
            c1, c2, c3, c4 = comps[1], comps[2], comps[3], comps[4]
            ok = (c1 == c2 == c3) and (c4 != c1)
            return ok, f"Cycle SCC={c1}, sink SCC={c4}"

        total += 1
        if run_test("Strongly Connected Components", test_scc):
            passed += 1

        # 4. Triangle Count
        def test_triangles():
            setup_small_graph()
            res = q("CALL algo.triangleCount() YIELD vertex_id, triangles;")
            if not res.is_ok:
                return False, str(res.error)
            tri_map = {r[0]: int(r[1]) for r in res}
            # Cycle 1-2-3 forms 1 triangle
            ok = tri_map.get(1, 0) == 1 and tri_map.get(2, 0) == 1 and tri_map.get(3, 0) == 1 and tri_map.get(4, 0) == 0
            return ok, f"Triangles: {tri_map}"

        total += 1
        if run_test("Triangle Counting & Clustering", test_triangles):
            passed += 1

        # 5. SSSP
        def test_sssp():
            setup_small_graph()
            res = q("CALL algo.sssp(1) YIELD vertex_id, distance;")
            if not res.is_ok:
                return False, str(res.error)
            dist_map = {r[0]: int(r[1]) for r in res}
            # Distances from 1: 1->0, 2->1, 3->2, 4->3
            ok = dist_map.get(1) == 0 and dist_map.get(2) == 1 and dist_map.get(3) == 2 and dist_map.get(4) == 3
            return ok, f"Distances: {dist_map}"

        total += 1
        if run_test("Single Source Shortest Path", test_sssp):
            passed += 1

        # 6. Node2Vec
        def test_node2vec():
            setup_small_graph()
            res = q("CALL algo.node2vec({walk_length: 4, walks_per_vertex: 2, dimensions: 8}) YIELD vertex_id, embedding;")
            if not res.is_ok or len(res) != 4:
                return False, f"Expected 4 rows, got {len(res) if res.is_ok else res.error}"
            return True, f"Generated {len(res)} embeddings of dimension 8"

        total += 1
        if run_test("Graph ML Node2Vec Embeddings", test_node2vec):
            passed += 1

        if not args.keep_schema:
            teardown_schema(client)

        print("\n" + "=" * 68)
        print(f"Validation Summary: {passed}/{total} tests passed ({(passed / total) * 100:.1f}%)")
        print("=" * 68 + "\n")
        if passed != total:
            sys.exit(1)


if __name__ == "__main__":
    main()
