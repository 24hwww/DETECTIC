#!/bin/sh
# Combined boot detection + reboot script
# First run: writes marker, waits, then reboots
# Second run (after reboot): writes "auto_started" marker, sends callback
trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BASE="http://192.168.0.27:8080"
MARKER="/var/run/misc/misc_rw/detectic/boot_marker.txt"
UPTIME=$($BB cat /proc/uptime 2>/dev/null | $BB awk '{print $1}')

# Ensure detectic dir exists
$BB mkdir -p /var/run/misc/misc_rw/detectic 2>/dev/null

if [ -f "$MARKER" ]; then
    # This is a subsequent run - check if it's after a reboot
    PREV_UPTIME=$($BB cat "$MARKER" 2>/dev/null | $BB grep '^reboot_uptime=' | $BB cut -d= -f2)
    CURRENT_UPTIME=$UPTIME

    # If current uptime is less than previous, we rebooted
    # Also check if the marker says "rebooted"
    REBOOTED=$($BB cat "$MARKER" 2>/dev/null | $BB grep '^rebooted=1')

    if [ -n "$REBOOTED" ] || [ $(echo "$CURRENT_UPTIME < $PREV_UPTIME" | bc 2>/dev/null) = "1" ]; then
        # This is an auto-start after reboot!
        {
        echo "AUTO_STARTED_AFTER_REBOOT"
        echo "uptime=$CURRENT_UPTIME"
        echo "prev_uptime=$PREV_UPTIME"
        echo "date=$($BB date 2>/dev/null)"
        echo "pid=$$"
        echo "ppid=$PPID"
        $BB ps 2>/dev/null | $BB grep -E 'phoenix|cos|init' | head -10
        } > "$MARKER" 2>/dev/null

        # Append to autostart log
        echo "[${CURRENT_UPTIME}] AUTO_STARTED_AFTER_REBOOT pid=$$ ppid=$PPID" >> /var/run/misc/misc_rw/detectic/autostart.log 2>/dev/null

        # Send callback
        $BB wget -q -T 5 -O /dev/null "${BASE}/done?status=auto_started&uptime=${CURRENT_UPTIME}&pid=$$" 2>/dev/null || true
        exit 0
    fi
fi

# First run (or no marker) - write marker and reboot
{
echo "first_run"
echo "reboot_uptime=$UPTIME"
echo "rebooted=1"
echo "pid=$$"
echo "date=$($BB date 2>/dev/null)"
} > "$MARKER" 2>/dev/null

# Append to autostart log
echo "[${UPTIME}] boot_test_first_run pid=$$ - will reboot" >> /var/run/misc/misc_rw/detectic/autostart.log 2>/dev/null

# Send pre-reboot callback
$BB wget -q -T 3 -O /dev/null "${BASE}/done?status=pre_reboot&uptime=${UPTIME}&pid=$$" 2>/dev/null || true

# Wait a moment for the callback to be sent
sleep 2

# Sync and reboot
sync
sleep 1
reboot

exit 0
