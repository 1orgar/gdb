#!/usr/bin/env python3
"""
GDB Concurrent Stress Test Suite (Powered by gdb-client)
Generates high-concurrency mixed OLTP writes and OLAP graph queries
against GDB Cluster, measuring latency distribution and Prometheus metrics delta.
"""

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
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


def teardown_schema(client: GdbClient, silent: bool = False):
    if not silent:
        print("[*] Cleaning up test schemas (User, Device, KNOWS, LINKED)...")
    for s in ["DROP VERTEX User;", "DROP VERTEX Device;", "DROP EDGE KNOWS;", "DROP EDGE LINKED;"]:
        try:
            client.execute(s)
        except Exception:
            pass


def setup_schema(client: GdbClient):
    print("[*] Recreating fresh test schema (User, Device, KNOWS, LINKED)...")
    teardown_schema(client, silent=True)
    stmts = [
        "CREATE VERTEX User (name STRING, age INT64);",
        "CREATE VERTEX Device (model STRING);",
        "CREATE EDGE KNOWS ();",
        "CREATE EDGE LINKED ();",
    ]
    for s in stmts:
        try:
            client.execute(s)
        except Exception:
            pass

    # Pre-seed 100 base vertices
    rows = []
    for vid in range(1, 101):
        rows.append(f"({vid}, 'User_{vid}', {20 + (vid % 40)})")
    client.execute(f"INSERT VERTEX User (id, name, age) VALUES {', '.join(rows)};")


def main():
    parser = argparse.ArgumentParser(description="GDB Concurrent Stress Test Suite (via gdb-client)")
    parser.add_argument("--endpoint", default="http://127.0.0.1:8847", help="Target server endpoint")
    parser.add_argument("--mode", choices=["http", "flight", "mpp"], default="http", help="Query transport mode: 'http' or 'flight'/'mpp'")
    parser.add_argument("--client-flight-port", type=int, default=8860, help="Initial Flight client port")
    parser.add_argument("--threads", type=int, default=8, help="Number of concurrent worker threads")
    parser.add_argument("--duration", type=int, default=10, help="Duration of stress test in seconds")
    parser.add_argument("--read-ratio", type=float, default=0.7, help="Ratio of read vs write queries (default: 0.7)")
    parser.add_argument("--timeout", type=float, default=10.0, help="Query timeout in seconds")
    parser.add_argument("--keep-schema", action="store_true", help="Preserve test schemas after test")
    args = parser.parse_args()
    use_flight = args.mode in ("flight", "mpp")

    print("\033[1;36m" + "=" * 68)
    print("      GDB HIGH-CONCURRENCY HTAP STRESS TEST (gdb-client)          ")
    print("=" * 68 + "\033[0m")
    print(f"[*] Target Endpoint:  {args.endpoint}")
    print(f"[*] Transport Mode:   {'Arrow Flight MPP' if use_flight else 'HTTP REST'}")
    print(f"[*] Concurrency:      {args.threads} worker threads")
    print(f"[*] Duration:         {args.duration}s")
    print(f"[*] Read/Write Ratio: {int(args.read_ratio * 100)}% reads / {int((1 - args.read_ratio) * 100)}% writes")
    print("--------------------------------------------------------------------\n")

    with GdbClient(endpoint=args.endpoint, client_flight_port=args.client_flight_port, timeout=args.timeout) as client:
        # Check health
        try:
            health = client.health()
            print(f"[✓] Connected: Status={health.get('status')} | Version={health.get('version')} | Role={health.get('role')}\n")
        except Exception as e:
            print(f"\033[1;31m[!] Failed to connect to GDB at {args.endpoint}: {e}\033[0m")
            sys.exit(1)

        setup_schema(client)

        stop_time = time.time() + args.duration
        latencies = []
        errors = 0
        total_queries = 0

        def worker_fn(worker_id):
            nonlocal errors, total_queries
            local_lats = []
            local_errs = 0
            count = 0
            next_id = worker_id * 1000000 + 200

            while time.time() < stop_time:
                is_read = random.random() < args.read_ratio
                if is_read:
                    vid = random.randint(1, 100)
                    q = f"MATCH (u:User) WHERE u.id = {vid} RETURN u.name;"
                else:
                    next_id += 1
                    q = f"INSERT VERTEX User (id, name, age) VALUES ({next_id}, 'Stress_{next_id}', 30);"

                t0 = time.perf_counter()
                try:
                    res = client.query(q, use_flight=use_flight)
                    elapsed = (time.perf_counter() - t0) * 1000.0
                    if res.is_ok:
                        local_lats.append(elapsed)
                    else:
                        local_errs += 1
                except Exception:
                    local_errs += 1

                count += 1

            return local_lats, local_errs, count

        start_bench = time.perf_counter()
        with ThreadPoolExecutor(max_workers=args.threads) as pool:
            futures = [pool.submit(worker_fn, i) for i in range(args.threads)]
            for f in as_completed(futures):
                lats, errs, cnt = f.result()
                latencies.extend(lats)
                errors += errs
                total_queries += cnt

        total_elapsed = time.perf_counter() - start_bench
        qps = total_queries / total_elapsed if total_elapsed > 0 else 0

        latencies.sort()
        p50 = latencies[int(len(latencies) * 0.50)] if latencies else 0
        p95 = latencies[int(len(latencies) * 0.95)] if latencies else 0
        p99 = latencies[int(len(latencies) * 0.99)] if latencies else 0
        avg = sum(latencies) / len(latencies) if latencies else 0

        if not args.keep_schema:
            teardown_schema(client)

        print("=" * 68)
        print("                   STRESS TEST RESULTS                           ")
        print("=" * 68)
        print(f"Total Requests:     {total_queries:,} across {args.threads} threads ({total_elapsed:.2f}s)")
        print(f"Overall Throughput: {qps:>9,.1f} requests/sec (QPS)")
        print(f"Success Rate:       {(total_queries - errors) / total_queries * 100:.2f}% ({errors} errors)")
        print(f"Latency P50:        {p50:.2f} ms")
        print(f"Latency P95:        {p95:.2f} ms")
        print(f"Latency P99:        {p99:.2f} ms")
        print(f"Latency Avg:        {avg:.2f} ms")
        print("=" * 68 + "\n")


if __name__ == "__main__":
    main()
