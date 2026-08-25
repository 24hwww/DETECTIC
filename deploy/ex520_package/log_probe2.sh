#!/bin/sh
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BASE=http://192.168.0.27:8080

send() {
    f=$1
    $BB tail -n 30 "$f" 2>/dev/null | while read line; do
        [ -n "$line" ] || continue
        esc=$(echo "$line" | $BB sed 's/ /+/g')
        curl -m 5 -s -o /dev/null "${BASE}/line?f=$f&l=$esc"
    done
}

send /var/run/misc/misc_rw/detectic/autostart.log
send /var/run/misc/misc_rw/detectic/detectic.log
curl -m 5 -s -o /dev/null "${BASE}/done?t=logdone"
