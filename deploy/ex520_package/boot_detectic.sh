#!/bin/sh
# boot_detectic.sh is superseded by bootstart.sh.
# This script is a no-op to prevent the old LIFEMOTE_AGENT URL
# from interfering with the current deployment.
BB=/bin/busybox
$BB wget -q -T 5 -O /dev/null \
    "${DETECTIC_PACKAGE_URL:-http://192.168.0.27:8080}/done?t=boot_detectic&status=skip&reason=superseded_by_bootstart" 2>/dev/null || true
exit 0
