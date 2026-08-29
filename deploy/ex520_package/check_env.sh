#!/bin/sh
CB="http://192.168.0.27:8080"
BB=/bin/busybox
DIR="/var/run/misc/misc_rw/detectic"
TMPDIR="/var/tmp/detectic"
OUT="/tmp/check_env.txt"
$BB rm -f "$OUT"
$BB touch "$OUT"

echo "=== uptime ===" >> "$OUT"
$BB cat /proc/uptime >> "$OUT"

echo "=== dir env ===" >> "$OUT"
$BB ls -la "$DIR/detectic.env" "$DIR/.env" "$TMPDIR/detectic.env" "$TMPDIR/.env" 2>/dev/null >> "$OUT"

echo "=== grep password dir detectic.env (count) ===" >> "$OUT"
$BB grep -c '^DETECTIC_PASSWORD=' "$DIR/detectic.env" 2>/dev/null >> "$OUT"

echo "=== grep secret dir detectic.env (count) ===" >> "$OUT"
$BB grep -c '^DETECTIC_SECRET=' "$DIR/detectic.env" 2>/dev/null >> "$OUT"

echo "=== env keys only (sensitive names masked, values NEVER logged) ===" >> "$OUT"
$BB grep -E '^[A-Za-z0-9_]+=' "$DIR/detectic.env" 2>/dev/null | $BB cut -d'=' -f1 | \
  $BB sed -e 's/^DETECTIC_PASSWORD$/secret-key/' \
          -e 's/^DETECTIC_SECRET$/secret-key/' \
          -e 's/^DETECTIC_BACKEND_TOKEN$/secret-key/' \
          -e 's/^DETECTIC_SMTP_PASSWORD$/secret-key/' \
          -e 's/^DETECTIC_SMTP_USER$/secret-key/' \
          -e 's/^DETECTIC_D1_SYNC_URL$/secret-key/' \
          -e 's/^PASSWORD$/secret-key/' \
          -e 's/^SECRET$/secret-key/' >> "$OUT"

/usr/sbin/curl -m 15 -T "$OUT" "${CB}/proof_log?tag=check_env" 2>/dev/null || true
