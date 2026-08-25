#!/bin/bash
# Prepare Detectic deployment package for EX520
# Run on the development machine
# Creates a tarball ready for transfer to the router

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BIN="${PROJECT_ROOT}/target/aarch64-unknown-linux-musl/release/detectic"
LAUNCHER="${SCRIPT_DIR}/launcher.sh"
RECON="${SCRIPT_DIR}/recon.sh"
PACKAGE_DIR="${SCRIPT_DIR}/package"
PACKAGE_TAR="${SCRIPT_DIR}/detectic-ex520.tar.gz"

echo "=== Detectic EX520 Deployment Package ==="
echo ""

# Verify binary exists
if [ ! -f "$BIN" ]; then
    echo "ERROR: Binary not found at $BIN"
    echo "Build with: cargo build --release --target aarch64-unknown-linux-musl --no-default-features"
    exit 1
fi

# Verify binary properties
echo "Binary: $BIN"
file "$BIN"
ls -la "$BIN"
SHA=$(sha256sum "$BIN" | awk '{print $1}')
echo "SHA-256: $SHA"
echo ""

# Create package directory
rm -rf "$PACKAGE_DIR"
mkdir -p "$PACKAGE_DIR/detectic"

# Copy binary
cp "$BIN" "$PACKAGE_DIR/detectic/detectic"
chmod +x "$PACKAGE_DIR/detectic/detectic"

# Copy launcher
cp "$LAUNCHER" "$PACKAGE_DIR/detectic/launcher.sh"
chmod +x "$PACKAGE_DIR/detectic/launcher.sh"

# Copy recon script
cp "$RECON" "$PACKAGE_DIR/detectic/recon.sh"
chmod +x "$PACKAGE_DIR/detectic/recon.sh"

# Create manifest
cat > "$PACKAGE_DIR/detectic/manifest.txt" << EOF
Detectic EX520 Deployment Package
=================================
Date: $(date -u '+%Y-%m-%d %H:%M:%S UTC')
Binary: detectic
Architecture: aarch64 (ARM64)
Size: $(stat -c%s "$BIN") bytes
SHA-256: $SHA
Features: --no-default-features (no persist/TLS)
Target: /var/run/misc/misc_rw/detectic/

Contents:
  detectic      - main binary (ARM64 static)
  launcher.sh   - POSIX shell launcher
  recon.sh      - live reconnaissance script
  manifest.txt  - this file

Installation:
  1. Transfer tarball to router
  2. Extract to /var/run/misc/misc_rw/detectic/
  3. chmod +x detectic launcher.sh
  4. Run: ./launcher.sh start
  5. Run: ./launcher.sh status

Removal:
  ./launcher.sh stop
  rm -rf /var/run/misc/misc_rw/detectic/
EOF

# Create tarball
cd "$PACKAGE_DIR"
tar czf "$PACKAGE_TAR" detectic/
cd "$SCRIPT_DIR"

# Report
echo "=== Package Created ==="
ls -la "$PACKAGE_TAR"
echo "SHA-256: $(sha256sum "$PACKAGE_TAR" | awk '{print $1}')"
echo ""
echo "Package contents:"
tar tzf "$PACKAGE_TAR"
echo ""
echo "=== Transfer methods ==="
echo ""
echo "Option A - If SCP available:"
echo "  scp $PACKAGE_TAR root@<EX520_IP>:/tmp/"
echo ""
echo "Option B - If only Telnet available (base64):"
echo "  base64 $PACKAGE_TAR | nc <EX520_IP> 23"
echo "  (or split into chunks for large files)"
echo ""
echo "Option C - If web UI backup/restore available:"
echo "  (manual upload through web interface)"
echo ""
echo "Option D - HTTP server on dev machine:"
echo "  python3 -m http.server 8080 --directory $PACKAGE_DIR"
echo "  Then on router: wget http://<DEV_IP>:8080/detectic/detectic"
echo ""
