#!/bin/sh
# Phase 14.3 — Lifemote Autostart Test Payload
# HARMLESS: creates only a volatile marker in /tmp
# NO configuration changes, NO firmware modifications, NO persistence

MARKER="/tmp/lifemote_autostart_test_$(date +%s)_$$"
TIMESTAMP=$(date '+%Y-%m-%d %H:%M:%S')
HOSTNAME=$(hostname 2>/dev/null || echo "unknown")

# Create the marker
echo "LIFEMOTE_AUTOSTART_TEST" > "$MARKER"
echo "timestamp=$TIMESTAMP" >> "$MARKER"
echo "pid=$$" >> "$MARKER"
echo "hostname=$HOSTNAME" >> "$MARKER"
echo "marker_path=$MARKER" >> "$MARKER"

# Signal file for external detection
echo "$MARKER" > /tmp/lifemote_test_marker_path

exit 0
