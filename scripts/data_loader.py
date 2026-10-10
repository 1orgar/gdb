#!/usr/bin/env python3
"""
GDB High-Throughput Test Data Ingestion Script.

Populates the database with realistic graph datasets:
- Generates vertices (User: id, name, age)
- Generates edges (FOLLOWS / KNOWS) with scale-free or small-world topologies
- Streams directly via HTTP REST API or outputs a high-speed batch file for gdb-cli
"""

import argparse
import json
import random
import sys
import time
import urllib.request
import urllib.error

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

def send_query(base_url, query_str):
    url = f"{base_url}/query"
    data = query_str.encode('utf-8')
    req = urllib.request.Request(url, data=data, headers={'Content-Type': 'text/plain'})
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            body = resp.read().decode('utf-8')
            return json.loads(body)
    except urllib.error.URLError as e:
        print(f"\x1b[1;31mError connecting to {url}: {e}\x1b[0m")
        sys.exit(1)

def main():
    parser = argparse.ArgumentParser(description="GDB Data Ingestion Tool")
    parser.add_argument("--url", default="http://localhost:8847", help="GDB HTTP endpoint (default: http://localhost:8847)")
    parser.add_argument("--vertices", type=int, default=10000, help="Number of vertices to generate (default: 10000)")
    parser.add_argument("--edges", type=int, default=100000, help="Number of edges to generate (default: 100000)")
    parser.add_argument("--file", type=str, default=None, help="Save to a .gdb batch file instead of sending over HTTP")
    parser.add_argument("--batch-size", type=int, default=500, help="Batch size for multi-value inserts (default: 500)")
    parser.add_argument("--compact", action="store_true", default=True, help="Trigger CSR compaction after ingestion")
    parser.add_argument("--no-recreate", action="store_true", help="Do not drop and recreate schema before ingesting")
    parser.add_argument("--teardown", action="store_true", help="Drop User and FOLLOWS schema and exit")

    args = parser.parse_args()

    if args.teardown:
        print("[*] Tearing down graph schema (User, FOLLOWS)...")
        send_query(args.url, "DROP VERTEX User;")
        send_query(args.url, "DROP EDGE FOLLOWS;")
        print("[✓] Schema dropped successfully.")
        return

    print("\x1b[1;36m")
    print("============================================================")
    print("      GDB High-Throughput Test Data Ingestion Tool          ")
    print("============================================================\x1b[0m")
    print(f"[*] Target:      {args.file if args.file else args.url}")
    print(f"[*] Vertices:    {args.vertices:,}")
    print(f"[*] Edges:       {args.edges:,}")
    print(f"[*] Batch Size:  {args.batch_size:,}")
    print("------------------------------------------------------------\n")

    out_file = open(args.file, "w") if args.file else None

    def emit(statement):
        if out_file:
            out_file.write(statement + "\n")
        else:
            res = send_query(args.url, statement)
            if res.get("status") == "error":
                err = res.get("error", "")
                if "already exists" not in err and "not found" not in err:
                    print(f"\x1b[1;31m[!] Query failed: {err}\x1b[0m")

    # 1. Initialize Schema
    if not args.no_recreate:
        print("\x1b[1;33m[1/3] Recreating Graph Schema (User, FOLLOWS)...\x1b[0m")
        emit("DROP VERTEX User;")
        emit("DROP EDGE FOLLOWS;")
    else:
        print("\x1b[1;33m[1/3] Ensuring Graph Schema (User, FOLLOWS)...\x1b[0m")
    emit("CREATE VERTEX User (name STRING, age INT64);")
    emit("CREATE EDGE FOLLOWS ();")

    # 2. Ingest Vertices in Multi-Row Batches
    print(f"\x1b[1;33m[2/3] Generating & Ingesting {args.vertices:,} Vertices (batch_size={args.batch_size})...\x1b[0m")
    start_v = time.time()
    v_buffer = []

    for vid in range(1, args.vertices + 1):
        name = generate_random_name()
        age = random.randint(18, 75)
        v_buffer.append(f"({vid}, '{name}', {age})")

        if len(v_buffer) >= args.batch_size or vid == args.vertices:
            batch_query = f"INSERT VERTEX User (id, name, age) VALUES {', '.join(v_buffer)};"
            emit(batch_query)
            v_buffer.clear()

            if vid % 10000 == 0 or vid == args.vertices:
                pct = (vid / args.vertices) * 100
                print(f"  -> Vertices: {vid:,} / {args.vertices:,} ({pct:.1f}%)")

    elapsed_v = time.time() - start_v
    print(f"\x1b[1;32m[✓] Vertices created in {elapsed_v:.2f}s ({args.vertices / elapsed_v:,.0f} vertices/sec)\x1b[0m\n")

    # 3. Ingest Edges in Script Batches
    print(f"\x1b[1;33m[3/3] Generating & Ingesting {args.edges:,} Edges (batch_size={args.batch_size})...\x1b[0m")
    start_e = time.time()
    
    influencer_bound = max(1, args.vertices // 10)
    e_buffer = []

    for i in range(args.edges):
        src = random.randint(1, args.vertices)
        if random.random() < 0.5:
            dst = random.randint(1, influencer_bound)
        else:
            dst = random.randint(1, args.vertices)
        
        while dst == src:
            dst = random.randint(1, args.vertices)

        e_buffer.append(f"INSERT EDGE FOLLOWS FROM {src} TO {dst};")

        if len(e_buffer) >= args.batch_size or (i + 1) == args.edges:
            batch_script = " ".join(e_buffer)
            emit(batch_script)
            e_buffer.clear()

            if (i + 1) % 20000 == 0 or (i + 1) == args.edges:
                pct = ((i + 1) / args.edges) * 100
                print(f"  -> Edges: {i + 1:,} / {args.edges:,} ({pct:.1f}%)")

    elapsed_e = time.time() - start_e
    print(f"\x1b[1;32m[✓] Edges created in {elapsed_e:.2f}s ({args.edges / elapsed_e:,.0f} edges/sec)\x1b[0m\n")

    # 4. Compaction
    if args.compact:
        print("\x1b[1;33m[*] Compacting Delta buffer into cache-aligned Chunked-CSR...\x1b[0m")
        if out_file:
            out_file.write("compact;\n")
        else:
            c_url = f"{args.url}/compact"
            req = urllib.request.Request(c_url, data=b"{}", headers={'Content-Type': 'application/json'})
            with urllib.request.urlopen(req) as resp:
                body = json.loads(resp.read().decode('utf-8'))
                print(f"\x1b[1;32m[✓] CSR Compaction completed in {body.get('elapsed_us', 0) / 1000:.2f}ms\x1b[0m\n")

    if out_file:
        out_file.close()
        print(f"\x1b[1;32m[✓] Saved batch dataset to {args.file}!\x1b[0m")
        print(f"    You can load it into GDB anytime via: ./bin/gdb-cli -f {args.file}")
    else:
        print("\x1b[1;32m[✓] Data ingestion complete! Graph is ready for analytical calculations.\x1b[0m")

if __name__ == "__main__":
    main()
