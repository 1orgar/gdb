#!/bin/bash
# Stops running GDB cluster nodes

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PID_FILE="$PROJECT_ROOT/.cluster.pids"

STOPPED=false

for pid_file in "$PROJECT_ROOT/.cluster.pids" "$PROJECT_ROOT/.cluster_amd64.pids"; do
    if [ -f "$pid_file" ]; then
        PIDS=$(cat "$pid_file")
        echo "Stopping GDB cluster nodes from $(basename "$pid_file") (PIDs: $PIDS)..."
        kill $PIDS 2>/dev/null
        rm -f "$pid_file"
        STOPPED=true
    fi
done

# Kill any remaining gdb-server instances
pkill -f "gdb-server" 2>/dev/null || true
echo "GDB Cluster stopped cleanly."
