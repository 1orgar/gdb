#!/bin/bash
# Starts a 3-node GDB cluster on ARM Mac with Metal GPU acceleration enabled

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$PROJECT_ROOT/bin/gdb-server"
PID_FILE="$PROJECT_ROOT/.cluster.pids"

if [ ! -f "$BIN" ]; then
    echo "Binary $BIN not found! Building release binaries..."
    (cd "$PROJECT_ROOT" && source "$HOME/.cargo/env" && cargo build --release --bin gdb-server --bin gdb-cli && mkdir -p bin && cp target/release/gdb-server target/release/gdb-cli bin/)
fi

echo -e "\x1b[1;36m============================================================\x1b[0m"
echo -e "\x1b[1;36m       Starting 3-Node GDB Cluster (ARM Mac + GPU)         \x1b[0m"
echo -e "\x1b[1;36m============================================================\x1b[0m"

mkdir -p "$PROJECT_ROOT/data/node1/wal" "$PROJECT_ROOT/data/node2/wal" "$PROJECT_ROOT/data/node3/wal"
mkdir -p "$PROJECT_ROOT/logs"

# Start Node 1 (Coordinator / Leader + Storage)
"$BIN" --node-id 1 --partitions 8 --port 8848 --http-port 8847 --wal-dir "$PROJECT_ROOT/data/node1/wal" --peers "http://127.0.0.1:8846,http://127.0.0.1:8845" > "$PROJECT_ROOT/logs/node1.log" 2>&1 &
PID1=$!
echo -e "\x1b[1;32m[+] Node 1 started (PID $PID1):\x1b[0m Flight :8848 | HTTP :8847 | GPU: Apple Metal UMA | Role: Leader"

# Start Node 2 (Storage / Follower)
"$BIN" --node-id 2 --partitions 8 --port 8849 --http-port 8846 --wal-dir "$PROJECT_ROOT/data/node2/wal" --peers "http://127.0.0.1:8847,http://127.0.0.1:8845" > "$PROJECT_ROOT/logs/node2.log" 2>&1 &
PID2=$!
echo -e "\x1b[1;32m[+] Node 2 started (PID $PID2):\x1b[0m Flight :8849 | HTTP :8846 | GPU: Apple Metal UMA | Role: Follower"

# Start Node 3 (Storage / Follower)
"$BIN" --node-id 3 --partitions 8 --port 8850 --http-port 8845 --wal-dir "$PROJECT_ROOT/data/node3/wal" --peers "http://127.0.0.1:8847,http://127.0.0.1:8846" > "$PROJECT_ROOT/logs/node3.log" 2>&1 &
PID3=$!
echo -e "\x1b[1;32m[+] Node 3 started (PID $PID3):\x1b[0m Flight :8850 | HTTP :8845 | GPU: Apple Metal UMA | Role: Follower"

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
