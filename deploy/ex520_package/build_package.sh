#!/bin/bash
# build_package.sh — Build a hardened Detectic deployment package for EX520V.
#
# The package is designed for the canonical Path-3/Path-4 architecture:
#   host package server  <->  EX520 phoenix  <->  bootstart.sh  <->  launcher.sh
#
# Hardening applied by this build:
#   * Split binary is SHA-256 verified before reassembly.
#   * detectic.env is copied only if present or explicitly supplied.
#   * No SSH, no cron, no firmware hooks, no SquashFS modifications.
#
# Usage:
#   ./build_package.sh
#
# The resulting files are placed in _fw_build/package and can be served by
# package_server.py (or any LAN static HTTP server).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="$PROJECT_ROOT/_fw_build/package"
DETECTIC_BIN="$PROJECT_ROOT/dist/detectic-aarch64-musl"

# Detectic runtime env file: prefer a real one if the operator supplied it,
# otherwise fall back to the documented example and warn.
if [ -f "$SCRIPT_DIR/detectic.env" ]; then
    ENV_FILE="$SCRIPT_DIR/detectic.env"
elif [ -f "$SCRIPT_DIR/detectic.env.example" ]; then
    ENV_FILE="$SCRIPT_DIR/detectic.env.example"
    echo "WARNING: using detectic.env.example — copy, edit, and re-run for production"
else
    ENV_FILE=""
fi

PACKAGE_NAME="detectic-ex520-$(date +%Y%m%d_%H%M%S)"

echo "============================================"
echo " EX520 Detectic Package Builder"
echo " $(date)"
echo "============================================"
echo ""

# --- Verify prerequisites ---
if [ ! -f "$DETECTIC_BIN" ]; then
    echo "ERROR: Detectic binary not found: $DETECTIC_BIN"
    echo "  Build with: make router"
    exit 1
fi

mkdir -p "$BUILD_DIR"
rm -rf "$BUILD_DIR"/*

echo "Detectic binary: $DETECTIC_BIN ($(ls -la "$DETECTIC_BIN" | awk '{print $5}') bytes)"

# --- Split binary ---
echo "[1/4] Splitting binary..."
SPLIT_SIZE=$((1024 * 1024))  # 1 MiB per part; produces detectic.aa + detectic.ab
split -b "$SPLIT_SIZE" "$DETECTIC_BIN" "$BUILD_DIR/detectic."
ls -la "$BUILD_DIR"/detectic.*
echo ""

# --- Copy launcher and config ---
echo "[2/4] Copying launcher and config..."
cp "$SCRIPT_DIR/bootstart.sh" "$BUILD_DIR/"
cp "$SCRIPT_DIR/launcher.sh" "$BUILD_DIR/"
if [ -n "$ENV_FILE" ]; then
    cp "$ENV_FILE" "$BUILD_DIR/detectic.env"
fi
if [ -f "$PROJECT_ROOT/VERSION" ]; then
    cp "$PROJECT_ROOT/VERSION" "$BUILD_DIR/version"
else
    echo "dev-$(date +%Y%m%d)" > "$BUILD_DIR/version"
fi
echo ""

# --- Generate SHA-256 checksums ---
echo "[3/4] Generating SHA-256 checksums..."
cd "$BUILD_DIR"
sha256sum -b detectic.aa   | awk '{print $1}' > detectic.aa.sha256
sha256sum -b detectic.ab   | awk '{print $1}' > detectic.ab.sha256
# Reassemble to compute the full binary checksum.
cat detectic.aa detectic.ab > .detectic.full.tmp
sha256sum -b .detectic.full.tmp | awk '{print $1}' > detectic.sha256
rm -f .detectic.full.tmp

VERSION="$(cat version)"
cat > manifest.json <<EOF
{
  "version": "$VERSION",
  "files": {
    "detectic.aa": "$(cat detectic.aa.sha256)",
    "detectic.ab": "$(cat detectic.ab.sha256)",
    "detectic": "$(cat detectic.sha256)"
  },
  "generated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF

echo "  detectic.aa: $(cat detectic.aa.sha256)"
echo "  detectic.ab: $(cat detectic.ab.sha256)"
echo "  detectic:    $(cat detectic.sha256)"
echo ""

# --- Package archive ---
echo "[4/4] Creating package archive..."
cd "$PROJECT_ROOT"
tar czf "$PROJECT_ROOT/$PACKAGE_NAME.tar.gz" -C "$BUILD_DIR" .

echo ""
echo "============================================"
echo " Package build complete!"
echo "============================================"
echo ""
echo "  Package: $PROJECT_ROOT/$PACKAGE_NAME.tar.gz"
echo "  Size:    $(ls -la "$PROJECT_ROOT/$PACKAGE_NAME.tar.gz" | awk '{print $5}') bytes"
echo ""
echo "  To deploy:"
echo "    1. Copy package files to your package server directory:"
echo "       cp $BUILD_DIR/* /path/to/package/server/"
echo "    2. Start the package server: python3 package_server.py"
echo "    3. Start the Edge Supervisor: DETECTIC_PASSWORD=... python3 watchdog.py"
echo ""
echo "  The supervisor will send a GTPR so DEV2_LIFEMOTE_AGENT after a cold boot."
echo "============================================"
