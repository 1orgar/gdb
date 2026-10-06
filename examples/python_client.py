#!/usr/bin/env python3
"""
GDB Python Client Example
=========================
Demonstrates how to interact with a GDB cluster using standard Python 3.
Zero external dependencies required (uses urllib and json from stdlib).
"""

import json
import urllib.request
import urllib.error
import sys

GDB_ENDPOINT = "http://localhost:8847"

def execute_query(endpoint: str, query: str) -> dict:
    url = f"{endpoint}/query"
    payload = json.dumps({"query": query}).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=payload,
        headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read().decode("utf-8"))

def fetch_json(endpoint: str, path: str) -> dict:
    url = f"{endpoint}{path}"
    with urllib.request.urlopen(url) as resp:
        return json.loads(resp.read().decode("utf-8"))

def print_table(columns: list, rows: list):
    if not columns or not rows:
        print("  (Empty result set)")
        return
    col_widths = [len(str(c)) for c in columns]
    for row in rows:
        for i, val in enumerate(row):
            col_widths[i] = max(col_widths[i], len(str(val)))
    
    header = " | ".join(f"{str(c):<{col_widths[i]}}" for i, c in enumerate(columns))
    sep = "-+-".join("-" * col_widths[i] for i in range(len(columns)))
    print(f"  {header}")
    print(f"  {sep}")
    for row in rows:
        line = " | ".join(f"{str(v):<{col_widths[i]}}" for i, v in enumerate(row))
        print(f"  {line}")

def main():
    print("=" * 60)
    print("       GDB Python Client Example (Stdlib HTTP REST)         ")
    print("=" * 60)

    # 1. Check Health & Cluster Status
    try:
        health = fetch_json(GDB_ENDPOINT, "/health")
        print(f"[+] Cluster Health: {health.get('status')} (version: {health.get('version')})")
    except Exception as e:
        print(f"[-] Failed to connect to GDB at {GDB_ENDPOINT}: {e}")
        print("    Make sure the cluster is running: ./scripts/start_cluster.sh")
        sys.exit(1)

    cluster_info = fetch_json(GDB_ENDPOINT, "/cluster")
    print(f"[+] Active Nodes:   {cluster_info.get('active_nodes', 1)} | Role: {cluster_info.get('role')}")

    gpu_info = fetch_json(GDB_ENDPOINT, "/gpu")
    print(f"[+] GPU Hardware:   {gpu_info.get('backend', 'N/A')} (Available: {gpu_info.get('available')})")

    # 2. DDL - Create Schema
    print("\n[+] Creating schema...")
    execute_query(GDB_ENDPOINT, "CREATE VERTEX Product (name STRING, price FLOAT64);")
    execute_query(GDB_ENDPOINT, "CREATE EDGE BOUGHT_WITH ();")

    # 3. DML - Insert Data
    print("[+] Inserting product vertices & co-purchase edges...")
    products = [
        (1001, "MacBook Pro", 2499.0),
        (1002, "Studio Display", 1599.0),
        (1003, "Magic Keyboard", 199.0),
        (1004, "Magic Trackpad", 149.0),
    ]
    for pid, name, price in products:
        execute_query(GDB_ENDPOINT, f"INSERT VERTEX Product (id, name, price) VALUES ({pid}, '{name}', {price});")

    edges = [
        (1001, 1002),
        (1001, 1003),
        (1001, 1004),
        (1003, 1004),
    ]
    for u, v in edges:
        execute_query(GDB_ENDPOINT, f"INSERT EDGE BOUGHT_WITH FROM {u} TO {v};")

    # 4. Compact into Chunked-CSR
    execute_query(GDB_ENDPOINT, "compact;")
    print("[✓] Data compacted into high-performance Chunked-CSR.")

    # 5. openCypher Query
    print("\n[+] Query: Co-purchased items (1-Hop Traversal):")
    res = execute_query(GDB_ENDPOINT, "MATCH (p:Product)-[:BOUGHT_WITH]->(related:Product) RETURN p.name, related.name;")
    print_table(res.get("columns", []), res.get("rows", []))

    # 6. Graph Analytics: PageRank
    print("\n[+] Algorithm: PageRank influence scores:")
    pr_res = execute_query(GDB_ENDPOINT, "CALL algo.pageRank({damping: 0.85, max_iter: 10}) YIELD vertex_id, score;")
    print_table(pr_res.get("columns", []), pr_res.get("rows", []))

    # 7. Graph Analytics: Triangle Count
    print("\n[+] Algorithm: Triangle counting (dense product clusters):")
    tri_res = execute_query(GDB_ENDPOINT, "CALL algo.triangleCount() YIELD vertex_id, triangles;")
    print_table(tri_res.get("columns", []), tri_res.get("rows", []))

    # 8. Resources
    resources = fetch_json(GDB_ENDPOINT, "/resources")
    print(f"\n[+] Cluster Memory RSS: {resources.get('memory_resident_mb', 0):.2f} MB")
    print(f"[+] Total Edges:        {resources.get('total_edges', 0)}")
    print("\n[✓] GDB Python Client Example completed successfully!")

if __name__ == "__main__":
    main()
