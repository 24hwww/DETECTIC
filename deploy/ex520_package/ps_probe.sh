#!/bin/sh
CB="http://192.168.0.27:8080"
BB=/bin/busybox
TMP=/tmp/probe_ps.txt
$BB ps -w > $TMP 2>&1
$BB wget -q -T 10 -O /dev/null --post-file=$TMP "${CB}/probe_log?tag=ps" 2>/dev/null || true
