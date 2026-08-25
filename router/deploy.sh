#!/bin/sh
# Deploy the Detectic sensor to a TP-Link EX520v (OpenWrt-based) and start it.
#
# Usage:
#   ./router/deploy.sh [user@router-ip]
#   ./router/deploy.sh root@192.168.0.1
#
# Requirements on the dev machine:
#   - the router cross-binary built:  make router
#   - ssh + scp access to the router (obtained via UART / shell, see AGENTS.md M1)
#
# This script is reversible: `./etc/init.d/detectic stop && ./etc/init.d/detectic disable`
# (and `rm /usr/bin/detectic`) removes the sensor. No firmware is flashed.
set -e

ROUTER="${1:-root@192.168.0.1}"
HERE="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${HERE}/target/aarch64-unknown-linux-musl/release/detectic"
INIT="${HERE}/router/detectic.initd"
CONF="${HERE}/router/detectic.conf.example"

[ -x "$BIN" ] || { echo "build the router binary first: make router"; exit 1; }

echo "[*] copying binary -> ${ROUTER}:/usr/bin/detectic"
scp "$BIN" "${ROUTER}:/usr/bin/detectic"
ssh "$ROUTER" "chmod +x /usr/bin/detectic"

echo "[*] installing init script -> ${ROUTER}:/etc/init.d/detectic"
scp "$INIT" "${ROUTER}:/etc/init.d/detectic"
ssh "$ROUTER" "chmod +x /etc/init.d/detectic"

echo "[*] installing config (only if /etc/detectic.conf missing)"
ssh "$ROUTER" "test -f /etc/detectic.conf || (cat > /etc/detectic.conf <<'EOF'
$(cat "$CONF")
EOF
)"

echo "[*] enabling + starting service"
ssh "$ROUTER" "/etc/init.d/detectic enable && /etc/init.d/detectic start"

echo "[*] done. Verify with:  ssh ${ROUTER} 'logread | grep detectic'"
echo "[*] revert:            ssh ${ROUTER} '/etc/init.d/detectic stop && /etc/init.d/detectic disable'"
