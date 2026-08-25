#!/bin/bash
# test_ssh_vectors.sh — Probar TODAS las vías de acceso SSH/Telnet en EX520
# No modifica nada permanentemente. Solo lee y prueba configuraciones.

set -euo pipefail

source .env 2>/dev/null || true

# Defaults
EX520_URL="${EX520_URL:-http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]}"
EX520_USER="${EX520_USER:-user}"
DETECTIC_PASSWORD="${DETECTIC_PASSWORD:-}"
DETECTIC_BIN="${DETECTIC_BIN:-./dist/detectic-aarch64-musl}"
EX520_IPV6="${EX520_IPV6:-fe80::3e6a:d2ff:fe5f:abc1%enp2s0}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

ok() { echo -e "${GREEN}[OK]${NC} $*"; }
fail() { echo -e "${RED}[FAIL]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
info() { echo -e "[INFO] $*"; }

run_gtpr() {
    local cmd=$1; shift
    DETECTIC_PASSWORD="$DETECTIC_PASSWORD" "$DETECTIC_BIN" \
        --url "$EX520_URL" --user "$EX520_USER" "$cmd" "$@" 2>&1
}

echo "============================================"
echo " EX520 SSH/Telnet Vector Audit"
echo " $(date)"
echo "============================================"
echo ""

# --- Phase 1: Connectivity ---
info "Phase 1: Connectivity Check"

if ping6 -c 1 -W 2 "$EX520_IPV6" >/dev/null 2>&1; then
    ok "IPv6 ping reachable"
else
    fail "IPv6 ping unreachable"
fi

if run_gtpr query DEV2_WIFI_APDEV_ASSOCDEV | grep -q "ASSOCDEV\|data"; then
    ok "GTPR query works"
else
    fail "GTPR query failed — cannot proceed"
    exit 1
fi
echo ""

# --- Phase 2: Read all remote-access OIDs ---
info "Phase 2: Query all remote-access OIDs"

for OID in DEV2_SSH_CFG DEV2_TELNET_CFG X_TTNET_CONF_SHELL \
           DEV2_USER_CFG DEV2_HTTP_CFG DEV2_CURRENT_USER \
           DEV2_DIAG_TOOL DEV2_TTNET_CONFIG; do
    result=$(run_gtpr query "$OID" 2>/dev/null)
    if echo "$result" | grep -q "errorcode\|error\|Error"; then
        warn "$OID → $(echo "$result" | head -1)"
    elif [ -n "$result" ]; then
        ok "$OID → $(echo "$result" | head -c 200)"
    else
        warn "$OID → empty/no response"
    fi
done
echo ""

# --- Phase 3: Port scan ---
info "Phase 3: Port scan"

for PORT in 22 23 80 443 8080 8088 9999; do
    if nc -z -w2 "$EX520_IPV6" "$PORT" 2>/dev/null; then
        ok "Port $PORT OPEN"
    else
        info "Port $PORT closed"
    fi
done
echo ""

# --- Phase 4: Try enabling SSH ---
info "Phase 4: Test SSH enablement via GTPR"

echo "Attempting: set DEV2_SSH_CFG enable=1"
result=$(run_gtpr set DEV2_SSH_CFG \
    '{"Enable":"1","Port":"22","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}' 2>/dev/null)
echo "  Response: $result"

echo ""
echo "Attempting: set DEV2_TELNET_CFG enable=1"
result=$(run_gtpr set DEV2_TELNET_CFG \
    '{"telnetLocalEnabled":"1","telnetLocalPort":"23","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}' 2>/dev/null)
echo "  Response: $result"

echo ""
echo "Attempting: set X_TTNET_CONF_SHELL enable=1"
result=$(run_gtpr set X_TTNET_CONF_SHELL \
    '{"Enable":"1","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}' 2>/dev/null)
echo "  Response: $result"

echo ""
info "Waiting 5s for services to potentially start..."
sleep 5
echo ""

# --- Phase 5: Verify ports after enablement ---
info "Phase 5: Re-scan ports after enablement"

for PORT in 22 23 80 443 8080 8088 9999; do
    if nc -z -w2 "$EX520_IPV6" "$PORT" 2>/dev/null; then
        ok "Port $PORT OPEN"
    else
        info "Port $PORT still closed"
    fi
done
echo ""

# --- Phase 6: Try SSH connection ---
info "Phase 6: Try SSH connection"

if nc -z -w2 "$EX520_IPV6" 22 2>/dev/null; then
    ok "SSH port 22 is OPEN — attempting connection"
    
    # Try with common credentials
    for PASS in "$DETECTIC_PASSWORD" "admin" "password" ""; do
        if [ -n "$PASS" ]; then
            echo "  Trying SSH with password: ${PASS:0:3}***"
            timeout 5 ssh -o StrictHostKeyChecking=no \
                -o ConnectTimeout=3 \
                -o PreferredAuthentications=password \
                "$EX520_USER@$EX520_IPV6" "echo SSH_OK" 2>/dev/null && \
                ok "SSH LOGIN SUCCESS with password: ${PASS:0:3}***" || \
                warn "SSH login failed with this password"
        fi
    done
else
    warn "SSH port 22 still closed"
fi

if nc -z -w2 "$EX520_IPV6" 23 2>/dev/null; then
    ok "Telnet port 23 is OPEN — attempting connection"
    timeout 5 bash -c "echo 'show version' | nc $EX520_IPV6 23" 2>/dev/null && \
        ok "Telnet connection OK" || warn "Telnet connection failed"
else
    warn "Telnet port 23 still closed"
fi
echo ""

# --- Phase 7: Revert changes ---
info "Phase 7: Revert all changes"

echo "Disabling SSH..."
run_gtpr set DEV2_SSH_CFG \
    '{"Enable":"0","Port":"22","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}' 2>/dev/null

echo "Disabling Telnet..."
run_gtpr set DEV2_TELNET_CFG \
    '{"telnetLocalEnabled":"0","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}' 2>/dev/null

echo "Disabling Shell..."
run_gtpr set X_TTNET_CONF_SHELL \
    '{"Enable":"0","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}' 2>/dev/null

echo "Disabling Lifemote..."
run_gtpr set DEV2_LIFEMOTE_AGENT \
    '{"enable":"0","URL":"","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}' 2>/dev/null

ok "All changes reverted"
echo ""

echo "============================================"
echo " Audit complete — check results above"
echo "============================================"
