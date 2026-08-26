#!/bin/sh
# Detectic deployed binary probe v2 — more thorough search
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

# 1. Find ALL detectic processes
echo "=== PROCESSES ==="
$BB ps 2>/dev/null | $BB grep -i detectic | $BB grep -v grep

# 2. Find the binary in all possible locations
echo "=== BINARY SEARCH ==="
for dir in /var/tmp/detectic /var/run/misc/misc_rw/detectic /var/run/misc/misc_rw_bak/detectic /tmp/detectic /var/tmp; do
    if [ -d "$dir" ] || [ -f "$dir" ]; then
        $BB ls -la "$dir" 2>/dev/null | $BB head -10
        echo "---"
    fi
done

# 3. Find any file named 'detectic' on the filesystem
echo "=== FIND ==="
$BB find /var /tmp -name 'detectic' -type f 2>/dev/null

# 4. For each found binary, compute SHA256
echo "=== SHA256 ==="
for bin in $($BB find /var /tmp -name 'detectic' -type f 2>/dev/null); do
    SHA=$($BB sha256sum "$bin" 2>/dev/null | $BB awk '{print $1}')
    SIZE=$($BB ls -la "$bin" 2>/dev/null | $BB awk '{print $5}')
    echo "BIN=$bin SHA256=$SHA SIZE=$SIZE"
done

# 5. Check the running process's exe symlink
echo "=== PROC EXE ==="
for pid in $($BB ps 2>/dev/null | $BB grep -i detectic | $BB grep -v grep | $BB awk '{print $1}'); do
    if [ -d "/proc/$pid" ]; then
        EXE=$($BB ls -la /proc/$pid/exe 2>/dev/null)
        echo "PID=$pid EXE=$EXE"
        echo "PID=$pid CMDLINE=$($BB cat /proc/$pid/cmdline 2>/dev/null | $BB tr '\0' ' ')"
        echo "PID=$pid STARTTIME=$($BB cat /proc/$pid/stat 2>/dev/null | $BB awk '{print $22}')"
    fi
done

# 6. Sensor log
echo "=== SENSOR LOG ==="
$BB tail -10 /var/run/misc/misc_rw/detectic/sensor_log.txt 2>/dev/null
$BB tail -10 /var/run/misc/misc_rw/detectic/detectic.log 2>/dev/null

# 7. Autostart log
echo "=== AUTOSTART LOG ==="
$BB tail -5 /var/run/misc/misc_rw/detectic/autostart.log 2>/dev/null

# 8. Report back
CALLBACK="http://192.168.0.27:8080"
ALL=$($BB find /var /tmp -name 'detectic' -type f 2>/dev/null | $BB head -3 | $BB tr '\n' ',')
SHA1=$($BB sha256sum /var/run/misc/misc_rw/detectic/detectic 2>/dev/null | $BB awk '{print $1}')
SHA2=$($BB sha256sum /var/tmp/detectic/detectic 2>/dev/null | $BB awk '{print $1}')
$BB wget -q -T 5 -O /dev/null "${CALLBACK}/binary_probe_v2?sha1=${SHA1}&sha2=${SHA2}&bins=${ALL}" 2>/dev/null || true
