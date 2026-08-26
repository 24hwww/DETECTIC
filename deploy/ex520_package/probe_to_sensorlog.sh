#!/bin/sh
# Detectic binary probe — writes to sensor_log.txt which the sensor uploads
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
LOG="/var/run/misc/misc_rw/detectic/sensor_log.txt"

# Find the binary
BIN=$($BB find /var /tmp -name 'detectic' -type f 2>/dev/null | $BB head -1)
SHA=$($BB sha256sum "$BIN" 2>/dev/null | $BB awk '{print $1}')
SIZE=$($BB ls -la "$BIN" 2>/dev/null | $BB awk '{print $5}')

# Find PID
PID=$($BB ps | $BB grep -i detectic | $BB grep -v grep | $BB head -1 | $BB awk '{print $1}')

# Find proc exe
EXE=""
if [ -n "$PID" ] && [ -d "/proc/$PID" ]; then
    EXE=$($BB readlink /proc/$PID/exe 2>/dev/null)
fi

# Write to sensor_log.txt (the sensor uploads this periodically)
echo "[probe] bin=$BIN sha=$SHA size=$SIZE pid=$PID exe=$EXE" >> "$LOG" 2>/dev/null

# Also write spool info
SPOOL_SIZE=$($BB wc -c /var/run/misc/misc_rw/detectic/spool/detectic_buffer.jsonl 2>/dev/null | $BB awk '{print $1}')
echo "[probe] spool_size=$SPOOL_SIZE" >> "$LOG" 2>/dev/null

# Write env info
if [ -n "$PID" ] && [ -d "/proc/$PID" ]; then
    $BB cat /proc/$PID/environ 2>/dev/null | $BB tr '\0' '\n' | $BB grep DETECTIC >> "$LOG" 2>/dev/null
fi

echo "[probe] done" >> "$LOG" 2>/dev/null
