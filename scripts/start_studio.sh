#!/bin/bash
# Starts GDB Studio Web UI & Interactive Graph Workspace

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$PROJECT_ROOT/bin/gdb-studio"
PORT="${1:-3000}"
CLUSTER_URL="${2:-http://localhost:8847}"
PID_FILE="$PROJECT_ROOT/.studio.pid"

if [ ! -f "$BIN" ]; then
    echo "GDB Studio binary not found at $BIN! Building release binary..."
    (cd "$PROJECT_ROOT" && source "$HOME/.cargo/env" && cargo build --release --bin gdb-studio && mkdir -p bin && cp target/release/gdb-studio bin/)
fi

# Stop any previous instance
if [ -f "$PID_FILE" ]; then
    OLD_PID=$(cat "$PID_FILE")
    kill $OLD_PID 2>/dev/null
    rm -f "$PID_FILE"
fi

echo -e "\x1b[1;36m============================================================\x1b[0m"
echo -e "\x1b[1;36m       Starting GDB Studio (Web UI & Graph Workspace)       \x1b[0m"
echo -e "\x1b[1;36m============================================================\x1b[0m"

mkdir -p "$PROJECT_ROOT/logs"

# Start GDB Studio
"$BIN" --port "$PORT" --cluster-url "$CLUSTER_URL" > "$PROJECT_ROOT/logs/studio.log" 2>&1 &
STUDIO_PID=$!
echo "$STUDIO_PID" > "$PID_FILE"

sleep 1

# Check health
HEALTH=$(curl -s "http://localhost:$PORT/health")
if [[ $HEALTH == *"UP"* ]]; then
    echo -e "\x1b[1;32m[✓] GDB Studio is running!\x1b[0m (PID $STUDIO_PID)"
    echo -e "    Web UI URL:     \x1b[1;33mhttp://localhost:$PORT\x1b[0m"
    echo -e "    Target Cluster: \x1b[1;33m$CLUSTER_URL\x1b[0m"
    echo -e "    Logs:           \x1b[1;33mtail -f logs/studio.log\x1b[0m"
    echo -e "    Stop Studio:    \x1b[1;33m./scripts/stop_studio.sh\x1b[0m\n"

    # Try opening browser if on macOS
    if command -v open >/dev/null 2>&1; then
        open "http://localhost:$PORT" 2>/dev/null &
    fi
else
    echo -e "\x1b[1;31m[!] Warning: Studio health check failed. Check logs/studio.log\x1b[0m"
fi
