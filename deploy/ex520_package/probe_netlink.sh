#!/bin/sh
# nrd netlink probe bootstrap for EX520
# Downloads nrd_probe, runs it, posts results back to the package server.
# Runs as root from /usr/bin/phoenix.sh

trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

BASE="http://192.168.0.27:8080"
CALLBACK_BASE="${CALLBACK_BASE:-http://192.168.0.27:8080}"
TMPDIR="/var/tmp/nrd_probe"
BIN="$TMPDIR/nrd_probe"
LOG="$TMPDIR/probe.log"
OUT="$TMPDIR/probe_output.txt"

up() { read u _ < /proc/uptime; echo "$u"; }
log() { echo "[$(up)] $*" >> "$LOG" 2>/dev/null; }

$BB mkdir -p "$TMPDIR" 2>/dev/null
$BB rm -f "$BIN" "$LOG" "$OUT" 2>/dev/null

log "Downloading nrd_probe..."
if ! $BB wget -q -T 60 -O "$BIN" "${BASE}/nrd_probe"; then
    log "ERROR: download failed"
    $BB wget -q -T 5 -O /dev/null "${CALLBACK_BASE}/done?status=fail&reason=download" 2>/dev/null || true
    exit 0
fi
$BB chmod +x "$BIN"
log "Downloaded $(wc -c < "$BIN") bytes"

# Collect environment info into the probe output file
{
echo "=== /proc/net/wireless ==="
$BB cat /proc/net/wireless 2>/dev/null || echo "(not available)"
echo ""
echo "=== iwlist ra0 scan (first 30 lines) ==="
$BB iwlist ra0 scan 2>/dev/null | head -30 || echo "(failed)"
echo ""
echo "=== iwlist rai0 scan (first 30 lines) ==="
$BB iwlist rai0 scan 2>/dev/null | head -30 || echo "(failed)"
echo ""
echo "=== iwpriv ra0 stat ==="
$BB iwpriv ra0 stat 2>/dev/null | head -30 || echo "(failed)"
echo ""
echo "=== iwpriv rai0 stat ==="
$BB iwpriv rai0 stat 2>/dev/null | head -30 || echo "(failed)"
echo ""
echo "=== /tmp/ai_roaming/ar_pat/staInfo ==="
$BB cat /tmp/ai_roaming/ar_pat/staInfo 2>/dev/null | head -30 || echo "(not available)"
echo ""
echo "=== ls /var/tmp/ ==="
$BB ls -la /var/tmp/ 2>/dev/null
echo ""
echo "=== ls /var/tmp/45 ==="
$BB ls -la /var/tmp/45 2>/dev/null || echo "(no /var/tmp/45)"
echo ""
echo "=== ps | grep nrd ==="
$BB ps 2>/dev/null | $BB grep -i nrd || echo "(nrd not found)"
echo ""
echo "=== /proc/net/netlink ==="
$BB cat /proc/net/netlink 2>/dev/null || echo "(not available)"
echo ""
echo "=== ifconfig ==="
$BB ifconfig 2>/dev/null | head -30
echo ""
echo "=== nrd_probe output ==="
} > "$OUT" 2>&1

log "Running nrd_probe (30s)..."
"$BIN" >> "$OUT" 2>&1
PROBE_EXIT=$?
log "Probe exited with code $PROBE_EXIT"

# Extract key results
EVENTS=$($BB grep -o 'events=[0-9]*' "$OUT" 2>/dev/null | $BB head -1 | $BB cut -d= -f2)
[ -z "$EVENTS" ] && EVENTS=0

# Also write to the detectic sensor log location so the existing sensor can upload it
SENSORLOG="/var/run/misc/misc_rw/detectic/sensor_log.txt"
$BB mkdir -p /var/run/misc/misc_rw/detectic 2>/dev/null
echo "===== nrd_probe results (exit=$PROBE_EXIT events=$EVENTS) =====" >> "$SENSORLOG" 2>/dev/null
$BB cat "$OUT" >> "$SENSORLOG" 2>/dev/null

log "Summary: events=$EVENTS exit=$PROBE_EXIT"

# Send done callback with key results
$BB wget -q -T 5 -O /dev/null "${CALLBACK_BASE}/done?status=ok&probe_exit=${PROBE_EXIT}&events=${EVENTS}" 2>/dev/null || true

# Try to upload the log via POST (busybox wget on some builds supports --post-data)
LOGDATA=$($BB cat "$OUT" 2>/dev/null | $BB head -c 4000)
if [ -n "$LOGDATA" ]; then
    # Use a temp file for the POST body
    echo "$LOGDATA" > "$TMPDIR/upload.txt" 2>/dev/null
    $BB wget -q -T 15 -O /dev/null --post-file="$TMPDIR/upload.txt" "${CALLBACK_BASE}/probe_log" 2>/dev/null || true
fi

log "Done."
exit 0
