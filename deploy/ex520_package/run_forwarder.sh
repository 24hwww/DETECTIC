#!/bin/bash
# ============================================================================
# Detectic Forwarder — starts the HTTP→HTTPS bridge
# Run this on the HOST machine (192.168.0.27), NOT on the EX520.
# ============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
FORWARDER="$SCRIPT_DIR/forwarder.py"
PID_FILE="$SCRIPT_DIR/.forwarder.pid"
LOG_FILE="$SCRIPT_DIR/forwarder.log"
PORT="${FORWARDER_PORT:-8082}"

# Check if already running
if [ -f "$PID_FILE" ] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
    echo "[forwarder] already running (PID $(cat "$PID_FILE"))"
    exit 0
fi

# Start forwarder
echo "[forwarder] starting on port $PORT..."
nohup python3 -u "$FORWARDER" --port "$PORT" >> "$LOG_FILE" 2>&1 &
echo $! > "$PID_FILE"
echo "[forwarder] started (PID $!)"
echo "[forwarder] log: $LOG_FILE"
echo "[forwarder] health: curl http://localhost:$PORT/healthz"
