#!/bin/sh
CB="http://192.168.0.27:8080"
BB=/bin/busybox
TMP=/tmp/probe_ps.txt
$BB ps > $TMP 2>&1
/usr/sbin/curl -m 10 -X POST --data-binary @$TMP "${CB}/probe_log?tag=ps_curl" 2>/dev/null || true
