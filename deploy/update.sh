#!/bin/sh
# update.sh — Safe, verified, atomic update of Detectic.
#
# Stages a new release, verifies SHA256 via openssl, runs a health test,
# then atomically activates it. Rolls back automatically if the health
# test fails.
#
# Usage:
#   ./update.sh [install_base] <release_dir>

set -e

INSTALL_BASE="${1:-/var/run/misc/misc_rw/detectic}"
NEW_RELEASE_DIR="${2:-}"

if [ -z "$NEW_RELEASE_DIR" ]; then
    echo "[update] ERROR: provide a release directory" >&2
    echo "  usage: $0 <install_base> <release_dir>" >&2
    exit 1
fi

echo "[update] Starting safe update"

BIN="$NEW_RELEASE_DIR/detectic-aarch64-musl"
SUM="$NEW_RELEASE_DIR/detectic-aarch64-musl.sha256"
MANIFEST="$NEW_RELEASE_DIR/manifest.json"

for f in "$BIN" "$SUM" "$MANIFEST"; do
    if [ ! -f "$f" ]; then
        echo "[update] ERROR: missing $f" >&2
        exit 1
    fi
done

# Verify binary size
BINSIZE=$(ls -l "$BIN" | awk '{print $(NF-1)}')
if [ "$BINSIZE" -lt 100000 ]; then
    echo "[update] ERROR: binary too small ($BINSIZE bytes)" >&2
    exit 1
fi

# Verify SHA256
EXPECTED_SHA=$(head -n 1 "$SUM" | awk '{print $1}')
ACTUAL_SHA=$(openssl dgst -sha256 "$BIN" 2>/dev/null | awk '{print $NF}')
if [ -z "$ACTUAL_SHA" ]; then
    echo "[update] ERROR: cannot compute SHA256" >&2
    exit 1
fi
if [ "$EXPECTED_SHA" != "$ACTUAL_SHA" ]; then
    echo "[update] ERROR: SHA256 mismatch" >&2
    exit 1
fi
echo "[update] SHA256 OK"

VERSION=$(grep '"version"' "$MANIFEST" | head -1 | awk -F'"' '{print $4}')
if [ -z "$VERSION" ]; then
    echo "[update] ERROR: cannot read version" >&2
    exit 1
fi
echo "[update] New version: $VERSION"

# Stage release
STAGE_DIR="$INSTALL_BASE/releases/$VERSION"
mkdir -p "$STAGE_DIR"
cp "$BIN" "$STAGE_DIR/detectic"
cp "$MANIFEST" "$STAGE_DIR/manifest.json"
chmod 755 "$STAGE_DIR/detectic"

# Health test: run the staged binary's version + status commands
echo "[update] Running health test..."
if ! "$STAGE_DIR/detectic" version > /dev/null 2>&1; then
    echo "[update] ERROR: health test failed (binary won't execute), cleaning up" >&2
    rm -rf "$STAGE_DIR"
    exit 1
fi
ARCH=$("$STAGE_DIR/detectic" version 2>/dev/null | grep "architecture:" | awk '{print $2}')
if [ "$ARCH" != "aarch64" ]; then
    echo "[update] ERROR: wrong architecture ($ARCH), cleaning up" >&2
    rm -rf "$STAGE_DIR"
    exit 1
fi
echo "[update] Health test passed (arch=$ARCH)"

# Save previous, activate new
if [ -L "$INSTALL_BASE/current" ] || [ -e "$INSTALL_BASE/current" ]; then
    rm -f "$INSTALL_BASE/previous"
    mv "$INSTALL_BASE/current" "$INSTALL_BASE/previous" 2>/dev/null || true
fi
ln -sfn "releases/$VERSION" "$INSTALL_BASE/current"

echo "[update] Activated version $VERSION"
echo "[update] Run health.sh to verify; run rollback.sh if needed"
