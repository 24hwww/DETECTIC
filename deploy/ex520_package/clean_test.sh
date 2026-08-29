#!/bin/sh
# Clean auto-start test with sysrq reboot
# Phase 1: Write marker, send callback, force reboot via sysrq
# Phase 2 (after reboot, if auto-start works): Detect marker, send auto_started callback
trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BASE="http://192.168.0.27:8080"
MARKER="/var/run/misc/misc_rw/detectic/clean_test_marker.txt"
UPTIME=$($BB cat /proc/uptime 2>/dev/null | $BB awk '{print $1}')

$BB mkdir -p /var/run/misc/misc_rw/detectic 2>/dev/null

# Check if this is a post-reboot run
if [ -f "$MARKER" ]; then
    PREV_UPTIME=$($BB cat "$MARKER" 2>/dev/null | $BB grep '^prev_uptime=' | $BB cut -d= -f2)

    # If uptime < 300, this is likely a post-reboot auto-start
    if [ $(echo "$UPTIME < 300" 2>/dev/null) = "1" ] || [ "$UPTIME" -lt 300 ] 2>/dev/null; then
        # AUTO-STARTED after reboot!
        {
        echo "AUTO_STARTED_AFTER_REBOOT"
        echo "current_uptime=$UPTIME"
        echo "prev_uptime=$PREV_UPTIME"
        echo "pid=$$"
        echo "ppid=$PPID"
        $BB ps 2>/dev/null | $BB grep -E 'phoenix|cos' | head -5
        } > "$MARKER" 2>/dev/null

        echo "[${UPTIME}] CLEAN_TEST_AUTO_STARTED pid=$$ ppid=$PPID" >> /var/run/misc/misc_rw/detectic/autostart.log 2>/dev/null

        $BB wget -q -T 5 -O /dev/null "${BASE}/done?status=clean_auto_started&uptime=${UPTIME}&pid=$$&ppid=$PPID" 2>/dev/null || true
        exit 0
    fi
fi

# First run - write marker and force reboot
echo "prev_uptime=$UPTIME" > "$MARKER" 2>/dev/null
echo "pid=$$" >> "$MARKER" 2>/dev/null

echo "[${UPTIME}] clean_test_first_run pid=$$ - will force reboot" >> /var/run/misc/misc_rw/detectic/autostart.log 2>/dev/null

# Send first_run callback
$BB wget -q -T 5 -O /dev/null "${BASE}/done?status=clean_first_run&uptime=${UPTIME}&pid=$$" 2>/dev/null || true

# Wait for callback
sleep 3
sync

# Force reboot via sysrq (more reliable than reboot command)
echo b > /proc/sysrq-trigger 2>/dev/null

# If sysrq didn't work, try regular reboot
sleep 2
reboot 2>/dev/null

# If we get here, both methods failed
sleep 3
$BB wget -q -T 3 -O /dev/null "${BASE}/done?status=clean_reboot_failed&uptime=${UPTIME}" 2>/dev/null || true

exit 0
