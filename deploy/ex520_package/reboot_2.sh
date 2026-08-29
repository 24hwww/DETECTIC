#!/bin/sh
BB=/bin/busybox
DIR="/var/run/misc/misc_rw/detectic"
$BB mkdir -p "$DIR" 2>/dev/null || true
if [ -f "$DIR/rebooted_2" ]; then
    exit 0
fi
$BB touch "$DIR/rebooted_2"
$BB reboot
