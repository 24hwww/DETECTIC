#!/bin/sh
# Read LED test log and report via callback
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
LOG="/var/tmp/detectic_led_test.log"

# Read the log and send it as a URL-encoded callback
if [ -f "$LOG" ]; then
    CONTENT=$($BB cat "$LOG" 2>/dev/null | $BB tr '\n' '|' | $BB sed 's/ /_/g')
else
    CONTENT="no_log_file"
fi

# Send to a simple endpoint
$BB wget -q -T 5 -O /dev/null "http://192.168.0.27:8080/led_log?data=${CONTENT}" 2>/dev/null || true
