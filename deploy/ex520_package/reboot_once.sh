#!/bin/sh
# Reboot the EX520 once and then idle. Used to test cold boot.
# The rebooted_once marker is removed by bootstart.sh on next deployment.
BB=/bin/busybox
DIR="/var/run/misc/misc_rw/detectic"
$BB mkdir -p "$DIR" 2>/dev/null || true
if [ -f "$DIR/rebooted_once" ]; then
    exit 0
fi
$BB touch "$DIR/rebooted_once"
$BB reboot
