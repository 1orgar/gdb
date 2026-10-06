#!/bin/bash
# Stops running GDB cluster nodes

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PID_FILE="$PROJECT_ROOT/.cluster.pids"

if [ -f "$PID_FILE" ]; then
    PIDS=$(cat "$PID_FILE")
    echo "Stopping GDB cluster nodes (PIDs: $PIDS)..."
    kill $PIDS 2>/dev/null
    rm -f "$PID_FILE"
    echo "Cluster stopped."
else
    echo "No .cluster.pids file found. Killing any running gdb-server processes..."
    pkill -f gdb-server 2>/dev/null
    echo "Done."
fi
