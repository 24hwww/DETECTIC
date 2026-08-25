#!/bin/sh
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BASE=http://192.168.0.27:8080

alog="/var/run/misc/misc_rw/detectic/autostart.log"
dlog="/var/run/misc/misc_rw/detectic/detectic.log"

a=$($BB tail -c 1000 "$alog" 2>/dev/null | $BB tr '\n' '+')
d=$($BB tail -c 1000 "$dlog" 2>/dev/null | $BB tr '\n' '+')

curl -m 5 -s -o /dev/null "${BASE}/done?t=log&a=$a&d=$d"
