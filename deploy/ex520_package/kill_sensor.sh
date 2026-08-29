#!/bin/sh
# kill_sensor.sh — simulated crash for Test 2.
# Kills the Detectic sensor process but NOT the watchdog.
# Run via phoenix (DEV2_LIFEMOTE_AGENT toggle).
trap '' 1 15
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BASE="http://192.168.0.27:8080"

# Send callback before killing.
$BB wget -q -T 3 -O /dev/null "${BASE}/done?status=kill_sensor_start&up=$(read u _ < /proc/uptime; echo $u)" 2>/dev/null || true

# Kill ONLY the detectic sensor process (not the watchdog, not ourselves).
for _proc in /proc/[0-9]*/cmdline; do
    [ -f "$_proc" ] || continue
    _cmd="$($BB tr '\0' ' ' < "$_proc" 2>/dev/null)"
    case "$_cmd" in
        *detectic_watchdog*) ;;  # spare the watchdog
        *detectic*sensor*)
            _spid="$($BB echo "$_proc" | $BB sed 's|/proc/||;s|/cmdline||')"
            $BB wget -q -T 3 -O /dev/null "${BASE}/done?status=killing_pid=${_spid}" 2>/dev/null || true
            $BB kill -9 "$_spid" 2>/dev/null || true
            ;;
    esac
done

$BB sleep 2
$BB wget -q -T 3 -O /dev/null "${BASE}/done?status=kill_sensor_done" 2>/dev/null || true
exit 0
