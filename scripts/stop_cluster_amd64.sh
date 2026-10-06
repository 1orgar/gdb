#!/bin/bash
# Stops running AMD64 GDB cluster nodes

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PID_FILE="$PROJECT_ROOT/.cluster_amd64.pids"

if [ -f "$PID_FILE" ]; then
    PIDS=$(cat "$PID_FILE")
    echo "Stopping AMD64 GDB cluster nodes (PIDs: $PIDS)..."
    kill $PIDS 2>/dev/null
    rm -f "$PID_FILE"
    echo "Cluster stopped."
else
    echo "No .cluster_amd64.pids file found. Checking for any running amd64 gdb-server processes..."
    pkill -f "bin/amd64/gdb-server" 2>/dev/null
    echo "Done."
fi
