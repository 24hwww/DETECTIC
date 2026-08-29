#!/bin/sh
CB="http://192.168.0.27:8080"
BB=/bin/busybox
DIR="/var/run/misc/misc_rw/detectic"
TMPDIR="/var/tmp/detectic"
OUT="/tmp/probe.txt"
$BB rm -f "$OUT"
$BB touch "$OUT"
echo "=== uptime ===" >> "$OUT"
$BB cat /proc/uptime >> "$OUT"
echo "=== ps ===" >> "$OUT"
$BB ps 2>/dev/null | $BB grep '[d]etectic' >> "$OUT" 2>&1
echo "=== detectic.pid ===" >> "$OUT"
$BB cat "$DIR/detectic.pid" 2>/dev/null >> "$OUT"
echo "=== autostart.log ===" >> "$OUT"
$BB tail -c 10240 "$DIR/autostart.log" 2>/dev/null >> "$OUT"
echo "=== detectic.log ===" >> "$OUT"
$BB tail -c 51200 "$DIR/detectic.log" 2>/dev/null >> "$OUT"
echo "=== files ===" >> "$OUT"
$BB ls -la "$DIR" "$TMPDIR" 2>/dev/null >> "$OUT"
$BB ls -la "$TMPDIR" 2>/dev/null >> "$OUT"
/usr/sbin/curl -m 15 -T "$OUT" "${CB}/proof_log?tag=log_probe" 2>/dev/null || true
