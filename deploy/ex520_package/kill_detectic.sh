#!/bin/sh
BB=/bin/busybox
for _proc in /proc/[0-9]*/cmdline; do
    [ -f "$_proc" ] || continue
    # Kill the detectic binary or its launcher only, never bootstart or phoenix.
    if $BB grep -qaE '/var/tmp/detectic/detectic|launcher\.sh' "$_proc" 2>/dev/null; then
        _pid="$($BB echo "$_proc" | $BB sed 's|/proc/||;s|/cmdline||')"
        $BB kill -9 "$_pid" 2>/dev/null || true
    fi
done
