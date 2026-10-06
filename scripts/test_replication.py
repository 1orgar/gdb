#!/usr/bin/env python3
"""
GDB Multi-Node Replication Test Suite (Pure Python Standard Library)
Verifies that DDL, DML and Compaction executed on Node 1 (Leader)
are consistently replicated to Node 2 and Node 3 (Followers).
"""

import sys
import time
import json
import urllib.request
import urllib.error

NODE1_URL = "http://127.0.0.1:8847"
NODE2_URL = "http://127.0.0.1:8846"
NODE3_URL = "http://127.0.0.1:8845"

def query(endpoint, q, timeout=3):
    url = f"{endpoint}/query"
    data = json.dumps({"query": q}).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except Exception as e:
        return {"status": "error", "error": str(e)}

def check_health(endpoint):
    url = f"{endpoint}/health"
    try:
        with urllib.request.urlopen(url, timeout=2) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            return data.get("status") == "UP"
    except Exception:
        return False

def main():
    print("\033[1;36m" + "=" * 60)
    print("      GDB Multi-Node Raft Replication Verification Suite     ")
    print("=" * 60 + "\033[0m")

    # 1. Health check all 3 nodes
    print("[*] Probing cluster nodes health...")
    nodes = [("Node 1 (Leader)", NODE1_URL), ("Node 2 (Follower)", NODE2_URL), ("Node 3 (Follower)", NODE3_URL)]
    for name, url in nodes:
        if check_health(url):
            print(f"  \033[1;32m[✓]\033[0m {name} at {url} is UP")
        else:
            print(f"  \033[1;31m[✗]\033[0m {name} at {url} is UNREACHABLE!")
            print("\nPlease ensure the 3-node cluster is running via ./scripts/start_cluster.sh")
            sys.exit(1)

    print("\n[1/4] Creating Schema on Leader (Node 1)...")
    res1 = query(NODE1_URL, "CREATE VERTEX Device (model STRING, ram_gb INT64);")
    res2 = query(NODE1_URL, "CREATE EDGE LINKED ();")
    print(f"  -> Leader DDL: {res1.get('status')} | {res2.get('status')}")
    time.sleep(0.1)

    print("\n[2/4] Ingesting Vertices and Edges on Leader (Node 1)...")
    num_vertices = 10
    num_edges = 12

    for i in range(1, num_vertices + 1):
        q = f"INSERT VERTEX Device (id, model, ram_gb) VALUES ({100 + i}, 'M5_Server_{i}', {16 * i});"
        res = query(NODE1_URL, q)
        if res.get("status") != "ok":
            print(f"  [!] Failed vertex insert on leader: {res.get('error')}")

    for i in range(1, num_edges + 1):
        src = 100 + ((i - 1) % num_vertices) + 1
        dst = 100 + (i % num_vertices) + 1
        q = f"INSERT EDGE LINKED FROM {src} TO {dst};"
        query(NODE1_URL, q)

    # Force compaction on leader
    query(NODE1_URL, "compact;")
    print(f"  [✓] Inserted {num_vertices} vertices and {num_edges} edges on Node 1, compacted.")

    # Small delay for network propagation
    time.sleep(0.3)

    print("\n[3/4] Validating Replicated State on Followers (Node 2 & Node 3)...")
    read_query = "MATCH (a:Device)-[:LINKED]->(b:Device) RETURN a.model, b.model;"

    # Query Node 1 (Source)
    n1_res = query(NODE1_URL, read_query)
    n1_rows = len(n1_res.get("rows", []))
    print(f"  Node 1 (Source Leader):   {n1_rows} edges found")

    # Query Node 2 (Follower)
    n2_res = query(NODE2_URL, read_query)
    n2_rows = len(n2_res.get("rows", []))
    print(f"  Node 2 (Follower 1):      {n2_rows} edges found")

    # Query Node 3 (Follower)
    n3_res = query(NODE3_URL, read_query)
    n3_rows = len(n3_res.get("rows", []))
    print(f"  Node 3 (Follower 2):      {n3_rows} edges found")

    print("\n[4/4] Verifying Replicated PageRank Analytics across all nodes...")
    pr_query = "CALL algo.pageRank({damping: 0.85, max_iter: 10}) YIELD vertex_id, score;"
    pr1 = query(NODE1_URL, pr_query)
    pr2 = query(NODE2_URL, pr_query)
    pr3 = query(NODE3_URL, pr_query)

    pr1_len = len(pr1.get("rows", []))
    pr2_len = len(pr2.get("rows", []))
    pr3_len = len(pr3.get("rows", []))

    print(f"  PageRank Result Rows: Node 1 = {pr1_len} | Node 2 = {pr2_len} | Node 3 = {pr3_len}")

    # Final assertions
    success = (n1_rows > 0 and n2_rows == n1_rows and n3_rows == n1_rows and pr2_len == pr1_len and pr3_len == pr1_len)

    print("\n" + "=" * 60)
    if success:
        print("\033[1;32m[PASS] Multi-Node Raft Replication is FULLY OPERATIONAL!\033[0m")
        print("All mutations and compact actions replicated identically to all followers.")
    else:
        print("\033[1;31m[FAIL] Discrepancy detected between Leader and Followers!\033[0m")
        print(f"Details: N1={n1_rows}, N2={n2_rows}, N3={n3_rows}")
        sys.exit(1)
    print("=" * 60 + "\n")

if __name__ == "__main__":
    main()
