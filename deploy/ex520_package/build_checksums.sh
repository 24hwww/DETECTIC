#!/bin/bash
# build_checksums.sh — Generate SHA-256 checksums for the EX520 package.
#
# The package server exposes these files so bootstart.sh can verify every
# download before executing anything.
#
# Files produced:
#   detectic.sha256      — SHA-256 of the full reassembled binary
#   detectic.aa.sha256   — SHA-256 of binary part 1
#   detectic.ab.sha256   — SHA-256 of binary part 2
#   manifest.json        — version + checksums (for humans/supervisor)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PKG_DIR="$SCRIPT_DIR"

cd "$PKG_DIR"

if [ ! -f "detectic.aa" ] || [ ! -f "detectic.ab" ]; then
    echo "ERROR: detectic.aa and/or detectic.ab not found in $PKG_DIR"
    echo "  Build the router binary first: make router"
    echo "  Then split it: split -b <size> detectic detectic."
    exit 1
fi

echo "[build_checksums] Generating SHA-256 checksums..."

# Reassemble the full binary to compute its checksum.
TMP_BIN=".detectic.full.tmp"
rm -f "$TMP_BIN"
cat detectic.aa detectic.ab > "$TMP_BIN"

# Generate checksum files.  Using `sha256sum -b` and stripping the filename
# keeps the output deterministic and easy to verify on the router with
# busybox `sha256sum`.
sha256sum -b detectic.aa   | awk '{print $1}' > detectic.aa.sha256
sha256sum -b detectic.ab   | awk '{print $1}' > detectic.ab.sha256
sha256sum -b "$TMP_BIN"    | awk '{print $1}' > detectic.sha256

# Build a manifest with version + checksums.
VERSION="$(cat version 2>/dev/null || echo 'unknown')"
cat > manifest.json <<EOF
{
  "version": "$VERSION",
  "files": {
    "detectic.aa": "$(cat detectic.aa.sha256)",
    "detectic.ab": "$(cat detectic.ab.sha256)",
    "detectic":    "$(cat detectic.sha256)"
  },
  "generated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF

rm -f "$TMP_BIN"

echo "[build_checksums] Done:"
echo "  detectic.aa.sha256: $(cat detectic.aa.sha256)"
echo "  detectic.ab.sha256: $(cat detectic.ab.sha256)"
echo "  detectic.sha256:    $(cat detectic.sha256)"
echo "  manifest.json:      created"
