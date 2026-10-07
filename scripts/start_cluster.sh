#!/bin/bash
# Starts a 3-node GDB cluster on ARM Mac with Metal GPU acceleration enabled

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$PROJECT_ROOT/bin/gdb-server"
PID_FILE="$PROJECT_ROOT/.cluster.pids"

if [ ! -f "$BIN" ]; then
    echo "Binary $BIN not found! Building release binaries..."
    (cd "$PROJECT_ROOT" && source "$HOME/.cargo/env" && cargo build --release --bin gdb-server --bin gdb-cli && mkdir -p bin && cp target/release/gdb-server target/release/gdb-cli bin/)
fi

RF=3
REP_MODE="sync"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --rf|--replication-factor|-r)
            RF="$2"
            shift 2
            ;;
        --sync|sync)
            REP_MODE="sync"
            shift
            ;;
        --async|async)
            REP_MODE="async"
            shift
            ;;
        --replication-mode)
            REP_MODE="$2"
            shift 2
            ;;
        --sharding|--sharded|sharding)
            RF=1
            shift
            ;;
        --replication|--replicated|replication)
            RF=3
            shift
            ;;
        *)
            shift
            ;;
    esac
done

MODE_UPPER=$(echo "$REP_MODE" | tr '[:lower:]' '[:upper:]')
echo -e "\x1b[1;36m============================================================\x1b[0m"
echo -e "\x1b[1;36m       Starting 3-Node GDB Cluster (Leaderless Ring)        \x1b[0m"
echo -e "\x1b[1;33m       Replication Factor: RF=$RF | Mode: $MODE_UPPER       \x1b[0m"
echo -e "\x1b[1;36m============================================================\x1b[0m"

mkdir -p "$PROJECT_ROOT/data/node1/wal" "$PROJECT_ROOT/data/node2/wal" "$PROJECT_ROOT/data/node3/wal"
mkdir -p "$PROJECT_ROOT/logs"

# Start Peer 1
"$BIN" --node-id 1 --partitions 8 --port 8848 --client-flight-port 8860 --http-port 8847 --wal-dir "$PROJECT_ROOT/data/node1/wal" --peers "http://127.0.0.1:8846,http://127.0.0.1:8845" --replication-factor "$RF" --replication-mode "$REP_MODE" > "$PROJECT_ROOT/logs/node1.log" 2>&1 &
PID1=$!
echo -e "\x1b[1;32m[+] Peer 1 started (PID $PID1):\x1b[0m Internal Flight :8848 | Client Flight :8860 | HTTP :8847 | RF: $RF | Mode: $REP_MODE | Role: Peer"

# Start Peer 2
"$BIN" --node-id 2 --partitions 8 --port 8849 --client-flight-port 8861 --http-port 8846 --wal-dir "$PROJECT_ROOT/data/node2/wal" --peers "http://127.0.0.1:8847,http://127.0.0.1:8845" --replication-factor "$RF" --replication-mode "$REP_MODE" > "$PROJECT_ROOT/logs/node2.log" 2>&1 &
PID2=$!
echo -e "\x1b[1;32m[+] Peer 2 started (PID $PID2):\x1b[0m Internal Flight :8849 | Client Flight :8861 | HTTP :8846 | RF: $RF | Mode: $REP_MODE | Role: Peer"

# Start Peer 3
"$BIN" --node-id 3 --partitions 8 --port 8850 --client-flight-port 8862 --http-port 8845 --wal-dir "$PROJECT_ROOT/data/node3/wal" --peers "http://127.0.0.1:8847,http://127.0.0.1:8846" --replication-factor "$RF" --replication-mode "$REP_MODE" > "$PROJECT_ROOT/logs/node3.log" 2>&1 &
PID3=$!
echo -e "\x1b[1;32m[+] Peer 3 started (PID $PID3):\x1b[0m Internal Flight :8850 | Client Flight :8862 | HTTP :8845 | RF: $RF | Mode: $REP_MODE | Role: Peer"

echo "$PID1 $PID2 $PID3" > "$PID_FILE"

sleep 1

# Check health of Node 1
HEALTH=$(curl -s http://localhost:8847/health)
if [[ $HEALTH == *"UP"* ]]; then
    echo -e "\n\x1b[1;32m[✓] 3-node cluster is healthy and ready for queries!\x1b[0m"
    echo -e "    CLI connect:   \x1b[1;33m./bin/gdb-cli\x1b[0m"
    echo -e "    Web Studio UI: \x1b[1;33m./scripts/start_studio.sh\x1b[0m (http://localhost:3000)"
    echo -e "    HTTP endpoint: \x1b[1;33mhttp://localhost:8847/query\x1b[0m"
    echo -e "    Logs:          \x1b[1;33mtail -f logs/node*.log\x1b[0m"
    echo -e "    Stop cluster:  \x1b[1;33m./scripts/stop_cluster.sh\x1b[0m\n"
else
    echo -e "\x1b[1;31m[!] Warning: Cluster health check did not respond immediately. Check logs/node1.log\x1b[0m"
fi
