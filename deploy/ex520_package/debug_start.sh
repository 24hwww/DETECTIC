#!/bin/sh
CB="http://192.168.0.27:8080"
BB=/bin/busybox
DIR="/var/run/misc/misc_rw/detectic"
TMPDIR="/var/tmp/detectic"
OUT="/tmp/debug.txt"
$BB rm -f "$OUT"
$BB touch "$OUT"

# source env
[ -f "$TMPDIR/detectic.env" ] && . "$TMPDIR/detectic.env" 2>/dev/null
[ -f "$DIR/detectic.env" ] && . "$DIR/detectic.env" 2>/dev/null

echo "=== uptime ===" >> "$OUT"
$BB cat /proc/uptime >> "$OUT"

echo "=== version ===" >> "$OUT"
"$TMPDIR/detectic" version 2>&1 | $BB head -c 500 >> "$OUT" 2>&1

echo "=== clear logs ===" >> "$OUT"
$BB rm -f "$DIR/detectic.log" "$DIR/detectic.pid" "$DIR/autostart.log" 2>&1 >> "$OUT"

echo "=== start detectic ===" >> "$OUT"
( trap '' 1; exec "$TMPDIR/detectic" sensor >> "$DIR/detectic.log" 2>&1 ) &
new_pid=$!
echo "$new_pid" > "$DIR/detectic.pid"
echo "started PID=$new_pid" >> "$OUT"

$BB sleep 5

echo "=== ps ===" >> "$OUT"
$BB ps 2>/dev/null | $BB grep '[d]etectic' >> "$OUT" 2>&1

echo "=== pid ===" >> "$OUT"
$BB cat "$DIR/detectic.pid" 2>/dev/null >> "$OUT"

echo "=== detectic.log head ===" >> "$OUT"
$BB head -n 50 "$DIR/detectic.log" 2>/dev/null >> "$OUT"

echo "=== detectic.log tail ===" >> "$OUT"
$BB tail -n 50 "$DIR/detectic.log" 2>/dev/null >> "$OUT"

echo "=== files ===" >> "$OUT"
$BB ls -la "$TMPDIR" "$DIR" 2>/dev/null >> "$OUT"

/usr/sbin/curl -m 15 -T "$OUT" "${CB}/proof_log?tag=debug" 2>/dev/null || true
