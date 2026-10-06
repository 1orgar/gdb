#!/bin/bash
# Starts a 3-node GDB cluster on AMD64 (x86_64) architecture

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$PROJECT_ROOT/bin/amd64/gdb-server"
CLI="$PROJECT_ROOT/bin/amd64/gdb-cli"
PID_FILE="$PROJECT_ROOT/.cluster_amd64.pids"

# Check if binary exists
if [ ! -f "$BIN" ]; then
    echo "AMD64 binary $BIN not found! Building x86_64 release binaries..."
    (cd "$PROJECT_ROOT" && source "$HOME/.cargo/env" && cargo build --release --target x86_64-apple-darwin --bin gdb-server --bin gdb-cli && mkdir -p bin/amd64 && cp target/x86_64-apple-darwin/release/gdb-server target/x86_64-apple-darwin/release/gdb-cli bin/amd64/)
fi

# Detect host CPU architecture
HOST_ARCH=$(uname -m)
RUNNER=""

if [ "$HOST_ARCH" = "arm64" ]; then
    # Running x86_64 binary on Apple Silicon requires Rosetta 2
    if arch -x86_64 true 2>/dev/null; then
        RUNNER="arch -x86_64"
    else
        echo -e "\x1b[1;33m[!] Notice: You are on an Apple Silicon Mac (arm64).\x1b[0m"
        echo -e "    To run AMD64 (x86_64) binaries on Apple Silicon, install Rosetta 2:"
        echo -e "    \x1b[1msoftwareupdate --install-rosetta --agree-to-license\x1b[0m"
        echo -e "    Or run native ARM64 cluster: \x1b[1;32m./scripts/start_cluster.sh\x1b[0m\n"
        exit 1
    fi
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
echo -e "\x1b[1;36m       Starting 3-Node GDB Cluster (AMD64 / x86_64)         \x1b[0m"
echo -e "\x1b[1;33m       Replication Factor: RF=$RF | Mode: $MODE_UPPER       \x1b[0m"
echo -e "\x1b[1;36m============================================================\x1b[0m"

mkdir -p "$PROJECT_ROOT/data/amd64/node1/wal" "$PROJECT_ROOT/data/amd64/node2/wal" "$PROJECT_ROOT/data/amd64/node3/wal"
mkdir -p "$PROJECT_ROOT/logs/amd64"

# Start Peer 1
$RUNNER "$BIN" --node-id 1 --partitions 8 --port 8848 --http-port 8847 --wal-dir "$PROJECT_ROOT/data/amd64/node1/wal" --peers "http://127.0.0.1:8846,http://127.0.0.1:8845" --replication-factor "$RF" --replication-mode "$REP_MODE" > "$PROJECT_ROOT/logs/amd64/node1.log" 2>&1 &
PID1=$!
echo -e "\x1b[1;32m[+] Peer 1 started (PID $PID1):\x1b[0m Flight :8848 | HTTP :8847 | RF: $RF | Mode: $REP_MODE | Arch: AMD64 | Role: Peer"

# Start Peer 2
$RUNNER "$BIN" --node-id 2 --partitions 8 --port 8849 --http-port 8846 --wal-dir "$PROJECT_ROOT/data/amd64/node2/wal" --peers "http://127.0.0.1:8847,http://127.0.0.1:8845" --replication-factor "$RF" --replication-mode "$REP_MODE" > "$PROJECT_ROOT/logs/amd64/node2.log" 2>&1 &
PID2=$!
echo -e "\x1b[1;32m[+] Peer 2 started (PID $PID2):\x1b[0m Flight :8849 | HTTP :8846 | RF: $RF | Mode: $REP_MODE | Arch: AMD64 | Role: Peer"

# Start Peer 3
$RUNNER "$BIN" --node-id 3 --partitions 8 --port 8850 --http-port 8845 --wal-dir "$PROJECT_ROOT/data/amd64/node3/wal" --peers "http://127.0.0.1:8847,http://127.0.0.1:8846" --replication-factor "$RF" --replication-mode "$REP_MODE" > "$PROJECT_ROOT/logs/amd64/node3.log" 2>&1 &
PID3=$!
echo -e "\x1b[1;32m[+] Peer 3 started (PID $PID3):\x1b[0m Flight :8850 | HTTP :8845 | RF: $RF | Mode: $REP_MODE | Arch: AMD64 | Role: Peer"

echo "$PID1 $PID2 $PID3" > "$PID_FILE"

sleep 1

# Check health of Node 1
HEALTH=$(curl -s http://localhost:8847/health)
if [[ $HEALTH == *"UP"* ]]; then
    echo -e "\n\x1b[1;32m[✓] 3-node AMD64 cluster is healthy and ready for queries!\x1b[0m"
    echo -e "    CLI connect:   \x1b[1;33m./bin/amd64/gdb-cli\x1b[0m"
    echo -e "    Web Studio UI: \x1b[1;33m./scripts/start_studio.sh\x1b[0m (http://localhost:3000)"
    echo -e "    HTTP endpoint: \x1b[1;33mhttp://localhost:8847/query\x1b[0m"
    echo -e "    Logs:          \x1b[1;33mtail -f logs/amd64/node*.log\x1b[0m"
    echo -e "    Stop cluster:  \x1b[1;33m./scripts/stop_cluster_amd64.sh\x1b[0m\n"
else
    echo -e "\x1b[1;31m[!] Warning: Cluster health check did not respond immediately. Check logs/amd64/node1.log\x1b[0m"
fi
