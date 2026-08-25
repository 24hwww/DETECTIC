#!/bin/bash
# ============================================================================
# Detectic Cloudflare Worker — Full Deployment
# ============================================================================
# Run this locally (not in CI) to set up D1 + secrets.
# Then push to GitHub for CI auto-deploy.
# ============================================================================
set -euo pipefail

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log()  { echo -e "${GREEN}[deploy]${NC} $*"; }
warn() { echo -e "${YELLOW}[deploy]${NC} $*"; }
err()  { echo -e "${RED}[deploy]${NC} $*" >&2; }

cd "$(dirname "$0")"

# --- Step 1: Check wrangler ---
if ! command -v npx &>/dev/null; then
    err "npx not found. Install Node.js first."
    exit 1
fi

log "Checking wrangler login..."
npx wrangler whoami 2>/dev/null || {
    err "Not logged in. Run: npx wrangler login"
    exit 1
}

# --- Step 2: Create D1 database ---
log "Creating D1 database..."
D1_OUTPUT=$(npx wrangler d1 create detectic-db 2>&1)
echo "$D1_OUTPUT"

# Extract database_id from output
DATABASE_ID=$(echo "$D1_OUTPUT" | grep -oP 'database_id = "\K[^"]+' || true)
if [ -z "$DATABASE_ID" ]; then
    # Fallback: try different format
    DATABASE_ID=$(echo "$D1_OUTPUT" | grep -oP '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)
fi

if [ -z "$DATABASE_ID" ]; then
    err "Could not extract database_id from output."
    err "Please copy the database_id manually into wrangler.toml"
    err "Then re-run this script."
    exit 1
fi

log "Database ID: $DATABASE_ID"

# --- Step 3: Update wrangler.toml ---
log "Updating wrangler.toml..."
sed -i "s/YOUR_D1_DATABASE_ID_HERE/$DATABASE_ID/" wrangler.toml
log "wrangler.toml updated"

# --- Step 4: Initialize schema ---
log "Initializing D1 schema..."
npx wrangler d1 execute detectic-db --file=schema.sql
log "Schema created"

# --- Step 5: Generate secrets ---
log "Generating secrets..."
MASTER_SECRET=$(openssl rand -hex 32 2>/dev/null || head -c 64 /dev/urandom | od -An -tx1 | tr -d ' \n' | head -c 64)
SENSOR_SECRET=$(openssl rand -hex 16 2>/dev/null || head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' | head -c 32)

# --- Step 6: Set secrets ---
log "Setting DETECTIC_MASTER_SECRET..."
echo "$MASTER_SECRET" | npx wrangler secret put DETECTIC_MASTER_SECRET

log "Setting DETECTIC_SENSORS..."
echo "{\"ex520-001\":\"$SENSOR_SECRET\"}" | npx wrangler secret put DETECTIC_SENSORS

# --- Step 7: Deploy ---
log "Deploying Worker..."
npx wrangler deploy

# --- Step 8: Print summary ---
echo ""
log "============================================"
log "  Deployment Complete!"
log "============================================"
echo ""
log "  D1 Database ID: $DATABASE_ID"
log "  Database Name:  detectic-db"
echo ""
log "  Sensor secret (SAVE THIS):"
log "    DETECTIC_SENSOR_SECRET=$SENSOR_SECRET"
echo ""
log "  Master secret (SAVE THIS):"
log "    DETECTIC_MASTER_SECRET=$MASTER_SECRET"
echo ""
log "  EX520 config (detectic.env):"
log "    DETECTIC_UPLOAD_URL=https://detectic.YOUR_SUBDOMAIN.workers.dev/api/v1/events"
log "    DETECTIC_SECRET=$SENSOR_SECRET"
echo ""
log "  Dashboard:"
log "    https://detectic.YOUR_SUBDOMAIN.workers.dev/"
echo ""
log "  Push to GitHub to enable CI auto-deploy:"
log "    git add backend/cf-worker/"
log "    git commit -m 'feat: detectic cloudflare worker backend'"
log "    git push"
