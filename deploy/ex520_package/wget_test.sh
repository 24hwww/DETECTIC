#!/bin/sh
CB="http://192.168.0.27:8080"
BB=/bin/busybox
$BB wget --help > /tmp/wget_help.txt 2>&1
$BB wget -q -T 10 -O /dev/null --post-file=/tmp/wget_help.txt "${CB}/probe_log?tag=wget_help" 2>&1 | $BB head -c 200 > /tmp/wget_err.txt
$BB wget -q -T 10 -O /dev/null --post-file=/tmp/wget_err.txt "${CB}/probe_log?tag=wget_err" 2>/dev/null || true
$BB echo "ok" > /tmp/test_data.txt
$BB wget -q -T 10 -O /dev/null --post-file=/tmp/test_data.txt "${CB}/probe_log?tag=wget_post_test" 2>/dev/null || true
