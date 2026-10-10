#!/usr/bin/env python3
"""
SNAP Graph Dataset Ingestion with GDB Official Python Client
============================================================
Fast ingestion of Stanford SNAP graph datasets (e.g. soc-Epinions1.txt,
com-LiveJournal, twitter-social-network) into GDB using the official `gdb-client`.

Supports:
1. Ultra-high-speed Arrow Flight parallel scatter-ingest via Polars.
2. Standard HTTP REST batch-ingest with automatic chunking.

Usage:
    # 1. Download dataset:
    wget https://snap.stanford.edu/data/soc-Epinions1.txt.gz
    gzip -d soc-Epinions1.txt.gz

    # 2. Ingest via Arrow Flight (fastest, direct streaming to ring nodes):
    python3 examples/load_snap_dataset.py --file soc-Epinions1.txt --mode flight

    # 3. Or ingest via HTTP REST batches:
    python3 examples/load_snap_dataset.py --file soc-Epinions1.txt --mode rest
"""

import argparse
import os
import sys
import time

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


def main():
    parser = argparse.ArgumentParser(description="Ingest SNAP datasets into GDB using official gdb-client")
    parser.add_argument("--file", type=str, required=True, help="Path to SNAP edge list file (e.g. soc-Epinions1.txt)")
    parser.add_argument("--endpoint", default="http://localhost:8847", help="GDB HTTP endpoint (default: http://localhost:8847)")
    parser.add_argument("--vertex-label", default="User", help="Vertex label (default: User)")
    parser.add_argument("--edge-type", default="TRUSTS", help="Relationship edge type (default: TRUSTS)")
    parser.add_argument("--mode", choices=["flight", "rest"], default="flight" if pl is not None else "rest",
                        help="Ingest mode: flight (Arrow Flight streaming) or rest (HTTP batches)")
    parser.add_argument("--batch-size", type=int, default=1000, help="Batch size for REST mode (default: 1000)")
    parser.add_argument("--no-compact", action="store_true", help="Skip automatic CSR compaction after ingest")
    args = parser.parse_args()

    if not os.path.exists(args.file):
        print(f"[-] Error: File not found: {args.file}")
        sys.exit(1)

    print("=" * 65)
    print("      GDB SNAP Dataset Ingestion Tool (gdb-client)            ")
    print("=" * 65)
    print(f"[*] Dataset:       {args.file}")
    print(f"[*] Target GDB:    {args.endpoint}")
    print(f"[*] Ingest Mode:   {args.mode.upper()}")
    print(f"[*] Schema:        ({args.vertex_label})-[:{args.edge_type}]->({args.vertex_label})")
    print("-" * 65)

    with GdbClient(endpoint=args.endpoint) as client:
        # 1. Verify connection
        try:
            health = client.health()
            print(f"[✓] Connected to GDB: {health.get('status')} (version: {health.get('version', '0.5.1')})")
        except Exception as e:
            print(f"[-] Failed to connect to GDB at {args.endpoint}: {e}")
            sys.exit(1)

        # 2. Register Schema
        print(f"[*] Registering vertex '{args.vertex_label}' and edge '{args.edge_type}'...")
        client.execute_script(f"""
            CREATE VERTEX {args.vertex_label} ();
            CREATE EDGE {args.edge_type} ();
        """)

        # 3. Read and Ingest Data
        start_time = time.perf_counter()

        if args.mode == "flight":
            if pl is None:
                print("[-] Error: 'flight' mode requires polars and pyarrow. Install via: pip install 'gdb-client[all]'")
                sys.exit(1)

            print(f"[*] Reading '{args.file}' using Polars SIMD TSV parser...")
            t_read = time.perf_counter()
            df = pl.read_csv(
                args.file,
                separator="\t",
                comment_prefix="#",
                has_header=False,
                new_columns=["src", "dst"],
                schema={"src": pl.Int64, "dst": pl.Int64},
            )
            read_sec = time.perf_counter() - t_read
            print(f"[✓] Loaded {len(df):,} edges into RAM in {read_sec:.2f}s.")

            # Ingest Unique Vertices
            print(f"[*] Extracting unique vertices...")
            vertices_df = pl.concat([
                df.select(pl.col("src").alias("id")),
                df.select(pl.col("dst").alias("id")),
            ]).unique()
            print(f"[*] Streaming {len(vertices_df):,} vertices via Arrow Flight scatter-ingest...")
            v_res = client.scatter_ingest_vertices(vertices_df, label=args.vertex_label, id_col="id")
            print(f"    Ingested {v_res['rows_ingested']:,} vertices in {v_res['elapsed_seconds']}s ({v_res['rows_per_second']:,.0f} rows/s)")

            # Ingest Edges
            print(f"[*] Streaming {len(df):,} edges via Arrow Flight scatter-ingest...")
            e_res = client.scatter_ingest_edges(df, edge_type=args.edge_type, src_col="src", dst_col="dst")
            print(f"    Ingested {e_res['rows_ingested']:,} edges in {e_res['elapsed_seconds']}s ({e_res['rows_per_second']:,.0f} rows/s)")

        else:
            # REST batch mode
            print(f"[*] Parsing '{args.file}' and streaming via REST multi-row statements...")
            edges = []
            vertex_ids = set()
            with open(args.file, "r") as f:
                for line in f:
                    line = line.strip()
                    if not line or line.startswith("#"):
                        continue
                    parts = line.split()
                    if len(parts) >= 2:
                        u, v = int(parts[0]), int(parts[1])
                        edges.append((u, v))
                        vertex_ids.add(u)
                        vertex_ids.add(v)

            print(f"[✓] Parsed {len(edges):,} edges and {len(vertex_ids):,} unique vertices.")

            print(f"[*] Ingesting vertices in batches of {args.batch_size}...")
            v_records = [{"id": vid} for vid in vertex_ids]
            client.insert_vertices(args.vertex_label, v_records, batch_size=args.batch_size)

            print(f"[*] Ingesting edges in batches of {args.batch_size}...")
            client.insert_edges(args.edge_type, edges, batch_size=args.batch_size)

        total_elapsed = time.perf_counter() - start_time
        print(f"\n[✓] Ingestion completed in {total_elapsed:.2f}s.")

        # 4. Trigger CSR Compaction
        if not args.no_compact:
            print("[*] Triggering CSR compaction for optimal graph traversal...")
            client.compact()
            time.sleep(0.5)

        # 5. Compute CBO Statistics
        print("[*] Running ANALYZE GRAPH for Cost-Based Optimizer...")
        cbo_res = client.analyze()
        print(f"[✓] {cbo_res.message}")

        # 6. Verify with Sample Query & PageRank
        print("\n[*] Running test query (top 5 degree vertices):")
        test_res = client.query(f"""
            MATCH (a:{args.vertex_label})-[:{args.edge_type}]->(b:{args.vertex_label})
            RETURN a.id, count(b) AS degree
            LIMIT 5;
        """)
        if pl is not None:
            print(test_res.to_polars())
        else:
            for row in test_res:
                print(f"    ID: {row[0]} | Degree: {row[1]}")

        # 7. Check Cluster Memory & Edge Counts
        stats = client.resources()
        print(f"\n[+] Cluster Live Memory RSS: {stats.get('memory_resident_mb', 0):.2f} MB")
        print(f"[+] Total CSR Edges:        {stats.get('total_edges', 0):,}")


if __name__ == "__main__":
    main()
