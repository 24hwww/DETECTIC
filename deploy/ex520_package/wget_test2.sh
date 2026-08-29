#!/bin/sh
CB="http://192.168.0.27:8080"
BB=/bin/busybox
# Test which wget post options exist
$BB echo "wget post data test" | $BB wget -q -T 10 -O /dev/null --post-data="test data from busybox wget" "${CB}/probe_log?tag=post_data" 2>/dev/null || true
$BB echo "post file test" > /tmp/pft.txt
$BB wget -q -T 10 -O /dev/null --post-file=/tmp/pft.txt "${CB}/probe_log?tag=post_file" 2>/dev/null || true
