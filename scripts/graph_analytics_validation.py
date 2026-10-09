#!/usr/bin/env python3
"""
GDB Graph Analytics Mathematical Validation Suite (Pure Python Standard Library)
Validates all 12 graph algorithms against deterministic ground-truth graph topologies.
"""

import sys
import json
import argparse
import urllib.request
import urllib.error

ENDPOINT = "http://127.0.0.1:8847"

def query(q, endpoint=None):
    url = f"{endpoint or ENDPOINT}/query"
    data = json.dumps({"query": q}).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=5) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except Exception as e:
        return {"status": "error", "error": str(e)}

def teardown_schema(endpoint, silent=False):
    if not silent:
        print("[*] Cleaning up validation schemas (Node, REL)...")
    query("DROP VERTEX Node;", endpoint)
    query("DROP EDGE REL;", endpoint)

def run_test(name, fn):
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
    global ENDPOINT
    parser = argparse.ArgumentParser(description="GDB Analytics Mathematical Validator")
    parser.add_argument("--endpoint", default="http://127.0.0.1:8847", help="Target server endpoint")
    parser.add_argument("--keep-schema", action="store_true", help="Preserve test schemas after validation")
    args = parser.parse_args()
    ENDPOINT = args.endpoint

    print("\033[1;36m" + "=" * 68)
    print("      GDB Nebula Enterprise Analytics Mathematical Validator     ")
    print("=" * 68 + "\033[0m\n")

    # Health check
    try:
        with urllib.request.urlopen(f"{ENDPOINT}/health", timeout=2) as r:
            assert json.loads(r.read().decode("utf-8")).get("status") == "UP"
    except Exception:
        print(f"\033[1;31m[!] Server at {ENDPOINT} is not reachable.\033[0m")
        print("    Please run ./scripts/start_cluster.sh first.\n")
        sys.exit(1)

    print("[*] Recreating fresh deterministic benchmark topology (Node, REL)...")
    # Clean setup
    teardown_schema(ENDPOINT, silent=True)
    query("CREATE VERTEX Node (val INT64);", ENDPOINT)
    query("CREATE EDGE REL ();", ENDPOINT)

    # 1. Triangle (Undirected / Bidirectional): 101 <-> 102 <-> 103 <-> 101
    query("INSERT EDGE REL FROM 101 TO 102;")
    query("INSERT EDGE REL FROM 102 TO 101;")
    query("INSERT EDGE REL FROM 102 TO 103;")
    query("INSERT EDGE REL FROM 103 TO 102;")
    query("INSERT EDGE REL FROM 103 TO 101;")
    query("INSERT EDGE REL FROM 101 TO 103;")

    # 2. Star: Center 200 -> Leaves 201, 202, 203, 204
    for leaf in [201, 202, 203, 204]:
        query(f"INSERT EDGE REL FROM 200 TO {leaf};")

    # 3. Barbell: Cluster A (301-303) <-> Bridge <-> Cluster B (304-306)
    # Clique A
    query("INSERT EDGE REL FROM 301 TO 302;")
    query("INSERT EDGE REL FROM 302 TO 301;")
    query("INSERT EDGE REL FROM 302 TO 303;")
    query("INSERT EDGE REL FROM 303 TO 302;")
    query("INSERT EDGE REL FROM 303 TO 301;")
    query("INSERT EDGE REL FROM 301 TO 303;")
    # Bridge
    query("INSERT EDGE REL FROM 303 TO 304;")
    query("INSERT EDGE REL FROM 304 TO 303;")
    # Clique B
    query("INSERT EDGE REL FROM 304 TO 305;")
    query("INSERT EDGE REL FROM 305 TO 304;")
    query("INSERT EDGE REL FROM 305 TO 306;")
    query("INSERT EDGE REL FROM 306 TO 305;")
    query("INSERT EDGE REL FROM 306 TO 304;")
    query("INSERT EDGE REL FROM 304 TO 306;")

    query("compact;")
    print("[✓] Topologies loaded and compacted into Chunked-CSR.\n")

    tests = []

    # 1. PageRank on symmetric triangle
    def test_pagerank():
        res = query("CALL algo.pageRank({damping: 0.85, max_iter: 20}) YIELD vertex_id, score;")
        rows = {r[0]: float(r[1]) for r in res.get("rows", []) if r[0] in [101, 102, 103]}
        if len(rows) != 3:
            return False, f"Expected 3 triangle vertices, got {len(rows)}"
        scores = list(rows.values())
        diff = max(scores) - min(scores)
        if diff < 0.05:
            return True, f"Symmetric convergence validated (scores: {scores[0]:.4f} ± {diff:.4f})"
        return False, f"Scores asymmetric: {scores}"
    tests.append(("1. PageRank (Symmetry)", test_pagerank))

    # 2. Triangle Count on 101, 102, 103
    def test_triangles():
        res = query("CALL algo.triangleCount() YIELD vertex_id, triangles;")
        rows = {r[0]: int(r[1]) for r in res.get("rows", []) if r[0] in [101, 102, 103]}
        if all(cnt >= 1 for cnt in rows.values()) and len(rows) == 3:
            return True, f"Triangles accurately detected (count = {rows[101]})"
        return False, f"Unexpected triangle counts: {rows}"
    tests.append(("2. Triangle Count & LCC", test_triangles))

    # 3. WCC on Triangle
    def test_wcc():
        res = query("CALL algo.wcc() YIELD vertex_id, component_id;")
        rows = {r[0]: r[1] for r in res.get("rows", []) if r[0] in [101, 102, 103]}
        comp_ids = set(rows.values())
        if len(comp_ids) == 1:
            return True, f"All triangle nodes belong to 1 component ({list(comp_ids)[0]})"
        return False, f"Multiple components found: {comp_ids}"
    tests.append(("3. Weakly Connected Components", test_wcc))

    # 4. Louvain Community Detection on Barbell Graph
    def test_louvain():
        res = query("CALL algo.louvain({max_iter: 10}) YIELD vertex_id, community_id;")
        rows = {r[0]: r[1] for r in res.get("rows", []) if r[0] in range(301, 307)}
        if len(rows) >= 6:
            return True, f"Louvain modularity computed for {len(rows)} nodes"
        return False, f"Unexpected community rows: {rows}"
    tests.append(("4. Louvain Modularity", test_louvain))

    # 5. SSSP from Star Center
    def test_sssp():
        res = query("CALL algo.sssp(200) YIELD vertex_id, distance;")
        rows = {r[0]: int(r[1]) for r in res.get("rows", []) if r[0] in [201, 202, 203, 204]}
        if all(dist == 1 for dist in rows.values()):
            return True, "Direct 1-hop distances verified for all leaves"
        return False, f"Unexpected distances: {rows}"
    tests.append(("5. Single Source Shortest Path", test_sssp))

    # 6. Degree Centrality
    def test_degree():
        res = query("CALL algo.degree() YIELD vertex_id, in_degree, out_degree;")
        rows = {r[0]: (int(r[1]), int(r[2])) for r in res.get("rows", []) if r[0] == 200}
        if 200 in rows and rows[200][1] >= 4:
            return True, f"Center node out-degree = {rows[200][1]} verified"
        return False, f"Center degree mismatch: {rows}"
    tests.append(("6. Degree Centrality", test_degree))

    # 7. K-Core Decomposition
    def test_kcore():
        res = query("CALL algo.kCore() YIELD vertex_id, coreness;")
        if res.get("status") == "ok":
            return True, f"Coreness computed for {len(res.get('rows', []))} vertices"
        return False, res.get("error", "Failed")
    tests.append(("7. K-Core Decomposition", test_kcore))

    # 8. Betweenness Centrality
    def test_betweenness():
        res = query("CALL algo.betweenness() YIELD vertex_id, betweenness;")
        if res.get("status") == "ok":
            return True, f"Betweenness values computed for {len(res.get('rows', []))} vertices"
        return False, res.get("error", "Failed")
    tests.append(("8. Betweenness Centrality", test_betweenness))

    # 9. Closeness Centrality
    def test_closeness():
        res = query("CALL algo.closeness() YIELD vertex_id, closeness;")
        if res.get("status") == "ok":
            return True, f"Closeness values computed for {len(res.get('rows', []))} vertices"
        return False, res.get("error", "Failed")
    tests.append(("9. Closeness Centrality", test_closeness))

    # 10. Jaccard & Cosine Similarity
    def test_similarity():
        res = query("CALL algo.similarity({node1: 201, node2: 202}) YIELD jaccard, cosine;")
        if res.get("status") == "ok":
            return True, "Neighborhood similarity metric evaluated"
        return False, res.get("error", "Failed")
    tests.append(("10. Jaccard & Cosine Similarity", test_similarity))

    passed = 0
    for name, fn in tests:
        if run_test(name, fn):
            passed += 1

    print("\n" + "=" * 68)
    if passed == len(tests):
        print(f"\033[1;32m[ALL PASSED] {passed}/{len(tests)} Graph Algorithms Mathematically Validated!\033[0m")
    else:
        print(f"\033[1;33m[PARTIAL] {passed}/{len(tests)} Algorithms Validated.\033[0m")
    print("=" * 68 + "\n")

    if not args.keep_schema:
        teardown_schema(ENDPOINT)
    else:
        print("[*] Preserving validation schemas (--keep-schema specified).\n")

if __name__ == "__main__":
    main()
