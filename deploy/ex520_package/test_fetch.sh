#!/bin/sh
CB="http://192.168.0.27:8080"
BB=/bin/busybox
OUT="/tmp/fetch_test.txt"
$BB rm -f "$OUT" "$OUT.wget" "$OUT.curl"
$BB touch "$OUT"

echo "=== wget launcher ===" >> "$OUT"
$BB wget -T 30 -O "$OUT.wget" "${CB}/launcher.sh" 2>&1 >> "$OUT"
echo "size=$($BB ls -la "$OUT.wget" 2>/dev/null | $BB awk '{print $5}')" >> "$OUT"

echo "=== curl launcher ===" >> "$OUT"
/usr/sbin/curl -m 30 -o "$OUT.curl" "${CB}/launcher.sh" 2>&1 >> "$OUT"
echo "size=$($BB ls -la "$OUT.curl" 2>/dev/null | $BB awk '{print $5}')" >> "$OUT"

echo "=== wget bootstart ===" >> "$OUT"
$BB rm -f "$OUT.boot"
$BB wget -T 30 -O "$OUT.boot" "${CB}/bootstart.sh" 2>&1 >> "$OUT"
echo "size=$($BB ls -la "$OUT.boot" 2>/dev/null | $BB awk '{print $5}')" >> "$OUT"

echo "=== head launcher ===" >> "$OUT"
$BB head -n 20 "$OUT.wget" 2>/dev/null >> "$OUT"

echo "=== tail launcher ===" >> "$OUT"
$BB tail -n 20 "$OUT.wget" 2>/dev/null >> "$OUT"

/usr/sbin/curl -m 15 -T "$OUT" "${CB}/proof_log?tag=test_fetch" 2>/dev/null || true
