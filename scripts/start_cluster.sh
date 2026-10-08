#!/bin/bash
# Starts a 3-node GDB cluster on ARM Mac with Metal GPU acceleration enabled

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PID_FILE="$PROJECT_ROOT/.cluster.pids"

resolve_binary() {
    local bin_name="$1"

    # 1. Direct path in bin/
    if [ -x "$PROJECT_ROOT/bin/$bin_name" ]; then
        echo "$PROJECT_ROOT/bin/$bin_name"
        return 0
    fi

    # 2. Check if file exists in bin/ without executable permissions
    if [ -f "$PROJECT_ROOT/bin/$bin_name" ]; then
        chmod +x "$PROJECT_ROOT/bin/$bin_name" 2>/dev/null
        if [ -x "$PROJECT_ROOT/bin/$bin_name" ]; then
            echo "$PROJECT_ROOT/bin/$bin_name"
            return 0
        fi
    fi

    # 3. Check root directory (if uncompressed flat)
    if [ -x "$PROJECT_ROOT/$bin_name" ]; then
        echo "$PROJECT_ROOT/$bin_name"
        return 0
    fi

    # 4. Check architecture subfolders (bin/amd64 or bin/arm64)
    local arch="$(uname -m)"
    if [ "$arch" = "x86_64" ] && [ -x "$PROJECT_ROOT/bin/amd64/$bin_name" ]; then
        mkdir -p "$PROJECT_ROOT/bin"
        cp "$PROJECT_ROOT/bin/amd64/$bin_name" "$PROJECT_ROOT/bin/$bin_name"
        echo "$PROJECT_ROOT/bin/$bin_name"
        return 0
    elif { [ "$arch" = "aarch64" ] || [ "$arch" = "arm64" ]; } && [ -x "$PROJECT_ROOT/bin/arm64/$bin_name" ]; then
        mkdir -p "$PROJECT_ROOT/bin"
        cp "$PROJECT_ROOT/bin/arm64/$bin_name" "$PROJECT_ROOT/bin/$bin_name"
        echo "$PROJECT_ROOT/bin/$bin_name"
        return 0
    fi

    # 5. Check target/release
    if [ -x "$PROJECT_ROOT/target/release/$bin_name" ]; then
        mkdir -p "$PROJECT_ROOT/bin"
        cp "$PROJECT_ROOT/target/release/$bin_name" "$PROJECT_ROOT/bin/$bin_name"
        echo "$PROJECT_ROOT/bin/$bin_name"
        return 0
    fi

    # 6. Check global PATH
    if command -v "$bin_name" >/dev/null 2>&1; then
        command -v "$bin_name"
        return 0
    fi

    # 7. Try compiling with cargo if source code is present
    if [ -f "$PROJECT_ROOT/Cargo.toml" ]; then
        if [ -f "$HOME/.cargo/env" ]; then
            source "$HOME/.cargo/env" 2>/dev/null || true
        fi
        if command -v cargo >/dev/null 2>&1; then
            echo "Binary $bin_name not found! Building release binaries with cargo..." >&2
            (cd "$PROJECT_ROOT" && cargo build --release --bin "$bin_name" && mkdir -p bin && cp "target/release/$bin_name" "bin/$bin_name") >&2
            if [ -x "$PROJECT_ROOT/bin/$bin_name" ]; then
                echo "$PROJECT_ROOT/bin/$bin_name"
                return 0
            fi
        fi
    fi

    return 1
}

BIN=$(resolve_binary "gdb-server")
if [ -z "$BIN" ] || [ ! -x "$BIN" ]; then
    echo -e "\x1b[1;31m[!] Error: gdb-server binary not found!\x1b[0m" >&2
    echo -e "    Checked '$PROJECT_ROOT/bin/gdb-server', '$PROJECT_ROOT/bin/amd64/gdb-server', target/release, and PATH." >&2
    echo -e "    Please place the executable into '$PROJECT_ROOT/bin/gdb-server' or run 'cargo build --release --bin gdb-server'.\x1b[0m" >&2
    exit 1
fi

NODES=3
RF=3
REP_MODE="sync"
ENABLE_GPU=false
GPU_DEVICE=0
GPU_THRESHOLD=10000

