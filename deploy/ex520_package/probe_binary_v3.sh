#!/bin/sh
# Detectic deployed binary probe v3 — sends full output via PUT to package server
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
OUT="/tmp/detectic_probe_out.txt"

{
    echo "=== PROCESSES ==="
    $BB ps 2>/dev/null | $BB grep -i detectic | $BB grep -v grep

    echo "=== FIND BINARY ==="
    $BB find /var /tmp -name 'detectic' -type f 2>/dev/null

    echo "=== LS LOCATIONS ==="
    $BB ls -la /var/run/misc/misc_rw/detectic/ 2>/dev/null
    echo "---"
    $BB ls -la /var/tmp/detectic/ 2>/dev/null
    echo "---"
    $BB ls -la /var/run/misc/misc_rw_bak/detectic/ 2>/dev/null

    echo "=== SHA256 ==="
    for bin in $($BB find /var /tmp -name 'detectic' -type f 2>/dev/null); do
        echo "BIN=$bin"
        $BB sha256sum "$bin" 2>/dev/null
        $BB ls -la "$bin" 2>/dev/null
    done

    echo "=== PROC EXE ==="
    for pid in $($BB ps 2>/dev/null | $BB grep -i detectic | $BB grep -v grep | $BB awk '{print $1}'); do
        if [ -d "/proc/$pid" ]; then
            echo "PID=$pid"
            $BB ls -la /proc/$pid/exe 2>/dev/null
            $BB cat /proc/$pid/cmdline 2>/dev/null | $BB tr '\0' ' '
            echo ""
        fi
    done

    echo "=== SENSOR LOG ==="
    $BB tail -10 /var/run/misc/misc_rw/detectic/sensor_log.txt 2>/dev/null
    $BB tail -10 /var/run/misc/misc_rw/detectic/detectic.log 2>/dev/null

    echo "=== AUTOSTART LOG ==="
    $BB tail -5 /var/run/misc/misc_rw/detectic/autostart.log 2>/dev/null

    echo "=== ENVIRON ==="
    for pid in $($BB ps 2>/dev/null | $BB grep -i detectic | $BB grep -v grep | $BB awk '{print $1}'); do
        if [ -d "/proc/$pid" ]; then
            echo "PID=$pid ENVIRON:"
            $BB cat /proc/$pid/environ 2>/dev/null | $BB tr '\0' '\n' | $BB grep DETECTIC
        fi
    done
} > "$OUT" 2>&1

# Send the full output via PUT to the package server
$BB wget -q -T 10 -O /dev/null --method=PUT --body-file="$OUT" "http://192.168.0.27:8080/sensor_log?f=binary_probe.txt" 2>/dev/null || true

# Also try a simple callback with just the SHA
SHA=$($BB find /var /tmp -name 'detectic' -type f -exec $BB sha256sum {} \; 2>/dev/null | $BB head -1 | $BB awk '{print $1}')
$BB wget -q -T 5 -O /dev/null "http://192.168.0.27:8080/binary_probe_v3?sha=${SHA}" 2>/dev/null || true

$BB rm -f "$OUT" 2>/dev/null
