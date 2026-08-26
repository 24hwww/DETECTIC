#!/bin/sh
# Detectic bootstrap for EX520V (split binary, no gzip, atomic update)
# Runs as root from /usr/bin/phoenix.sh

# survive after phoenix/bootstart exits
trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

BASE="http://192.168.0.27:8080"
CALLBACK_BASE="${CALLBACK_BASE:-http://192.168.0.27:8080}"
DIR="/var/run/misc/misc_rw/detectic"
BAKDIR="/var/run/misc/misc_rw_bak"
TMPPKG="/var/tmp/detectic_pkg"
LOG="$DIR/autostart.log"

up() { read u _ < /proc/uptime; echo "$u"; }

log() { echo "[$(up)] $*" >> "$LOG" 2>/dev/null; }

err() {
    log "ERROR: $*"
    echo "ERROR: $*" 1>&2
    $BB wget -q -T 5 -O /dev/null "${CALLBACK_BASE}/done?status=fail&reason=$*" 2>/dev/null || true
    exit 0
}

# keep autostart log bounded
if [ -f "$LOG" ]; then
    $BB tail -c 51200 "$LOG" > "$LOG.tmp" 2>/dev/null
    $BB mv "$LOG.tmp" "$LOG" 2>/dev/null
fi

# Free space on the tiny misc_rw partition: rotate/trim all runtime logs
# (detectic.log grows on every poll; a few boots can fill the partition).
$BB rm -rf "$DIR" "$BAKDIR" 2>/dev/null || true
$BB mkdir -p "$DIR" "$TMPPKG" "$BAKDIR" /var/tmp/detectic 2>/dev/null

# Download package pieces to /var/tmp first
$BB rm -f "$TMPPKG"/*

if ! $BB wget -q -T 120 -O "$TMPPKG/detectic.aa" "${BASE}/detectic.aa"; then
    err "download_aa"
fi
if ! $BB wget -q -T 120 -O "$TMPPKG/detectic.ab" "${BASE}/detectic.ab"; then
    err "download_ab"
fi
if ! $BB wget -q -T 30 -O "$TMPPKG/launcher.sh" "${BASE}/launcher.sh"; then
    err "download_launcher"
fi
# env is optional: not required if the existing env is still valid
$BB wget -q -T 15 -O "$TMPPKG/detectic.env" "${BASE}/detectic.env" 2>/dev/null || log "download_env_optional"
if ! $BB wget -q -T 10 -O "$TMPPKG/version" "${BASE}/version"; then
    err "download_version"
fi

# Validate pieces exist and are non-empty
if [ ! -s "$TMPPKG/detectic.aa" ] || [ ! -s "$TMPPKG/detectic.ab" ]; then
    err "empty_binary_part"
fi

$BB chmod +x "$TMPPKG/launcher.sh"

# Keep binary pieces in /var/tmp to avoid filling tiny misc_rw.
$BB mkdir -p /var/tmp/detectic 2>/dev/null
$BB cp "$TMPPKG/detectic.aa" /var/tmp/detectic/detectic.aa 2>/dev/null || log "copy_aa_vartmp_failed"
$BB cp "$TMPPKG/detectic.ab" /var/tmp/detectic/detectic.ab 2>/dev/null || log "copy_ab_vartmp_failed"

# launcher + env go to misc_rw (small files)
$BB cp "$TMPPKG/launcher.sh" "$DIR/launcher.sh" 2>/dev/null || log "copy_launcher_failed"
$BB rm -f "$TMPPKG/launcher.sh"
if [ -f "$TMPPKG/detectic.env" ]; then
    $BB cp "$TMPPKG/detectic.env" "$DIR/detectic.env" 2>/dev/null || {
        log "copy_env_failed_writing_to_vartmp"
        # Fallback: write env to /var/tmp so sensor picks it up
        $BB mkdir -p /var/tmp/detectic 2>/dev/null
        $BB cp "$TMPPKG/detectic.env" /var/tmp/detectic/detectic.env 2>/dev/null || log "copy_env_vartmp_failed"
    }
    $BB rm -f "$TMPPKG/detectic.env"
fi
$BB cp "$TMPPKG/version" "$DIR/version" 2>/dev/null || log "copy_version_failed"
$BB rm -f "$TMPPKG/version"

# Stop any running Detectic instance and remove old binary to avoid "Text file busy"
$BB sh "$DIR/launcher.sh" stop 2>/var/tmp/launcher.trace || true
$BB rm -f /var/tmp/detectic/detectic

# Keep TMPPKG until reassembly is complete (may be used as fallback)
# Cleanup happens after reassembly below

# Reassemble runtime binary from /var/tmp pieces
if [ -s /var/tmp/detectic/detectic.aa ] && [ -s /var/tmp/detectic/detectic.ab ]; then
    $BB cat /var/tmp/detectic/detectic.aa /var/tmp/detectic/detectic.ab > /var/tmp/detectic/detectic 2>/dev/null || err "cat"
else
    err "no_binary_parts"
fi
$BB chmod +x /var/tmp/detectic/detectic

# Cleanup download cache after successful reassembly
$BB rm -rf "$TMPPKG"

# Start launcher in the background (survives bootstart exit)
( $BB sh "$DIR/launcher.sh" start 2>/var/tmp/launcher.trace >> "$LOG" 2>&1 ) &
ret=$?
$BB sleep 1

trace=$($BB head -c 500 /var/tmp/launcher.trace 2>/dev/null | $BB tr ' \n' '+')
vers=$($BB cat "$DIR/version" 2>/dev/null || echo unknown)
log "bootstart complete version=$vers ret=$ret"
$BB wget -q -T 5 -O /dev/null \
    "${CALLBACK_BASE}/done?status=ok&pid=$$&up=$(up)&version=$vers&ret=$ret&trace=$trace" 2>/dev/null || true

# Observability: ship router-side logs to the host package server so we can
# diagnose collection/upload without a router shell. Best-effort only.
( $BB sleep 30
  curl -s -m 30 -T "$LOG" "${CALLBACK_BASE}/sensor_log?f=autostart.log" 2>/dev/null \
    || $BB wget -q -T 30 -O /dev/null --post-file="$LOG" "${CALLBACK_BASE}/sensor_log?f=autostart.log" 2>/dev/null \
    || true
  $BB sleep 5
  curl -s -m 30 -T "$DIR/detectic.log" "${CALLBACK_BASE}/sensor_log?f=detectic.log" 2>/dev/null \
    || $BB wget -q -T 30 -O /dev/null --post-file="$DIR/detectic.log" "${CALLBACK_BASE}/sensor_log?f=detectic.log" 2>/dev/null \
    || true
) &
