#!/usr/bin/env python3
"""
Official GDB Python Client Example
==================================
Demonstrates how to interact with a GDB cluster using the official `gdb-client` SDK:
- Cluster topology discovery and GPU status inspection
- Multi-statement DDL/DML schema setup
- Batch inserting vertices and edges
- Executing openCypher queries with Polars, Pandas, and NetworkX exports
- Running built-in Graph Analytics algorithms (PageRank, Triangle Counting)
- Cost-Based Optimizer (CBO) graph analysis
"""

import sys
import os

try:
    from gdb_client import GdbClient
except ImportError:
    sibling_sdk = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "gdb-py-client", "src"))
    if os.path.exists(sibling_sdk):
        sys.path.insert(0, sibling_sdk)
    try:
        from gdb_client import GdbClient
    except ImportError:
        print("[-] Could not import official `gdb_client` package.")
        print("    Install it via: pip install gdb-client")
        sys.exit(1)

try:
    import polars as pl
except ImportError:
    pl = None

try:
    import networkx as nx
except ImportError:
    nx = None


def main():
    endpoint = os.environ.get("GDB_ENDPOINT", "http://localhost:8847")
    print("=" * 65)
    print("        GDB Official Python Client SDK Example                   ")
    print("=" * 65)

    with GdbClient(endpoint=endpoint) as client:
        # 1. Health & Cluster Topology
        try:
            health = client.health()
            print(f"[+] Cluster Health: {health.get('status')} (version: {health.get('version', '0.5.1')})")
        except Exception as e:
            print(f"[-] Failed to connect to GDB at {endpoint}: {e}")
            print("    Please start the server first: ./bin/gdb-server")
            sys.exit(1)

        cluster_info = client.cluster()
        print(f"[+] Active Ring Nodes: {cluster_info.get('active_nodes', len(client.topology.nodes))}")

        gpu_info = client.gpu()
        print(f"[+] GPU Hardware:      {gpu_info.get('backend', 'N/A')} (Available: {gpu_info.get('available')})")

        # 2. Schema Setup via Multi-Statement Script
        print("\n[+] Registering schema via multi-statement Cypher script...")
        client.execute_script("""
            CREATE VERTEX Product (name STRING, price FLOAT64);
            CREATE EDGE BOUGHT_WITH ();
        """)

        # 3. Batch Ingest Vertices
        print("[+] Batch inserting product vertices...")
        products = [
            {"id": 1001, "name": "MacBook Pro", "price": 2499.0},
            {"id": 1002, "name": "Studio Display", "price": 1599.0},
            {"id": 1003, "name": "Magic Keyboard", "price": 199.0},
            {"id": 1004, "name": "Magic Trackpad", "price": 149.0},
            {"id": 1005, "name": "AirPods Max", "price": 549.0},
        ]
        num_v = client.insert_vertices("Product", products, batch_size=50)
        print(f"    Inserted {num_v} vertices.")

        # 4. Batch Ingest Edges
        print("[+] Batch inserting co-purchase relationship edges...")
        edges = [
            (1001, 1002),
            (1001, 1003),
            (1001, 1004),
            (1003, 1004),
            (1001, 1005),
        ]
        num_e = client.insert_edges("BOUGHT_WITH", edges, batch_size=50)
        print(f"    Inserted {num_e} edges.")

        # 5. Trigger CSR Compaction
        client.compact()
        print("[✓] Compaction triggered for immutable Chunked-CSR indexing.")

        # 6. Execute openCypher Query & Export to Polars
        print("\n[+] Executing 1-Hop Traversal Query:")
        query_str = "MATCH (p:Product)-[:BOUGHT_WITH]->(related:Product) RETURN p.name, related.name;"
        res = client.query(query_str)
        print(f"    Rows: {len(res)} | Latency: {res.elapsed_us / 1000.0:.2f}ms")
        if pl is not None:
            df = res.to_polars()
            print("--- Polars DataFrame ---")
            print(df)
        else:
            for row in res:
                print(f"    {row[0]} -> {row[1]}")

        # 7. Export Query Subgraph to NetworkX
        if nx is not None:
            graph_query = "MATCH (a:Product)-[:BOUGHT_WITH]->(b:Product) RETURN a.id, b.id;"
            g_res = client.query(graph_query)
            G = g_res.to_networkx()
            print(f"\n[+] NetworkX DiGraph: {G.number_of_nodes()} nodes, {G.number_of_edges()} edges")

        # 8. Run In-DB Graph Analytics Algorithms
        print("\n[+] Running Graph Analytics Algorithms:")
        pr_res = client.query("CALL algo.pageRank({damping: 0.85, max_iter: 10}) YIELD vertex_id, score;")
        print(f"    [✓] PageRank scored {len(pr_res)} vertices.")

        tri_res = client.query("CALL algo.triangleCount() YIELD vertex_id, triangles;")
        print(f"    [✓] Triangle counting completed for {len(tri_res)} vertices.")

        # 9. Cost-Based Optimizer (CBO) Graph Analysis
        cbo_res = client.analyze()
        print(f"\n[+] CBO Statistics: {cbo_res.message}")

        # 10. Live Resources & CSR Telemetry
        resources = client.resources()
        print(f"[+] Cluster Memory RSS: {resources.get('memory_resident_mb', 0):.2f} MB")
        print(f"[+] Total Edges:        {resources.get('total_edges', 0)}")

    print("\n[✓] GDB Python Client SDK Example completed successfully!")


if __name__ == "__main__":
    main()
