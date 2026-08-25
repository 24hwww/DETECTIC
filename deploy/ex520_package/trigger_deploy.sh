#!/bin/bash
# ============================================================================
# Trigger EX520 deployment via GTPR
# Sets DEV2_LIFEMOTE_AGENT to download and execute bootstart.sh
# ============================================================================
set -euo pipefail

EX520_URL="http://[fe80::3e6a:d2ff:fe5f:abc1%enp2s0]"
USER="user"
PASSWORD="Vida@2013"
PACKAGE_URL="http://192.168.0.27:8080"

echo "=== Triggering EX520 deployment ==="
echo "EX520: $EX520_URL"
echo "Package: $PACKAGE_URL"
echo ""

# Step 1: Verify EX520 is reachable
echo "1. Checking EX520 reachability..."
if ! curl -s -m 5 "$EX520_URL" >/dev/null 2>&1; then
    echo "   ERROR: EX520 not reachable"
    exit 1
fi
echo "   OK"

# Step 2: Verify package server is running
echo "2. Checking package server..."
VERSION=$(curl -s -m 5 "$PACKAGE_URL/version" 2>/dev/null || echo "error")
if [ "$VERSION" = "error" ]; then
    echo "   ERROR: Package server not reachable"
    exit 1
fi
echo "   OK (version: $VERSION)"

# Step 3: Trigger phoenix via GTPR
echo "3. Triggering phoenix.sh via GTPR..."
echo "   Setting DEV2_LIFEMOTE_AGENT..."

# Use the Python GTPR client to perform the set operation
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$(dirname "$SCRIPT_DIR")")"
cd "$REPO_DIR"

python3 -c "
import sys
sys.path.insert(0, 'python')
from detectic_client import GtprClient

client = GtprClient('$EX520_URL', '$USER', '$PASSWORD')
client.connect()

# Set LIFEMOTE agent to trigger bootstart.sh
result = client.so('DEV2_LIFEMOTE_AGENT', {
    'enable': '1',
    'URL': '$PACKAGE_URL/bootstart.sh',
    'stack': '0,0,0,0,0,0',
    'pstack': '0,0,0,0,0,0'
})
print(f'   Result: {result}')
" 2>&1

echo ""
echo "=== Deployment triggered ==="
echo "The EX520 should now:"
echo "  1. Download bootstart.sh from package server"
echo "  2. Download detectic.aa + detectic.ab (TLS binary)"
echo "  3. Reassemble and run the new binary"
echo "  4. Upload data to https://detectic.24hwww.workers.dev"
echo ""
echo "Monitor progress:"
echo "  tail -f deploy/ex520_package/sensor_log.txt"
echo "  curl -s https://detectic.24hwww.workers.dev/api/v1/stats"
