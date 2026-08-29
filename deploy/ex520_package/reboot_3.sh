#!/bin/sh
BB=/bin/busybox
DIR="/var/run/misc/misc_rw/detectic"
$BB mkdir -p "$DIR" 2>/dev/null || true
if [ -f "$DIR/rebooted_3" ]; then
    exit 0
fi
$BB touch "$DIR/rebooted_3"
$BB reboot
