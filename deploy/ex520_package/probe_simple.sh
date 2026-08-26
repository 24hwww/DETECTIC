#!/bin/sh
# Detectic binary probe — simple version with --post-data
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

# Find the binary
BIN=$($BB find /var /tmp -name 'detectic' -type f 2>/dev/null | $BB head -1)
SHA=$($BB sha256sum "$BIN" 2>/dev/null | $BB awk '{print $1}')
SIZE=$($BB ls -la "$BIN" 2>/dev/null | $BB awk '{print $5}')

# Find PID
PID=$($BB ps | $BB grep -i detectic | $BB grep -v grep | $BB head -1 | $BB awk '{print $1}')

# Build a compact data string
DATA="bin=${BIN}|sha=${SHA}|size=${SIZE}|pid=${PID}"

# Send via --post-data (small string, should work with busybox wget)
$BB wget -q -T 10 -O /dev/null --post-data "$DATA" "http://192.168.0.27:8080/sensor_log?f=binary_probe" 2>/dev/null
