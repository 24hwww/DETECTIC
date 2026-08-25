#!/bin/sh
# health.sh — Quick health check for the Detectic sensor.

set -e

INSTALL_BASE="${1:-/var/run/misc/misc_rw/detectic}"
CURRENT="$INSTALL_BASE/current/detectic"
PID_FILE="$INSTALL_BASE/state/detectic.pid"

echo "[health] Detectic health check"

if [ ! -x "$CURRENT" ]; then
    echo "[health] ERROR: binary missing" >&2
    exit 1
fi

# Verify binary runs (architecture + ELF verification by execution)
if ! "$CURRENT" version > /dev/null 2>&1; then
    echo "[health] ERROR: binary won't execute" >&2
    exit 1
fi

echo "[health] Binary: OK"

if [ -f "$PID_FILE" ] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
    echo "[health] Process: OK PID $(cat $PID_FILE)"
else
    echo "[health] Process: NOT RUNNING"
fi

if [ -f "$INSTALL_BASE/config/detectic.env" ]; then
    . "$INSTALL_BASE/config/detectic.env"
fi

"$CURRENT" health 2>&1 | sed 's/^/  /'
