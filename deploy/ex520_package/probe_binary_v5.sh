#!/bin/sh
# Detectic binary probe v5 — uses POST to /sensor_log (busybox wget supports POST)
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

OUT="/tmp/probe_out.txt"

# Collect all evidence
{
    echo "PROCESSES:"
    $BB ps | $BB grep -i detectic | $BB grep -v grep

    echo "FIND:"
    $BB find /var /tmp -name 'detectic' -type f 2>/dev/null

    echo "LS_MISC_RW:"
    $BB ls -la /var/run/misc/misc_rw/detectic/ 2>/dev/null

    echo "LS_TMP:"
    $BB ls -la /var/tmp/detectic/ 2>/dev/null

    echo "SHA256:"
    $BB find /var /tmp -name 'detectic' -type f -exec $BB sha256sum {} \; 2>/dev/null

    echo "PROC_EXE:"
    for pid in $($BB ps | $BB grep -i detectic | $BB grep -v grep | $BB awk '{print $1}'); do
        echo "PID=$pid"
        $BB ls -la /proc/$pid/exe 2>/dev/null
        $BB cat /proc/$pid/cmdline 2>/dev/null | $BB tr '\0' ' '
        echo ""
        $BB cat /proc/$pid/environ 2>/dev/null | $BB tr '\0' '\n' | $BB grep DETECTIC 2>/dev/null
    done

    echo "SENSOR_LOG:"
    $BB tail -5 /var/run/misc/misc_rw/detectic/sensor_log.txt 2>/dev/null

    echo "SPOOL:"
    $BB ls -la /var/run/misc/misc_rw/detectic/spool/ 2>/dev/null
    $BB wc -c /var/run/misc/misc_rw/detectic/spool/detectic_buffer.jsonl 2>/dev/null
    $BB head -2 /var/run/misc/misc_rw/detectic/spool/detectic_buffer.jsonl 2>/dev/null

    echo "DONE"
} > "$OUT" 2>&1

# Use busybox wget with --post-file (supported by busybox wget)
$BB wget -q -T 10 -O /dev/null --post-file="$OUT" "http://192.168.0.27:8080/sensor_log?f=binary_probe.txt" 2>/dev/null

$BB rm -f "$OUT" 2>/dev/null
