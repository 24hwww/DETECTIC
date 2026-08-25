#!/bin/sh
# remove.sh — Remove Detectic from the router.

set -e

INSTALL_BASE="${1:-/var/run/misc/misc_rw/detectic}"

echo "[remove] Removing Detectic from $INSTALL_BASE"

# Stop the sensor
if [ -f "$INSTALL_BASE/state/detectic.pid" ]; then
    PID=$(cat "$INSTALL_BASE/state/detectic.pid")
    if kill -0 "$PID" 2>/dev/null; then
        echo "[remove] Stopping PID $PID"
        kill "$PID" 2>/dev/null || true
        sleep 2
    fi
fi

# Remove installation
rm -rf "$INSTALL_BASE"
echo "[remove] Removed $INSTALL_BASE"

echo ""
echo "[remove] NOTE: If Telnet/Lifemote were enabled for deployment,"
echo "  disable them via GTPR to restore the router to its pre-Detectic state:"
echo '  detectic set DEV2_TELNET_CFG '\''{"telnetLocalEnabled":"0","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'\'''
echo '  detectic set DEV2_LIFEMOTE_AGENT '\''{"enable":"0","URL":"","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'\'''
