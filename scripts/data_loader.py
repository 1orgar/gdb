#!/usr/bin/env python3
"""
GDB High-Throughput Test Data Ingestion Script.

Populates the database with realistic graph datasets using the official `gdb-client` SDK:
- Generates vertices (User: id, name, age)
- Generates edges (FOLLOWS) with scale-free or small-world topologies
- Supports both REST HTTP batch ingestion and MPP Arrow Flight parallel scatter ingestion
- Optionally outputs a high-speed batch file for `gdb-cli -f`
"""

import argparse
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


FIRST_NAMES = [
    "Alice", "Bob", "Charlie", "Dave", "Emma", "Frank", "Grace", "Henry",
    "Ivy", "Jack", "Kate", "Liam", "Mia", "Noah", "Olivia", "Peter",
    "Quinn", "Rachel", "Sam", "Tara", "Victor", "Wendy", "Yuri", "Zoe"
]

LAST_NAMES = [
    "Smith", "Johnson", "Williams", "Brown", "Jones", "Miller", "Davis",
    "Garcia", "Rodriguez", "Wilson", "Martinez", "Anderson", "Taylor", "Thomas"
]


def generate_random_name():
    return f"{random.choice(FIRST_NAMES)}_{random.choice(LAST_NAMES)}"


def main():
    parser = argparse.ArgumentParser(description="GDB Data Ingestion Tool (via gdb-client)")
    parser.add_argument("--url", default="http://localhost:8847", help="GDB HTTP endpoint (default: http://localhost:8847)")
    parser.add_argument("--mode", choices=["http", "flight", "mpp"], default="http", help="Ingestion mode: 'http' (REST batch) or 'flight'/'mpp' (Arrow Flight scatter)")
    parser.add_argument("--client-flight-port", type=int, default=8860, help="Initial Arrow Flight client port (default: 8860)")
    parser.add_argument("--vertices", type=int, default=10000, help="Number of vertices to generate (default: 10000)")
    parser.add_argument("--edges", type=int, default=100000, help="Number of edges to generate (default: 100000)")
    parser.add_argument("--workers", type=int, default=4, help="Parallel scatter ingest workers (default: 4)")
    parser.add_argument("--file", type=str, default=None, help="Save to a .gdb batch file instead of sending over network")
    parser.add_argument("--batch-size", type=int, default=500, help="Batch size for HTTP multi-value inserts (default: 500)")
    parser.add_argument("--compact", action="store_true", default=True, help="Trigger CSR compaction after ingestion")
    parser.add_argument("--no-recreate", action="store_true", help="Do not drop and recreate schema before ingesting")
    parser.add_argument("--teardown", action="store_true", help="Drop User and FOLLOWS schema and exit")

    args = parser.parse_args()
    use_flight = args.mode in ("flight", "mpp")

    if use_flight and pl is None:
        print("\033[1;31m[!] Polars and PyArrow are required for Arrow Flight MPP mode.\033[0m")
        print("    Install them via: pip install polars pyarrow")
        sys.exit(1)

    with GdbClient(endpoint=args.url, client_flight_port=args.client_flight_port) as client:
        # Check health
        try:
            health = client.health()
            if health.get("status") != "UP":
                print(f"\033[1;31m[!] Cluster health check returned non-UP status: {health}\033[0m")
        except Exception as e:
            print(f"\033[1;31m[!] Could not connect to GDB cluster at {args.url}: {e}\033[0m")
            sys.exit(1)

        if args.teardown:
            print("[*] Tearing down graph schema (User, FOLLOWS)...")
            try:
                client.execute("DROP VERTEX User;")
            except Exception:
                pass
            try:
                client.execute("DROP EDGE FOLLOWS;")
            except Exception:
                pass
            print("[✓] Schema dropped successfully.")
            return

        print("\033[1;36m============================================================")
        print("      GDB High-Throughput Test Data Ingestion Tool          ")
        print("============================================================\033[0m")
        print(f"[*] Target Endpoint: {args.file if args.file else args.url}")
        print(f"[*] Ingest Mode:     {'Arrow Flight MPP' if use_flight else 'HTTP Batch'}")
        print(f"[*] Vertices:        {args.vertices:,}")
        print(f"[*] Edges:           {args.edges:,}")
        print(f"[*] Batch Size:      {args.batch_size:,}")
        print("------------------------------------------------------------\n")

        out_file = open(args.file, "w") if args.file else None

        # 1. Initialize Schema
        if not args.no_recreate:
            print("\033[1;33m[1/3] Recreating Graph Schema (User, FOLLOWS)...\033[0m")
            try:
                client.execute("DROP VERTEX User;")
            except Exception:
                pass
            try:
                client.execute("DROP EDGE FOLLOWS;")
            except Exception:
                pass
        else:
            print("\033[1;33m[1/3] Ensuring Graph Schema (User, FOLLOWS)...\033[0m")

        if out_file:
            out_file.write("CREATE VERTEX User (name STRING, age INT64);\n")
            out_file.write("CREATE EDGE FOLLOWS ();\n")
        else:
            try:
                client.execute("CREATE VERTEX User (name STRING, age INT64);")
            except Exception as e:
                if "already exists" not in str(e):
                    print(f"Warning creating vertex schema: {e}")
            try:
                client.execute("CREATE EDGE FOLLOWS ();")
            except Exception as e:
                if "already exists" not in str(e):
                    print(f"Warning creating edge schema: {e}")

        # 2. Ingest Vertices
        print(f"\033[1;33m[2/3] Generating & Ingesting {args.vertices:,} Vertices...\033[0m")
        start_v = time.perf_counter()

        vids = list(range(1, args.vertices + 1))
        names = [generate_random_name() for _ in range(args.vertices)]
        ages = [random.randint(18, 75) for _ in range(args.vertices)]

        if out_file:
            for i in range(0, args.vertices, args.batch_size):
                chunk_vids = vids[i:i + args.batch_size]
                chunk_names = names[i:i + args.batch_size]
                chunk_ages = ages[i:i + args.batch_size]
                rows = [f"({v}, '{n}', {a})" for v, n, a in zip(chunk_vids, chunk_names, chunk_ages)]
                out_file.write(f"INSERT VERTEX User (id, name, age) VALUES {', '.join(rows)};\n")
        elif use_flight:
            df_v = pl.DataFrame({"id": vids, "name": names, "age": ages})
            res = client.scatter_ingest_vertices(df_v, label="User", id_col="id", max_workers=args.workers)
            print(f"  -> Flight Ingest: {res.get('rows_ingested', 0):,} vertices across {res.get('partitions', 0)} partitions")
        else:
            records = [{"id": v, "name": n, "age": a} for v, n, a in zip(vids, names, ages)]
            client.insert_vertices("User", records, batch_size=args.batch_size)

        elapsed_v = time.perf_counter() - start_v
        throughput_v = args.vertices / elapsed_v if elapsed_v > 0 else 0
        print(f"\033[1;32m[✓] Vertices created in {elapsed_v:.2f}s ({throughput_v:,.0f} vertices/sec)\033[0m\n")

        # 3. Ingest Edges
        print(f"\033[1;33m[3/3] Generating & Ingesting {args.edges:,} Edges...\033[0m")
        start_e = time.perf_counter()

        influencer_bound = max(1, args.vertices // 10)
        srcs = [random.randint(1, args.vertices) for _ in range(args.edges)]
        dsts = []
        for s in srcs:
            d = random.randint(1, influencer_bound) if random.random() < 0.5 else random.randint(1, args.vertices)
            while d == s:
                d = random.randint(1, args.vertices)
            dsts.append(d)

        if out_file:
            for i in range(0, args.edges, args.batch_size):
                chunk_srcs = srcs[i:i + args.batch_size]
                chunk_dsts = dsts[i:i + args.batch_size]
                stmts = [f"INSERT EDGE FOLLOWS FROM {s} TO {d};" for s, d in zip(chunk_srcs, chunk_dsts)]
                out_file.write(" ".join(stmts) + "\n")
        elif use_flight:
            df_e = pl.DataFrame({"src": srcs, "dst": dsts})
            res = client.scatter_ingest_edges(df_e, edge_type="FOLLOWS", src_col="src", dst_col="dst", max_workers=args.workers)
            print(f"  -> Flight Ingest: {res.get('rows_ingested', 0):,} edges across {res.get('partitions', 0)} partitions")
        else:
            edge_tuples = list(zip(srcs, dsts))
            client.insert_edges("FOLLOWS", edge_tuples, batch_size=args.batch_size)

        elapsed_e = time.perf_counter() - start_e
        throughput_e = args.edges / elapsed_e if elapsed_e > 0 else 0
        print(f"\033[1;32m[✓] Edges created in {elapsed_e:.2f}s ({throughput_e:,.0f} edges/sec)\033[0m\n")

        # 4. Compaction
        if args.compact:
            print("\033[1;33m[*] Compacting Delta buffer into cache-aligned Chunked-CSR...\033[0m")
            if out_file:
                out_file.write("compact;\n")
            else:
                comp_res = client.compact()
                print(f"\033[1;32m[✓] CSR Compaction completed in {comp_res.get('elapsed_us', 0) / 1000.0:.2f}ms\033[0m\n")

        if out_file:
            out_file.close()
            print(f"\033[1;32m[✓] Saved batch dataset to {args.file}!\033[0m")
            print(f"    Load it into GDB via: ./bin/gdb-cli -f {args.file}")
        else:
            print("\033[1;32m[✓] Data ingestion complete! Graph is ready for analytical calculations.\033[0m")


if __name__ == "__main__":
    main()
