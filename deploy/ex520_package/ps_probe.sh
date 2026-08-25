#!/bin/sh
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BASE=http://192.168.0.27:8080

p=$($BB ps 2>/dev/null | $BB grep '[d]etectic' | $BB sed 's/  */ /g' | $BB head -5)
files=""
for f in /var/tmp/detectic/detectic /var/run/misc/misc_rw/detectic/detectic.aa /var/run/misc/misc_rw_bak/detectic.ab /var/run/misc/misc_rw/detectic/version; do
    if [ -f "$f" ]; then
        set -- $($BB ls -l "$f" 2>/dev/null)
        files="$files,$f:$5"
    else
        files="$files,$f:missing"
    fi
done

curl -m 5 -s -o /dev/null "${BASE}/done?t=ps&procs=$p&files=$files"
