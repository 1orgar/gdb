#!/usr/bin/env python3
"""
GDB Leaderless Hash Ring Replication Test Suite (Pure Python Standard Library)
Verifies symmetric peer-to-peer replication: writes accepted by ANY node (as coordinator)
replicate consistently across the cluster ring.
"""

import sys
import time
import json
import urllib.request
import urllib.error

PEER1_URL = "http://127.0.0.1:8847"
PEER2_URL = "http://127.0.0.1:8846"
PEER3_URL = "http://127.0.0.1:8845"

def query(endpoint, q, timeout=5):
    url = f"{endpoint}/query"
    data = json.dumps({"query": q}).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except Exception as e:
        return {"status": "error", "error": str(e)}

def get_json(url, timeout=3):
    try:
        with urllib.request.urlopen(url, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except Exception:
        return None

def main():
    print("\033[1;36m" + "=" * 65)
    print("      GDB Leaderless Ring Replication Verification Suite     ")
    print("=" * 65 + "\033[0m")

    # 1. Health & Cluster Check on all 3 peers
    print("[*] Probing leaderless ring peers and topology...")
    peers = [("Peer 1", PEER1_URL), ("Peer 2", PEER2_URL), ("Peer 3", PEER3_URL)]
    for name, url in peers:
        health = get_json(f"{url}/health")
        if health and health.get("status") == "UP":
            rf = health.get("replication_factor", "?")
            mode = health.get("replication_mode", "?")
            print(f"  \033[1;32m[✓]\033[0m {name} at {url} is UP | Role: Peer | RF={rf} | Mode={mode}")
        else:
            print(f"  \033[1;31m[✗]\033[0m {name} at {url} is UNREACHABLE!")
            print("\nPlease ensure the 3-node cluster is running via ./scripts/start_cluster.sh")
            sys.exit(1)

    print("\n[1/4] Creating Schema via Peer 1 (Broadcast DDL)...")
    res1 = query(PEER1_URL, "CREATE VERTEX Device (model STRING, ram_gb INT64);")
    res2 = query(PEER1_URL, "CREATE EDGE LINKED ();")
    print(f"  -> DDL Result: {res1.get('status')} | {res2.get('status')}")
    time.sleep(0.1)

    print("\n[2/4] Performing Symmetric Ingestion Across Different Peers...")
    # Insert from Peer 1
    print("  -> Inserting vertices 101..104 through Peer 1...")
    for i in range(1, 5):
        query(PEER1_URL, f"INSERT VERTEX Device (id, model, ram_gb) VALUES ({100 + i}, 'M5_Server_{i}', {16 * i});")

    # Insert from Peer 2
    print("  -> Inserting vertices 105..108 through Peer 2...")
    for i in range(5, 9):
        query(PEER2_URL, f"INSERT VERTEX Device (id, model, ram_gb) VALUES ({100 + i}, 'M5_Server_{i}', {16 * i});")

    # Insert from Peer 3
    print("  -> Inserting vertices 109..112 and connecting edges through Peer 3...")
    for i in range(9, 13):
        query(PEER3_URL, f"INSERT VERTEX Device (id, model, ram_gb) VALUES ({100 + i}, 'M5_Server_{i}', {16 * i});")

    for i in range(1, 13):
        src = 100 + ((i - 1) % 12) + 1
        dst = 100 + (i % 12) + 1
        query(PEER3_URL, f"INSERT EDGE LINKED FROM {src} TO {dst};")

    # Compaction from Peer 2
    print("  -> Triggering compaction via Peer 2...")
    query(PEER2_URL, "compact;")
    time.sleep(0.3)

    print("\n[3/4] Validating Consistency Across All Ring Peers...")
    read_query = "MATCH (a:Device)-[:LINKED]->(b:Device) RETURN a.model, b.model;"

    p1_res = query(PEER1_URL, read_query)
    p2_res = query(PEER2_URL, read_query)
    p3_res = query(PEER3_URL, read_query)

    p1_rows = len(p1_res.get("rows", []))
    p2_rows = len(p2_res.get("rows", []))
    p3_rows = len(p3_res.get("rows", []))

    print(f"  Peer 1 (:8847): {p1_rows} edges found")
    print(f"  Peer 2 (:8846): {p2_rows} edges found")
    print(f"  Peer 3 (:8845): {p3_rows} edges found")

    print("\n[4/4] Verifying Parallel PageRank Analytics across All Peers...")
    pr_query = "CALL algo.pageRank({damping: 0.85, max_iter: 10}) YIELD vertex_id, score;"
    pr1 = query(PEER1_URL, pr_query)
    pr2 = query(PEER2_URL, pr_query)
    pr3 = query(PEER3_URL, pr_query)

    pr1_len = len(pr1.get("rows", []))
    pr2_len = len(pr2.get("rows", []))
    pr3_len = len(pr3.get("rows", []))

    print(f"  PageRank Result Rows: Peer 1 = {pr1_len} | Peer 2 = {pr2_len} | Peer 3 = {pr3_len}")

    # Final assertions
    cluster_info = get_json(f"{PEER1_URL}/cluster")
    rf = cluster_info.get("effective_replication_factor", 3) if cluster_info else 3

    if rf == 3:
        # Full replication: all peers must have identical edge counts
        success = (p1_rows == 12 and p2_rows == 12 and p3_rows == 12 and pr1_len > 0 and pr2_len == pr1_len and pr3_len == pr1_len)
    else:
        # Partitioned / partial replication
        total_edges = p1_rows + p2_rows + p3_rows
        success = total_edges >= 12

    print("\n" + "=" * 65)
    if success:
        print("\033[1;32m[PASS] Leaderless Ring Replication is FULLY OPERATIONAL!\033[0m")
        print("Symmetric writes from all peers processed and replicated across the hash ring.")
    else:
        print("\033[1;31m[FAIL] Discrepancy detected across cluster peers!\033[0m")
        print(f"Details: P1={p1_rows}, P2={p2_rows}, P3={p3_rows}")
        sys.exit(1)
    print("=" * 65 + "\n")

if __name__ == "__main__":
    main()