while [[ $# -gt 0 ]]; do
    case "$1" in
        --nodes|-n)
            NODES="$2"
            shift 2
            ;;
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
        --gpu|--enable-gpu)
            ENABLE_GPU=true
            shift
            ;;
        --gpu-device)
            GPU_DEVICE="$2"
            shift 2
            ;;
        --gpu-threshold|--gpu-offload-threshold)
            GPU_THRESHOLD="$2"
            shift 2
            ;;
        *)
            shift
            ;;
    esac
done

if [ "$RF" -gt "$NODES" ]; then
    RF="$NODES"
fi

GPU_FLAGS=""
GPU_DESC="Disabled"
if [ "$ENABLE_GPU" = true ]; then
    GPU_FLAGS="--enable-gpu --gpu-device $GPU_DEVICE --gpu-offload-threshold $GPU_THRESHOLD"
    GPU_DESC="Active (Device #$GPU_DEVICE, Threshold: $GPU_THRESHOLD edges)"
fi

MODE_UPPER=$(echo "$REP_MODE" | tr '[:lower:]' '[:upper:]')
echo -e "\x1b[1;36m============================================================\x1b[0m"
echo -e "\x1b[1;36m       Starting $NODES-Node GDB Cluster (Leaderless Ring)        \x1b[0m"
echo -e "\x1b[1;33m       Replication Factor: RF=$RF | Mode: $MODE_UPPER       \x1b[0m"
echo -e "\x1b[1;35m       Hardware GPU: $GPU_DESC                              \x1b[0m"
echo -e "\x1b[1;36m============================================================\x1b[0m"

mkdir -p "$PROJECT_ROOT/logs"

PIDS=()
for ((i=1; i<=NODES; i++)); do
    mkdir -p "$PROJECT_ROOT/data/node$i/wal"
    HTTP_PORT=$(( 8847 - (i - 1) ))
    FLIGHT_PORT=$(( 8848 + (i - 1) ))
    CLIENT_FLIGHT_PORT=$(( 8860 + (i - 1) ))

    PEERS=""
    for ((j=1; j<=NODES; j++)); do
        if [ "$j" -ne "$i" ]; then
            P_HTTP=$(( 8847 - (j - 1) ))
            if [ -z "$PEERS" ]; then
                PEERS="http://127.0.0.1:$P_HTTP"
            else
                PEERS="$PEERS,http://127.0.0.1:$P_HTTP"
            fi
        fi
    done

    PEERS_FLAG=""
    if [ -n "$PEERS" ]; then
        PEERS_FLAG="--peers $PEERS"
    fi

    "$BIN" --node-id "$i" --partitions 8 --port "$FLIGHT_PORT" --client-flight-port "$CLIENT_FLIGHT_PORT" --http-port "$HTTP_PORT" --wal-dir "$PROJECT_ROOT/data/node$i/wal" $PEERS_FLAG --replication-factor "$RF" --replication-mode "$REP_MODE" $GPU_FLAGS > "$PROJECT_ROOT/logs/node$i.log" 2>&1 &
    PID=$!
    PIDS+=("$PID")
    echo -e "\x1b[1;32m[+] Peer $i started (PID $PID):\x1b[0m Internal Flight :$FLIGHT_PORT | Client Flight :$CLIENT_FLIGHT_PORT | HTTP :$HTTP_PORT | RF: $RF | Mode: $REP_MODE | GPU: $GPU_DESC | Role: Peer"
done

echo "${PIDS[*]}" > "$PID_FILE"

sleep 1

# Check health of Node 1
HEALTH=$(curl -s http://localhost:8847/health)
if [[ $HEALTH == *"UP"* ]]; then
    echo -e "\n\x1b[1;32m[✓] $NODES-node cluster is healthy and ready for queries!\x1b[0m"
    echo -e "    CLI connect:   \x1b[1;33m./bin/gdb-cli\x1b[0m"
    echo -e "    Web Studio UI: \x1b[1;33m./scripts/start_studio.sh\x1b[0m (http://localhost:3000)"
    echo -e "    HTTP endpoint: \x1b[1;33mhttp://localhost:8847/query\x1b[0m"
    echo -e "    Logs:          \x1b[1;33mtail -f logs/node*.log\x1b[0m"
    echo -e "    Stop cluster:  \x1b[1;33m./scripts/stop_cluster.sh\x1b[0m\n"
else
    echo -e "\x1b[1;31m[!] Warning: Cluster health check did not respond immediately. Check logs/node1.log\x1b[0m"
fi
