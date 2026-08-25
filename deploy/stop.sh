#!/bin/sh
# stop.sh — Stop the Detectic sensor.

set -e

INSTALL_BASE="${1:-/var/run/misc/misc_rw/detectic}"
PID_FILE="$INSTALL_BASE/state/detectic.pid"

echo "[stop] Stopping Detectic"

if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE")
    if kill -0 "$PID" 2>/dev/null; then
        kill "$PID" 2>/dev/null || true
        sleep 2
        if kill -0 "$PID" 2>/dev/null; then
            kill -9 "$PID" 2>/dev/null || true
        fi
        echo "[stop] Stopped PID $PID"
    else
        echo "[stop] Process already gone"
    fi
    rm -f "$PID_FILE"
else
    echo "[stop] No PID file found"
fi
