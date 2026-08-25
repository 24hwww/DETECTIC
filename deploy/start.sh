#!/bin/sh
# start.sh — Start the Detectic sensor service.

set -e

INSTALL_BASE="${1:-/var/run/misc/misc_rw/detectic}"
CURRENT="$INSTALL_BASE/current/detectic"
LOG="$INSTALL_BASE/logs/detectic.log"
PID_FILE="$INSTALL_BASE/state/detectic.pid"

echo "[start] Starting Detectic from $CURRENT"

if [ ! -x "$CURRENT" ]; then
    echo "[start] ERROR: binary not found: $CURRENT" >&2
    exit 1
fi

# Load config
if [ -f "$INSTALL_BASE/config/detectic.env" ] && [ -z "$DETECTIC_PASSWORD" ]; then
    . "$INSTALL_BASE/config/detectic.env"
fi

if [ -z "$DETECTIC_PASSWORD" ]; then
    echo "[start] ERROR: DETECTIC_PASSWORD is not set" >&2
    exit 1
fi
if [ -z "$DETECTIC_SECRET" ]; then
    echo "[start] ERROR: DETECTIC_SECRET is not set" >&2
    exit 1
fi

# Use generated sensor_id if config says "auto"
if [ "$DETECTIC_SENSOR_ID" = "auto" ] && [ -f "$INSTALL_BASE/state/sensor_id" ]; then
    export DETECTIC_SENSOR_ID=$(cat "$INSTALL_BASE/state/sensor_id")
fi

# Stop existing instance
if [ -f "$PID_FILE" ] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
    echo "[start] Stopping existing instance PID $(cat $PID_FILE)"
    kill "$(cat "$PID_FILE")" 2>/dev/null || true
    sleep 2
fi

# Start
nohup "$CURRENT" sensor > "$LOG" 2>&1 &
PID=$!
echo "$PID" > "$PID_FILE"
echo "[start] Started PID $PID"
echo "[start] Logs: $LOG"
