#!/bin/sh
# Definitive auto-start test script
# Phase 1 (first run): Send callback, write marker, reboot
# Phase 2 (after reboot): If phoenix.sh auto-starts, this script runs again
#   -> Send "auto_started" callback with low uptime
# If phoenix.sh does NOT auto-start, no second callback will be received
trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BASE="http://192.168.0.27:8080"
MARKER="/var/run/misc/misc_rw/detectic/auto_start_test.txt"
UPTIME=$($BB cat /proc/uptime 2>/dev/null | $BB awk '{print $1}')

$BB mkdir -p /var/run/misc/misc_rw/detectic 2>/dev/null

# Check if this is a post-reboot run (uptime < 300 = less than 5 minutes)
if [ -f "$MARKER" ]; then
    MARKER_CONTENT=$($BB cat "$MARKER" 2>/dev/null)
    PREV_REBOOT_UPTIME=$($BB echo "$MARKER_CONTENT" | $BB grep '^reboot_at_uptime=' | $BB cut -d= -f2)

    # If uptime is low (< 300s) and marker exists, this is likely a post-reboot auto-start
    IS_LOW_UPTIME=0
    if [ $(echo "$UPTIME < 300" | bc 2>/dev/null) = "1" ]; then
        IS_LOW_UPTIME=1
    fi

    # Also check if uptime is less than previous (definite reboot)
    IS_REBOOT=0
    if [ -n "$PREV_REBOOT_UPTIME" ]; then
        if [ $(echo "$UPTIME < $PREV_REBOOT_UPTIME" | bc 2>/dev/null) = "1" ]; then
            IS_REBOOT=1
        fi
    fi

    if [ "$IS_LOW_UPTIME" = "1" ] || [ "$IS_REBOOT" = "1" ]; then
        # This is a post-reboot auto-start!
        {
        echo "AUTO_STARTED_AFTER_REBOOT"
        echo "current_uptime=$UPTIME"
        echo "prev_reboot_uptime=$PREV_REBOOT_UPTIME"
        echo "date=$($BB date 2>/dev/null)"
        echo "pid=$$"
        echo "ppid=$PPID"
        $BB ps 2>/dev/null | $BB grep -E 'phoenix|cos|init' | head -10
        } > "$MARKER" 2>/dev/null

        echo "[${UPTIME}] AUTO_STARTED_AFTER_REBOOT pid=$$ ppid=$PPID" >> /var/run/misc/misc_rw/detectic/autostart.log 2>/dev/null

        # Send auto_started callback
        $BB wget -q -T 5 -O /dev/null "${BASE}/done?status=auto_started&uptime=${UPTIME}&pid=$$&ppid=$PPID" 2>/dev/null || true

        # Exit - don't reboot again
        exit 0
    fi
fi

# First run - write marker and reboot
{
echo "first_run"
echo "reboot_at_uptime=$UPTIME"
echo "date=$($BB date 2>/dev/null)"
echo "pid=$$"
} > "$MARKER" 2>/dev/null

echo "[${UPTIME}] auto_start_test_first_run pid=$$ - will reboot" >> /var/run/misc/misc_rw/detectic/autostart.log 2>/dev/null

# Send first_run callback
$BB wget -q -T 5 -O /dev/null "${BASE}/done?status=first_run&uptime=${UPTIME}&pid=$$" 2>/dev/null || true

# Wait for callback to be sent
sleep 3

# Sync and reboot
sync
sleep 1
reboot

# If reboot didn't work, send failure callback
sleep 5
$BB wget -q -T 3 -O /dev/null "${BASE}/done?status=reboot_failed&uptime=${UPTIME}" 2>/dev/null || true

exit 0
