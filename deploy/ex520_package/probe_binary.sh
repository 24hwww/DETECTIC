#!/bin/sh
# Detectic deployed binary probe — collects runtime evidence from the EX520
# Runs as root via phoenix.sh
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

# 1. Find running detectic process
PID=$($BB ps | $BB grep -v grep | $BB grep 'detectic' | $BB head -1 | $BB awk '{print $1}')
if [ -z "$PID" ]; then
    PID=$($BB ps | $BB grep -v grep | $BB grep '/var/tmp/detectic' | $BB head -1 | $BB awk '{print $1}')
fi

# 2. Binary path
BIN_PATH="/var/tmp/detectic/detectic"
if [ ! -f "$BIN_PATH" ]; then
    # Try misc_rw
    BIN_PATH="/var/run/misc/misc_rw/detectic/detectic"
fi
if [ ! -f "$BIN_PATH" ]; then
    # Search for it
    BIN_PATH=$($BB find /var/tmp /var/run/misc -name 'detectic' -type f 2>/dev/null | $BB head -1)
fi

# 3. Collect evidence
echo "PID=$PID"
echo "BIN_PATH=$BIN_PATH"

if [ -n "$PID" ] && [ -n "$BIN_PATH" ]; then
    # SHA256 of the binary on the router
    SHA=$($BB sha256sum "$BIN_PATH" 2>/dev/null | $BB awk '{print $1}')
    echo "SHA256=$SHA"

    # Binary size
    SIZE=$($BB ls -la "$BIN_PATH" 2>/dev/null | $BB awk '{print $5}')
    echo "SIZE=$SIZE"

    # Process start time (from /proc)
    if [ -d "/proc/$PID" ]; then
        STARTTIME=$($BB cat /proc/$PID/stat 2>/dev/null | $BB awk '{print $22}')
        UPTIME=$($BB cat /proc/uptime 2>/dev/null | $BB awk '{print $1}')
        echo "PROC_STARTTIME_TICKS=$STARTTIME"
        echo "UPTIME_SECS=$UPTIME"
        echo "PROC_CMDLINE=$($BB cat /proc/$PID/cmdline 2>/dev/null | $BB tr '\0' ' ')"
        echo "PROC_ENV_SENSOR_ID=$($BB cat /proc/$PID/environ 2>/dev/null | $BB tr '\0' '\n' | $BB grep DETECTIC_SENSOR_ID | $BB head -1)"
        echo "PROC_ENV_UPLOAD_URL=$($BB cat /proc/$PID/environ 2>/dev/null | $BB tr '\0' '\n' | $BB grep DETECTIC_UPLOAD_URL | $BB head -1)"
    fi
fi

# 4. Check sensor log for version
echo "SENSOR_LOG_TAIL=$($BB tail -5 /var/run/misc/misc_rw/detectic/sensor_log.txt 2>/dev/null | $BB tr '\n' '|')"

# 5. Check autostart log
echo "AUTOSTART_LOG_TAIL=$($BB tail -3 /var/run/misc/misc_rw/detectic/autostart.log 2>/dev/null | $BB tr '\n' '|')"

# 6. Report back via package server
CALLBACK="http://192.168.0.27:8080"
ALL_DATA=$(echo "PID=$PID SHA256=$SHA SIZE=$SIZE" | $BB sed 's/ /_/g')
$BB wget -q -T 5 -O /dev/null "${CALLBACK}/binary_probe?${ALL_DATA}" 2>/dev/null || true
