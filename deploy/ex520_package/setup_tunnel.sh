#!/bin/bash
# ============================================================================
# Detectic — Cloudflare Tunnel Setup
# ============================================================================
# Sets up cloudflared tunnel for secure EX520 → Cloudflare Worker communication.
#
# Security model:
#   EX520 → HTTP → relay (192.168.0.27:8082) → cloudflared → Cloudflare Worker
#   Only cloudflared has internet access.
#
# Prerequisites:
#   - cloudflared installed (https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/)
#   - Cloudflare account with DNS management
#   - Domain configured in Cloudflare (24hwww.com)
#
# Usage:
#   chmod +x setup_tunnel.sh
#   ./setup_tunnel.sh
# ============================================================================
set -euo pipefail

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log()  { echo -e "${GREEN}[tunnel]${NC} $*"; }
warn() { echo -e "${YELLOW}[tunnel]${NC} $*"; }
err()  { echo -e "${RED}[tunnel]${NC} $*" >&2; }

# --- Step 1: Check cloudflared ---
if ! command -v cloudflared &>/dev/null; then
    err "cloudflared not installed."
    echo ""
    echo "Install it:"
    echo "  # Debian/Ubuntu"
    echo "  curl -fsSL https://pkg.cloudflare.com/cloudflare-main.gpg | sudo tee /usr/share/keyrings/cloudflare-main.gpg >/dev/null"
    echo "  echo 'deb [signed-by=/usr/share/keyrings/cloudflare-main.gpg] https://pkg.cloudflare.com/cloudflared any main' | sudo tee /etc/apt/sources.list.d/cloudflared.list"
    echo "  sudo apt update && sudo apt install -y cloudflared"
    echo ""
    echo "  # Or download binary:"
    echo "  curl -fsSL https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64 -o /usr/local/bin/cloudflared"
    echo "  chmod +x /usr/local/bin/cloudflared"
    exit 1
fi

log "cloudflared found: $(cloudflared --version 2>&1 | head -1)"

# --- Step 2: Check authentication ---
if [ ! -f ~/.cloudflared/cert.pem ]; then
    log "Not authenticated. Opening browser for login..."
    cloudflared tunnel login
fi

if [ ! -f ~/.cloudflared/cert.pem ]; then
    err "Authentication failed. Run: cloudflared tunnel login"
    exit 1
fi

log "Authenticated ✓"

# --- Step 3: Create or find tunnel ---
TUNNEL_NAME="detectic"
TUNNEL_ID=$(cloudflared tunnel list 2>/dev/null | grep "$TUNNEL_NAME" | awk '{print $1}' || true)

if [ -z "$TUNNEL_ID" ]; then
    log "Creating tunnel '$TUNNEL_NAME'..."
    cloudflared tunnel create "$TUNNEL_NAME"
    TUNNEL_ID=$(cloudflared tunnel list | grep "$TUNNEL_NAME" | awk '{print $1}')
    log "Tunnel created: $TUNNEL_ID"
else
    log "Tunnel found: $TUNNEL_ID"
fi

# --- Step 4: Generate config ---
CONFIG_DIR="$HOME/.cloudflared"
CONFIG_FILE="$CONFIG_DIR/config.yml"
CRED_FILE="$CONFIG_DIR/${TUNNEL_ID}.json"

log "Generating config..."
cat > "$CONFIG_FILE" <<EOF
tunnel: $TUNNEL_ID
credentials-file: $CRED_FILE

ingress:
  - hostname: detectic.24hwww.com
    service: http://localhost:8082
    originRequest:
      keepAliveConnections: 10
      keepAliveTimeout: 90s
      connectTimeout: 10s
  - service: http_status:404
EOF

log "Config written to $CONFIG_FILE"

# --- Step 5: Create DNS record ---
log "Creating DNS record..."
cloudflared tunnel route dns "$TUNNEL_NAME" detectic.24hwww.com 2>/dev/null || \
    warn "DNS record may already exist (this is OK)"

# --- Step 6: Run tunnel ---
log "Starting tunnel..."
log ""
log "  Tunnel:    $TUNNEL_NAME ($TUNNEL_ID)"
log "  DNS:       detectic.24hwww.com"
log "  Local:     http://localhost:8082"
log "  Worker:    https://detectic.24hwww.workers.dev"
log ""
log "To start the tunnel:"
log "  cloudflared tunnel run $TUNNEL_NAME"
log ""
log "To run as a service:"
log "  sudo cloudflared service install"
log "  sudo systemctl start cloudflared"
log ""
log "To start the relay:"
log "  python3 relay.py --port 8082"
log ""
log "EX520 config (detectic.env):"
log "  DETECTIC_UPLOAD_URL=http://192.168.0.27:8082/api/v1/events"
log "  DETECTIC_SECRET=<your-sensor-secret>"
