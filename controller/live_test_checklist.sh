#!/bin/sh
# Phase 12F Live Test Checklist for EX520
# Run via Telnet/SSH after management enabled

set -e
echo "=== PHASE12F HARD SAFETY GATE ==="
uname -a
cat /proc/version
cat /etc/platform_ver 2>/dev/null || true
cat /proc/mtd
mount
df -h

echo "=== MISC_RW DISCOVERY ==="
df -h /var/run/misc/misc_rw
mount | grep misc_rw
ubinfo -a 2>/dev/null || true
du -sh /var/run/misc/misc_rw
ls -la /var/run/misc/misc_rw

echo "=== SAFE WRITE PROBE ==="
TESTDIR=/var/run/misc/misc_rw/detectic-validation
mkdir -p $TESTDIR
MARKER=$TESTDIR/marker_$(date +%s)
echo "$(date)" > $MARKER
sha256sum $MARKER
echo "Marker created: $MARKER"

echo "=== REBOOT REQUIRED FOR PERSISTENCE TEST ==="
echo "Reboot router, then re-run to verify marker"
