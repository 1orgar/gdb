#!/bin/bash
# Starts GDB Studio Web UI & Interactive Graph Workspace

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$PROJECT_ROOT/bin/gdb-studio"
PID_FILE="$PROJECT_ROOT/.studio.pid"

# Default configuration
PORT="${PORT:-3000}"
CLUSTER_URL="${CLUSTER_URL:-http://localhost:8847}"
HOST="${HOST:-0.0.0.0}"
NO_BROWSER=false

print_usage() {
    echo "Usage: $0 [options] [PORT] [CLUSTER_URL]"
    echo ""
    echo "Options:"
    echo "  -p, --port <PORT>                Port to serve the Web UI on (default: 3000)"
    echo "  -c, --cluster-url <URL>          Target GDB Cluster REST endpoint (default: http://localhost:8847)"
    echo "  -h, --host <HOST>                Bind address (default: 0.0.0.0)"
    echo "      --no-browser                 Do not automatically open browser on startup"
    echo "      --help                       Show this help message"
    echo ""
    echo "Positional Arguments (legacy compatibility):"
    echo "  PORT                             Port number (e.g. 8899)"
    echo "  CLUSTER_URL                      Cluster URL (e.g. http://localhost:8847)"
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        -p|--port)
            PORT="$2"
            shift 2
            ;;
        --port=*)
            PORT="${1#*=}"
            shift
            ;;
        -c|--cluster-url)
            CLUSTER_URL="$2"
            shift 2
            ;;
        --cluster-url=*)
            CLUSTER_URL="${1#*=}"
            shift
            ;;
        -h|--host)
            HOST="$2"
            shift 2
            ;;
        --host=*)
            HOST="${1#*=}"
            shift
            ;;
        --no-browser)
            NO_BROWSER=true
            shift
            ;;
        --help)
            print_usage
            exit 0
            ;;
        *)
            # Positional arguments support
            if [[ "$1" =~ ^[0-9]+$ ]]; then
                PORT="$1"
                shift
            elif [[ "$1" =~ ^https?:// ]]; then
                CLUSTER_URL="$1"
                shift
            else
                echo -e "\x1b[1;31m[!] Unknown option or argument: $1\x1b[0m" >&2
                print_usage >&2
                exit 1
            fi
            ;;
    esac
done

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
        mkdir -p "$PROJECT_ROOT/bin"
        rm -f "$PROJECT_ROOT/bin/$bin_name"
        cp "$PROJECT_ROOT/bin/amd64/$bin_name" "$PROJECT_ROOT/bin/$bin_name"
        chmod +x "$PROJECT_ROOT/bin/$bin_name"
        echo "$PROJECT_ROOT/bin/$bin_name" && return 0
    elif { [ "$arch" = "aarch64" ] || [ "$arch" = "arm64" ]; } && [ -x "$PROJECT_ROOT/bin/arm64/$bin_name" ]; then
        mkdir -p "$PROJECT_ROOT/bin"
        rm -f "$PROJECT_ROOT/bin/$bin_name"
        cp "$PROJECT_ROOT/bin/arm64/$bin_name" "$PROJECT_ROOT/bin/$bin_name"
        chmod +x "$PROJECT_ROOT/bin/$bin_name"
        echo "$PROJECT_ROOT/bin/$bin_name" && return 0
    fi
    if [ -x "$PROJECT_ROOT/target/release/$bin_name" ]; then
        mkdir -p "$PROJECT_ROOT/bin"
        rm -f "$PROJECT_ROOT/bin/$bin_name"
        cp "$PROJECT_ROOT/target/release/$bin_name" "$PROJECT_ROOT/bin/$bin_name"
        chmod +x "$PROJECT_ROOT/bin/$bin_name"
        echo "$PROJECT_ROOT/bin/$bin_name" && return 0
    fi
    if command -v "$bin_name" >/dev/null 2>&1; then command -v "$bin_name" && return 0; fi
    if [ -f "$PROJECT_ROOT/Cargo.toml" ]; then
        if [ -f "$HOME/.cargo/env" ]; then source "$HOME/.cargo/env" 2>/dev/null || true; fi
        if command -v cargo >/dev/null 2>&1; then
            echo "Binary $bin_name not found! Building release binary with cargo..." >&2
            (cd "$PROJECT_ROOT" && cargo build --release --bin "$bin_name" && mkdir -p bin && rm -f "bin/$bin_name" && cp "target/release/$bin_name" "bin/$bin_name" && chmod +x "bin/$bin_name") >&2
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
"$BIN" --port "$PORT" --cluster-url "$CLUSTER_URL" --host "$HOST" > "$PROJECT_ROOT/logs/studio.log" 2>&1 &
STUDIO_PID=$!
echo "$STUDIO_PID" > "$PID_FILE"

# Check health with retry loop
HEALTH_OK=false
for i in {1..10}; do
    HEALTH=$(curl -s "http://127.0.0.1:$PORT/health" 2>/dev/null || curl -s "http://localhost:$PORT/health" 2>/dev/null)
    if [[ "$HEALTH" == *"UP"* ]]; then
        HEALTH_OK=true
        break
    fi
    if ! kill -0 "$STUDIO_PID" 2>/dev/null; then
        break
    fi
    sleep 0.5
done

if [ "$HEALTH_OK" = true ]; then
    echo -e "\x1b[1;32m[✓] GDB Studio is running!\x1b[0m (PID $STUDIO_PID)"
    echo -e "    Web UI URL:     \x1b[1;33mhttp://localhost:$PORT\x1b[0m"
    echo -e "    Target Cluster: \x1b[1;33m$CLUSTER_URL\x1b[0m"
    echo -e "    Logs:           \x1b[1;33mtail -f logs/studio.log\x1b[0m"
    echo -e "    Stop Studio:    \x1b[1;33m./scripts/stop_studio.sh\x1b[0m\n"

    # Try opening browser if on macOS and not disabled
    if [ "$NO_BROWSER" = false ] && command -v open >/dev/null 2>&1; then
        open "http://localhost:$PORT" 2>/dev/null &
    fi
else
    echo -e "\x1b[1;31m[!] Warning: Studio health check failed. Check logs/studio.log\x1b[0m"
    if [ -f "$PROJECT_ROOT/logs/studio.log" ]; then
        echo -e "\x1b[1;33mRecent logs:\x1b[0m"
        tail -n 10 "$PROJECT_ROOT/logs/studio.log"
    fi
    exit 1
fi
