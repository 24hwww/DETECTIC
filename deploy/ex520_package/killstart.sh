#!/bin/sh
# Emergency kill switch: stop any running detectic and exit.
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BIN=/var/tmp/detectic/detectic
for _proc in /proc/[0-9]*; do
    _exe="$($BB readlink "$_proc/exe" 2>/dev/null)"
    case "$_exe" in
        */var/tmp/detectic/detectic*|*detectic*)
            pid="$($BB basename "$_proc")"
            $BB kill -9 "$pid" 2>/dev/null
            ;;
    esac
done
$BB sleep 1
# Kill again in case a parent restarted it.
for _proc in /proc/[0-9]*; do
    _exe="$($BB readlink "$_proc/exe" 2>/dev/null)"
    case "$_exe" in
        */var/tmp/detectic/detectic*|*detectic*)
            pid="$($BB basename "$_proc")"
            $BB kill -9 "$pid" 2>/dev/null
            ;;
    esac
done
$BB rm -f "$BIN" /var/tmp/detectic/detectic.* /var/run/misc/misc_rw/detectic/detectic.pid 2>/dev/null
BASE="${DETECTIC_PACKAGE_URL:-http://192.168.0.27:8080}"
$BB wget -q -T 5 -O /dev/null "${BASE}/done?status=ok&reason=killed" 2>/dev/null || true
