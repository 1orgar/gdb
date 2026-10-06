#!/bin/bash
# Stops running GDB Studio instance

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PID_FILE="$PROJECT_ROOT/.studio.pid"

if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE")
    echo "Stopping GDB Studio (PID $PID)..."
    kill $PID 2>/dev/null
    rm -f "$PID_FILE"
    echo "GDB Studio stopped."
else
    echo "No .studio.pid file found. Checking for any running gdb-studio processes..."
    pkill -f gdb-studio 2>/dev/null
    echo "Done."
fi
