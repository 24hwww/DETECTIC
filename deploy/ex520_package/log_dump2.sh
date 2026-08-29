#!/bin/sh
CB="http://192.168.0.27:8080"
BB=/bin/busybox
DIR="/var/run/misc/misc_rw/detectic"
TMPDIR="/var/tmp/detectic"
OUT="/tmp/log_dump2.txt"
$BB rm -f "$OUT"
$BB touch "$OUT"

echo "=== uptime ===" >> "$OUT"
$BB cat /proc/uptime >> "$OUT"

echo "=== ps ===" >> "$OUT"
$BB ps 2>/dev/null | $BB grep '[d]etectic' >> "$OUT" 2>&1

echo "=== detectic.pid ===" >> "$OUT"
$BB cat "$DIR/detectic.pid" 2>/dev/null >> "$OUT"

echo "=== autostart.log ===" >> "$OUT"
$BB tail -n 50 "$DIR/autostart.log" 2>/dev/null >> "$OUT"

echo "=== autostart.log.tmp ===" >> "$OUT"
$BB tail -n 50 "$DIR/autostart.log.tmp" 2>/dev/null >> "$OUT"

echo "=== detectic.log head ===" >> "$OUT"
$BB head -n 80 "$DIR/detectic.log" 2>/dev/null >> "$OUT"

echo "=== detectic.log tail ===" >> "$OUT"
$BB tail -n 80 "$DIR/detectic.log" 2>/dev/null >> "$OUT"

echo "=== files ===" >> "$OUT"
$BB ls -la "$TMPDIR" "$DIR" 2>/dev/null >> "$OUT"

/usr/sbin/curl -m 15 -T "$OUT" "${CB}/proof_log?tag=log_dump2" 2>/dev/null || true
