#!/bin/sh
# run_probe.sh — phoenix entrypoint that stages the sensor (bootstart.sh) then
# runs the lifecycle forensic probe in watch-only mode.
#
# bootstart.sh downloads + reassembles the binary and starts the sensor via
# launcher.sh. probe_lifecycle.sh then watches that running sensor and records
# the process tree + signal evidence until it dies.
BB=/bin/busybox
BASE="${DETECTIC_PACKAGE_URL:-http://192.168.0.27:8080}"

report() {
    _enc="$(echo "$1" | $BB tr ' ' '_' | $BB head -c 300)"
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?d=${_enc}" 2>/dev/null || true
}

report "RUN_PROBE start"

# Stage + start the sensor via the normal bootstrap. This ensures the binary is
# present and launcher.sh is running BEFORE the probe attaches.
$BB wget -q -T 20 -O /tmp/bootstart_probe.sh "${BASE}/bootstart.sh" 2>/dev/null || true
if [ -s /tmp/bootstart_probe.sh ]; then
    report "RUN_PROBE executing bootstart.sh"
    $BB sh /tmp/bootstart_probe.sh 2>/tmp/bootstart_probe.trace || true
else
    report "RUN_PROBE bootstart download FAILED"
fi

# Now attach the probe to watch the just-started sensor.
report "RUN_PROBE downloading probe"
$BB wget -q -T 20 -O /tmp/probe_lifecycle.sh "${BASE}/probe_lifecycle.sh" 2>/dev/null || true
if [ -s /tmp/probe_lifecycle.sh ]; then
    report "RUN_PROBE attaching probe"
    PROBE_WATCH_ONLY=1 $BB sh /tmp/probe_lifecycle.sh 2>/tmp/probe_lifecycle.trace || true
else
    report "RUN_PROBE probe download FAILED"
fi
report "RUN_PROBE done"
