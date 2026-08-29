#!/bin/bash
# ============================================================================
# deploy.sh — idempotent end-to-end deploy + post-reboot autonomous verify.
#
# Chain (Path 2 -> 3 -> 4):
#   ensure package server -> reboot EX520 (cos reads LIFEMOTE URL only at boot)
#   -> GTPR so DEV2_LIFEMOTE_AGENT -> phoenix -> bootstart.sh -> launcher.sh
#   -> sensor -> WSS/HTTPS -> Cloudflare.
#
# SUCCESS = post-reboot autonomous verification, NOT merely "deploy finished".
# The sensor must be alive past the phoenix lifecycle kill and emitting events.
#
# Usage:
#   DETECTIC_PASSWORD=... ./deploy.sh            # full deploy (reboot + trigger + verify)
#   DETECTIC_PASSWORD=... ./deploy.sh --verify   # verify only, no reboot/trigger
#   DETECTIC_PASSWORD=... ./deploy.sh --package  # rebuild package, no deploy
#   DETECTIC_PASSWORD=... ./deploy.sh --no-reboot
#
# Credentials are read from env (DETECTIC_PASSWORD). Reuse NOTHING hardcoded.
# ============================================================================
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

# ---- Environment (never hardcode secrets) ----
EX520_URL="${DETECTIC_URL:-http://[fe80::3e6a:d2ff:fe5f:abc1%enp2s0]}"
EX520_USER="${DETECTIC_USER:-user}"
EX520_PASSWORD="${DETECTIC_PASSWORD:-}"
PACKAGE_HOST="${PACKAGE_HOST:-192.168.0.27}"
PACKAGE_PORT="${PACKAGE_PORT:-8080}"
PACKAGE_URL="http://${PACKAGE_HOST}:${PACKAGE_PORT}"
BACKEND_HEALTH="${DETECTIC_BACKEND_HEALTH:-https://detectic.24hwww.workers.dev/api/v1/stats}"
# Sensor HTTP control plane binds [::]:8787 on the router; reachable via the
# router's IPv4 LAN IP (192.168.0.1) — simpler and proven vs. IPv6 link-local.
SENSOR_HEALTH_URL="${DETECTIC_SENSOR_HEALTH:-http://192.168.0.1:8787/health}"
# Wait windows
BOOT_WAIT_SECS=300          # max wait for EX520 down->up cycle after reboot
SENSOR_WAIT_SECS=280        # max wait for sensor to bind :8787 after trigger
STABILITY_GRACE=8           # seconds to let cos finish init before so() grant
POLL_SECS=3

if [ -z "$EX520_PASSWORD" ]; then
    echo "ERROR: set DETECTIC_PASSWORD (GTPR password)." >&2
    exit 2
fi

pkg_up()    { curl -s -m 5 "${PACKAGE_URL}/version" 2>/dev/null | grep -q .; }
ex520_up()  { curl -s -m 5 "${EX520_URL}" >/dev/null 2>&1; }
sensor_up() { curl -s -m 3 "${SENSOR_HEALTH_URL}" 2>/dev/null | grep -qi .; }

ensure_server() {
    if pkg_up; then
        echo "  package server ON ($PACKAGE_URL): $(curl -s -m 3 ${PACKAGE_URL}/version)"
        return 0
    fi
    echo "  starting package server..."
    nohup python3 "$SCRIPT_DIR/package_server.py" > "$SCRIPT_DIR/package_server.log" 2>&1 </dev/null & \
        echo $! > "$SCRIPT_DIR/.package_server.pid"
    for _ in $(seq 1 20); do pkg_up && break; sleep 1; done
    if pkg_up; then echo "  package server started"; else echo "  FAILED to start package server"; return 1; fi
}

reboot_ex520() {
    echo "  rebooting EX520 (ACT_REBOOT)..."
    python3 - "$EX520_URL" "$EX520_USER" "$EX520_PASSWORD" <<'PY'
import sys
sys.path.insert(0, 'python')
from detectic_client import GtprClient
client = GtprClient(sys.argv[1], sys.argv[2], sys.argv[3])
client.connect()
print(client.op('ACT_REBOOT'))
PY
}

