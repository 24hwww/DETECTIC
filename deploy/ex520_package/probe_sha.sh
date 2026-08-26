#!/bin/sh
# Detectic binary SHA256 probe — uses the same pattern as led_test.sh (proven working)
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
LOG="/var/tmp/detectic_sha_probe.log"

log() { echo "$@" >> "$LOG" 2>/dev/null; }

log "=== SHA PROBE START ==="

# Find the binary
BIN=$($BB find /var /tmp -name 'detectic' -type f 2>/dev/null)
log "BIN_FOUND=$BIN"

# For each found binary, compute SHA
for b in $BIN; do
    SHA=$($BB sha256sum "$b" 2>/dev/null | $BB awk '{print $1}')
    SIZE=$($BB ls -la "$b" 2>/dev/null | $BB awk '{print $5}')
    log "BIN=$b SHA=$SHA SIZE=$SIZE"
done

# Find PID
PID=$($BB ps | $BB grep -i detectic | $BB grep -v grep | $BB head -1 | $BB awk '{print $1}')
log "PID=$PID"

# Find proc exe
if [ -n "$PID" ] && [ -d "/proc/$PID" ]; then
    EXE=$($BB readlink /proc/$PID/exe 2>/dev/null)
    log "EXE=$EXE"
    log "CMDLINE=$($BB cat /proc/$PID/cmdline 2>/dev/null | $BB tr '\0' ' ')"
fi

# Spool
SPOOL=$($BB wc -c /var/run/misc/misc_rw/detectic/spool/detectic_buffer.jsonl 2>/dev/null | $BB awk '{print $1}')
log "SPOOL_SIZE=$SPOOL"

log "=== SHA PROBE END ==="

# Send via GET callback (same pattern as led_test.sh which worked)
CALLBACK="http://192.168.0.27:8080"
SHA=$($BB sha256sum /var/run/misc/misc_rw/detectic/detectic 2>/dev/null | $BB awk '{print $1}')
SHA2=$($BB sha256sum /var/tmp/detectic/detectic 2>/dev/null | $BB awk '{print $1}')
$BB wget -q -T 5 -O /dev/null "${CALLBACK}/done?status=ok&reason=sha_probe&sha1=${SHA}&sha2=${SHA2}&pid=${PID}&spool=${SPOOL}" 2>/dev/null || true
