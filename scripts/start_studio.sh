#!/bin/bash
# Starts GDB Studio Web UI & Interactive Graph Workspace

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$PROJECT_ROOT/bin/gdb-studio"
PORT="${1:-3000}"
CLUSTER_URL="${2:-http://localhost:8847}"
PID_FILE="$PROJECT_ROOT/.studio.pid"

resolve_binary() {
    local bin_name="$1"
    if [ -x "$PROJECT_ROOT/bin/$bin_name" ]; then
        echo "$PROJECT_ROOT/bin/$bin_name" && return 0
    fi
    if [ -f "$PROJECT_ROOT/bin/$bin_name" ]; then
        chmod +x "$PROJECT_ROOT/bin/$bin_name" 2>/dev/null
        if [ -x "$PROJECT_ROOT/bin/$bin_name" ]; then echo "$PROJECT_ROOT/bin/$bin_name" && return 0; fi
    fi
    if [ -x "$PROJECT_ROOT/$bin_name" ]; then echo "$PROJECT_ROOT/$bin_name" && return 0; fi
    local arch="$(uname -m)"
    if [ "$arch" = "x86_64" ] && [ -x "$PROJECT_ROOT/bin/amd64/$bin_name" ]; then
        mkdir -p "$PROJECT_ROOT/bin" && cp "$PROJECT_ROOT/bin/amd64/$bin_name" "$PROJECT_ROOT/bin/$bin_name"
        echo "$PROJECT_ROOT/bin/$bin_name" && return 0
    elif { [ "$arch" = "aarch64" ] || [ "$arch" = "arm64" ]; } && [ -x "$PROJECT_ROOT/bin/arm64/$bin_name" ]; then
        mkdir -p "$PROJECT_ROOT/bin" && cp "$PROJECT_ROOT/bin/arm64/$bin_name" "$PROJECT_ROOT/bin/$bin_name"
        echo "$PROJECT_ROOT/bin/$bin_name" && return 0
    fi
    if [ -x "$PROJECT_ROOT/target/release/$bin_name" ]; then
        mkdir -p "$PROJECT_ROOT/bin" && cp "$PROJECT_ROOT/target/release/$bin_name" "$PROJECT_ROOT/bin/$bin_name"
        echo "$PROJECT_ROOT/bin/$bin_name" && return 0
    fi
    if command -v "$bin_name" >/dev/null 2>&1; then command -v "$bin_name" && return 0; fi
    if [ -f "$PROJECT_ROOT/Cargo.toml" ]; then
        if [ -f "$HOME/.cargo/env" ]; then source "$HOME/.cargo/env" 2>/dev/null || true; fi
        if command -v cargo >/dev/null 2>&1; then
            echo "Binary $bin_name not found! Building release binary with cargo..." >&2
            (cd "$PROJECT_ROOT" && cargo build --release --bin "$bin_name" && mkdir -p bin && cp "target/release/$bin_name" "bin/$bin_name") >&2
            if [ -x "$PROJECT_ROOT/bin/$bin_name" ]; then echo "$PROJECT_ROOT/bin/$bin_name" && return 0; fi
        fi
    fi
    return 1
}

BIN=$(resolve_binary "gdb-studio")
if [ -z "$BIN" ] || [ ! -x "$BIN" ]; then
    echo -e "\x1b[1;31m[!] Error: gdb-studio binary not found!\x1b[0m" >&2
    echo -e "    Please place the executable into '$PROJECT_ROOT/bin/gdb-studio' or run 'cargo build --release --bin gdb-studio'.\x1b[0m" >&2
    exit 1
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
