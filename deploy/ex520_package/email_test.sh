#!/bin/sh
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BASE=https://detectic.24hwww.workers.dev
up() { read u _ < /proc/uptime; echo "$u"; }
up_s=$(up)
curl -m 10 -s -o /dev/null "${BASE}/email?type=startup&up=${up_s}&version=v0.1.0-ex520-20260824&pid=$$&status=manual-test" 2>/dev/null
