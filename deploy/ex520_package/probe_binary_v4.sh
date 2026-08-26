#!/bin/sh
# Detectic binary probe v4 — simple, uses curl if available, falls back to base64 GET
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

# Try curl first (EX520 may have it)
if [ -x /usr/bin/curl ] || [ -x /bin/curl ]; then
    curl -s -X PUT -T "$OUT" "http://192.168.0.27:8080/sensor_log?f=binary_probe.txt" 2>/dev/null
fi

# Fallback: encode and send via GET (base64)
if [ ! -x /usr/bin/curl ] && [ ! -x /bin/curl ]; then
    # Use busybox base64 if available
    B64=$($BB base64 "$OUT" 2>/dev/null | $BB tr -d '\n' | $BB head -c 4000)
    $BB wget -q -T 10 -O /dev/null "http://192.168.0.27:8080/binary_probe_v4?data=${B64}" 2>/dev/null
fi

$BB rm -f "$OUT" 2>/dev/null