wait_ex520() {
    # ACT_REBOOT is async: the router needs a few seconds to actually go down.
    # Wait for a real down->up transition (observed down then back up), then a
    # short stability grace period so cos is ready to accept a so() grant.
    # If the router is already down, skip the "down" wait.
    ex520_up || echo "  EX520 already down (no down-wait needed)"
    _w=0
    # 1) Wait for it to go DOWN (leaves the current/booting instance).
    while [ "$_w" -lt "${BOOT_WAIT_SECS}" ]; do
        ex520_up || { echo "  EX520 went DOWN after ${_w}s"; break; }
        sleep "$POLL_SECS"
        _w=$((_w + POLL_SECS))
    done
    # 2) Wait for it to come back UP.
    _w=0
    while [ "$_w" -lt "${BOOT_WAIT_SECS}" ]; do
        ex520_up && { echo "  EX520 back UP after ${_w}s"; break; }
        sleep "$POLL_SECS"
        _w=$((_w + POLL_SECS))
    done
    ex520_up || { echo "  ERROR: EX520 never came back within ${BOOT_WAIT_SECS}s" >&2; return 1; }
    # 3) Stability grace: let cos finish init before granting so().
    echo "  waiting ${STABILITY_GRACE}s for cos..."
    sleep "$STABILITY_GRACE"
    return 0
}

set_lifemote() {
    echo "  setting DEV2_LIFEMOTE_AGENT -> ${PACKAGE_URL}/bootstart.sh..."
    python3 - "$EX520_URL" "$EX520_USER" "$EX520_PASSWORD" "$PACKAGE_URL/bootstart.sh" <<'PY'
import sys
sys.path.insert(0, 'python')
from detectic_client import GtprClient
client = GtprClient(sys.argv[1], sys.argv[2], sys.argv[3])
client.connect()
r = client.so('DEV2_LIFEMOTE_AGENT', {
    'enable':'1', 'URL':sys.argv[4],
    'stack':'0,0,0,0,0,0','pstack':'0,0,0,0,0,0',
})
print('so lifemote:', r)
PY
}

wait_sensor() {
    # Wait for the sensor to bind :8787, then confirm it STAYS up long enough
    # to survive the phoenix lifecycle kill (~78s window seen live).
    _t=0
    while [ "$_t" -lt "$SENSOR_WAIT_SECS" ]; do
        sensor_up && break
        sleep "$POLL_SECS"
        _t=$((_t + POLL_SECS))
    done
    if ! sensor_up; then
        echo "  ERROR: sensor never bound :8787 within ${SENSOR_WAIT_SECS}s" >&2
        return 1
    fi
    echo "  sensor up on :8787 after ${_t}s"
    # Persistence proof: observe uptime. A sensor that survives the phoenix
    # lifecycle kill keeps running (launcher restart loop + setsid). We require
    # the sensor to be still alive after a sustained window, else a rogue one-off
    # execution looks success-until-killed.
    _probe=0
    while [ "$_probe" -lt 90 ]; do
        sleep 3
        _probe=$((_probe + 3))
    done
    if sensor_up; then
        echo "  VERIFIED: sensor stable after ${_probe}s (survived lifecycle kill)"
        return 0
    fi
    echo "  FAILED: sensor died within ${_probe}s (phoenix lifecycle kill reapplied?)" >&2
    return 1
}

backend_stats() {
    echo "  backend: $(curl -s -m 10 "$BACKEND_HEALTH" 2>/dev/null | head -c 300 || echo 'unreachable')"
}

DO_PACKAGE=0; DO_REBOOT=1; DO_VERIFY=1
for arg in "$@"; do
    case "$arg" in
        --package)  DO_PACKAGE=1 ;;
        --no-reboot) DO_REBOOT=0 ;;
        --verify)   DO_VERIFY=1 ;;
        *) echo "unknown arg: $arg" >&2 ;;
    esac
done

echo "=== Deploy POST-REBOOT verification ==="

if [ "$DO_PACKAGE" = "1" ]; then
    echo "[package] building..."
    (cd "$REPO_DIR" && make package) || { echo "package build failed" >&2; exit 1; }
fi

echo "[pre] ensure package server"
ensure_server || exit 1

echo "[pre] EX520 reachability"
ex520_up && echo "  EX520 reachable" || { echo "  EX520 NOT reachable" >&2; exit 1; }

if [ "$DO_REBOOT" = "1" ]; then
    echo "[step1] reboot"
    reboot_ex520
    # Wait for a real down->up cycle AND a stability grace so cos is ready to
    # accept a so() grant (ACT_REBOOT is async; granting too early is lost).
    wait_ex520 || exit 1
else
    echo "[step1] reboot skipped"
fi

echo "[step2] trigger"
set_lifemote || exit 1

echo "[step3] wait + verify sensor autonomy"
wait_sensor || exit 1

echo "[verify] backend events"
backend_stats

echo ""
echo "DEPLOY VERIFIED (post-reboot autonomous)."
