#!/bin/sh
# Reboot the router from inside (runs as root via Phoenix)
trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BASE="http://192.168.0.27:8080"

# Send callback before reboot
UPTIME=$($BB cat /proc/uptime 2>/dev/null | $BB awk '{print $1}')
$BB wget -q -T 3 -O /dev/null "${BASE}/done?status=rebooting&uptime=${UPTIME}" 2>/dev/null || true

# Sync filesystems before reboot
sync
sleep 1

# Reboot the router
reboot

exit 0
