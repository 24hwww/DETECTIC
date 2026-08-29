#!/bin/sh
CB="http://192.168.0.27:8080"
BB=/bin/busybox
DIR="/var/run/misc/misc_rw/detectic"
TMPDIR="/var/tmp/detectic"
OUT="/tmp/debug2.txt"
$BB rm -f "$OUT"
$BB touch "$OUT"

echo "=== uptime ===" >> "$OUT"
$BB cat /proc/uptime >> "$OUT"

echo "=== env copy ===" >> "$OUT"
$BB cp "$TMPDIR/detectic.env" "$DIR/detectic.env" 2>&1 >> "$OUT"
$BB ls -la "$TMPDIR/detectic.env" "$DIR/detectic.env" >> "$OUT"

echo "=== version copy ===" >> "$OUT"
$BB cp "$TMPDIR/version" "$DIR/version" 2>&1 >> "$OUT"
$BB ls -la "$TMPDIR/version" "$DIR/version" >> "$OUT"

echo "=== launcher copy ===" >> "$OUT"
$BB cp "$TMPDIR/launcher.sh" "$DIR/launcher.sh" 2>&1 >> "$OUT"
$BB ls -la "$TMPDIR/launcher.sh" "$DIR/launcher.sh" >> "$OUT"

echo "=== clear logs ===" >> "$OUT"
$BB rm -f "$DIR/detectic.log" "$DIR/detectic.pid" "$DIR/autostart.log" 2>&1 >> "$OUT"

echo "=== start via launcher ===" >> "$OUT"
sh "$DIR/launcher.sh" start 2>&1 | $BB head -c 500 >> "$OUT"

echo "" >> "$OUT"
echo "=== wait 10s ===" >> "$OUT"
$BB sleep 10

echo "=== ps ===" >> "$OUT"
$BB ps 2>/dev/null | $BB grep '[d]etectic' >> "$OUT" 2>&1

echo "=== pid ===" >> "$OUT"
$BB cat "$DIR/detectic.pid" 2>/dev/null >> "$OUT"

echo "=== detectic.log head ===" >> "$OUT"
$BB head -n 60 "$DIR/detectic.log" 2>/dev/null >> "$OUT"

echo "=== detectic.log tail ===" >> "$OUT"
$BB tail -n 60 "$DIR/detectic.log" 2>/dev/null >> "$OUT"

echo "=== files ===" >> "$OUT"
$BB ls -la "$TMPDIR" "$DIR" 2>/dev/null >> "$OUT"

/usr/sbin/curl -m 15 -T "$OUT" "${CB}/proof_log?tag=debug2" 2>/dev/null || true
