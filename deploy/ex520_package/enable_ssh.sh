#!/bin/bash
# enable_ssh.sh — Intento directo de habilitar SSH en el EX520
# Usa GTPR para habilitar dropbear, luego intenta conexión SSH.
#
# SEGURIDAD: Solo ejecutar con autorización explícita.
# Revertir después de la prueba.

set -euo pipefail

source .env 2>/dev/null || true

EX520_URL="${EX520_URL:-http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]}"
EX520_USER="${EX520_USER:-user}"
DETECTIC_PASSWORD="${DETECTIC_PASSWORD:-}"
EX520_IPV6="${EX520_IPV6:-fe80::3e6a:d2ff:fe5f:abc1%enp2s0}"
DETECTIC_BIN="${DETECTIC_BIN:-./dist/detectic-aarch64-musl}"

gtpr() {
    DETECTIC_PASSWORD="$DETECTIC_PASSWORD" "$DETECTIC_BIN" \
        --url "$EX520_URL" --user "$EX520_USER" "$@"
}

echo "=== Step 1: Query current SSH config ==="
gtpr query DEV2_SSH_CFG
echo ""

echo "=== Step 2: Try to enable SSH (dropbear) ==="
# Method A: Direct DEV2_SSH_CFG set
gtpr set DEV2_SSH_CFG \
    '{"Enable":"1","Port":"22","Access":"1","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'
echo ""

echo "=== Step 3: Wait for dropbear to start ==="
sleep 5

echo "=== Step 4: Check SSH port ==="
if nc -z -w3 "$EX520_IPV6" 22 2>/dev/null; then
    echo "SSH PORT 22 IS OPEN!"
    
    echo "=== Step 5: Try SSH login ==="
    ssh -o StrictHostKeyChecking=no \
        -o ConnectTimeout=5 \
        -o UserKnownHostsFile=/dev/null \
        -o PasswordAuthentication=yes \
        "$EX520_USER@$EX520_IPV6" "echo SSH_SUCCESS; uname -a; whoami" 2>&1
else
    echo "SSH port 22 still closed"
    echo ""
    echo "=== Alternative: Try telnet enable ==="
    gtpr set DEV2_TELNET_CFG \
        '{"telnetLocalEnabled":"1","telnetLocalPort":"23","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'
    sleep 3
    
    if nc -z -w3 "$EX520_IPV6" 23 2>/dev/null; then
        echo "TELNET PORT 23 IS OPEN!"
        timeout 5 bash -c "echo 'uname -a' | nc $EX520_IPV6 23" 2>&1
    else
        echo "Telnet port 23 also closed"
    fi
fi

echo ""
echo "=== Step 6: Revert changes ==="
gtpr set DEV2_SSH_CFG \
    '{"Enable":"0","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}' 2>/dev/null
gtpr set DEV2_TELNET_CFG \
    '{"telnetLocalEnabled":"0","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}' 2>/dev/null
echo "Changes reverted."
