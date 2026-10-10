#!/usr/bin/env python3
"""
GDB Leaderless Hash Ring Replication Test Suite (Powered by gdb-client)
Verifies symmetric peer-to-peer replication: writes accepted by ANY node (as coordinator)
replicate consistently across the cluster ring.
"""

import argparse
import os
import sys
import time

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

PEER1_URL = "http://127.0.0.1:8847"
PEER2_URL = "http://127.0.0.1:8846"
PEER3_URL = "http://127.0.0.1:8845"


def teardown_schema(client: GdbClient, silent: bool = False):
    if not silent:
        print("[*] Cleaning up replication schemas (Device, LINKED)...")
    try:
        client.execute("DROP VERTEX Device;")
    except Exception:
        pass
    try:
        client.execute("DROP EDGE LINKED;")
    except Exception:
        pass


def main():
    parser = argparse.ArgumentParser(description="GDB Replication Verification (via gdb-client)")
    parser.add_argument("--mode", choices=["http", "flight", "mpp"], default="http", help="Query transport mode: 'http' or 'flight'/'mpp'")
    parser.add_argument("--keep-schema", action="store_true", help="Preserve test schemas after verification")
    args = parser.parse_args()
    use_flight = args.mode in ("flight", "mpp")

    print("\033[1;36m" + "=" * 65)
    print("      GDB Leaderless Ring Replication Verification Suite (gdb-client)     ")
    print("=" * 65 + "\033[0m")
    print(f"[*] Transport Mode: {'Arrow Flight MPP' if use_flight else 'HTTP REST'}\n")

    # 1. Health & Cluster Check on all 3 peers
    print("[*] Probing leaderless ring peers and topology...")
    peers = [("Peer 1", PEER1_URL, 8860), ("Peer 2", PEER2_URL, 8861), ("Peer 3", PEER3_URL, 8862)]
    clients = []

    for name, url, fport in peers:
        try:
            c = GdbClient(endpoint=url, client_flight_port=fport)
            health = c.health()
            if health.get("status") == "UP":
                rf = health.get("replication_factor", "?")
                mode = health.get("replication_mode", "?")
                print(f"  \033[1;32m[✓]\033[0m {name} at {url} is UP | Role: Peer | RF={rf} | Mode={mode}")
                clients.append((name, c))
            else:
                print(f"  \033[1;31m[!]\033[0m {name} at {url} returned non-UP status: {health}")
        except Exception as e:
            print(f"  \033[1;31m[!]\033[0m {name} at {url} is NOT reachable: {e}")

    if len(clients) < 3:
        print("\n\033[1;33m[!] Notice: Requires a running 3-node cluster to verify replication.\033[0m")
        print("    Please run: ./scripts/start_cluster.sh")
        sys.exit(1)

    c1 = clients[0][1]
    c2 = clients[1][1]
    c3 = clients[2][1]

    # 2. Setup Schema via Peer 1
    print("\n[*] Initializing schema via Peer 1 coordinator...")
    teardown_schema(c1, silent=True)
    c1.execute("CREATE VERTEX Device (name STRING, ip STRING);")
    c1.execute("CREATE EDGE LINKED ();")

    # Wait brief moment for schema broadcast
    time.sleep(0.5)

    # 3. Insert Entities via Peer 1 (Coordinator)
    print("[*] Inserting 10 vertices and 5 edges via Peer 1 (coordinator)...")
    for vid in range(1, 11):
        c1.execute(f"INSERT VERTEX Device (id, name, ip) VALUES ({vid}, 'Dev_{vid}', '192.168.1.{vid}');")

    c1.execute("INSERT EDGE LINKED FROM 1 TO 2; INSERT EDGE LINKED FROM 2 TO 3; INSERT EDGE LINKED FROM 3 TO 4; INSERT EDGE LINKED FROM 4 TO 5; INSERT EDGE LINKED FROM 5 TO 1;")

    # Compact storage on all peers
    for _, c in clients:
        c.compact()

    time.sleep(0.5)

    # 4. Verify Reads Across All Peers
    print("[*] Verifying replication consistency across all 3 ring peers...")
    all_ok = True

    for name, c in clients:
        try:
            res_v = c.query("MATCH (d:Device) RETURN count(d);", use_flight=use_flight)
            res_e = c.query("MATCH (a:Device)-[:LINKED]->(b:Device) RETURN count(b);", use_flight=use_flight)
            v_count = res_v.rows[0][0] if res_v.rows else 0
            e_count = res_e.rows[0][0] if res_e.rows else 0

            print(f"  -> {name} reports: {v_count} vertices, {e_count} edges")
            if v_count < 10 or e_count < 5:
                all_ok = False
        except Exception as e:
            print(f"  \033[1;31m[!] Read error on {name}: {e}\033[0m")
            all_ok = False

    # 5. Symmetric Write via Peer 3
    print("[*] Testing symmetric write coordination via Peer 3...")
    try:
        c3.execute("INSERT VERTEX Device (id, name, ip) VALUES (99, 'Dev_Symmetric', '10.0.0.1');")
        for _, c in clients:
            c.compact()
        time.sleep(0.5)

        res_check = c1.query("MATCH (d:Device) WHERE d.id = 99 RETURN d.name;", use_flight=use_flight)
        if res_check.rows and res_check.rows[0][0] == "Dev_Symmetric":
            print("  \033[1;32m[✓]\033[0m Symmetric write through Peer 3 verified on Peer 1!")
        else:
            print("  \033[1;31m[!]\033[0m Symmetric write failed to replicate to Peer 1")
            all_ok = False
    except Exception as e:
        print(f"  \033[1;31m[!] Error executing write on Peer 3: {e}\033[0m")
        all_ok = False

    if not args.keep_schema:
        teardown_schema(c1)

    for _, c in clients:
        c.close()

    print("\n" + "=" * 65)
    if all_ok:
        print("\033[1;32m[✓] All 3-node leaderless replication tests PASSED successfully!\033[0m")
    else:
        print("\033[1;31m[!] Replication tests finished with errors.\033[0m")
        sys.exit(1)
    print("=" * 65 + "\n")


if __name__ == "__main__":
    main()
