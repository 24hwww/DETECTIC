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
# The package server serves this exact directory, so the package is built
# straight into the served root (no separate _fw_build staging that would
# drift out of sync with what the EX520 actually downloads).
BUILD_DIR="${PACKAGE_ROOT:-$SCRIPT_DIR}"
# Binary source: prefer the freshly cross-compiled binary under dist/, then
# fall back to a copy already present in the package dir.
DETECTIC_BIN="$PROJECT_ROOT/dist/detectic-aarch64-musl"
[ -f "$DETECTIC_BIN" ] || DETECTIC_BIN="$SCRIPT_DIR/detectic-aarch64-musl"

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
    echo "  Build with: make router  (cross-compiles to target/aarch64-unknown-linux-musl)"
    echo "  Then:       cp target/aarch64-unknown-linux-musl/release/detectic dist/detectic-aarch64-musl"
    exit 1
fi

mkdir -p "$BUILD_DIR"
# Clean ONLY regenerable split/checksum artifacts — never delete the source
# scripts (bootstart.sh, launcher.sh) or detectic.env that live in this dir.
rm -f "$BUILD_DIR"/detectic.?? "$BUILD_DIR"/detectic.??.sha256 \
      "$BUILD_DIR"/detectic.sha256 "$BUILD_DIR"/manifest.json "$BUILD_DIR"/version

echo "Detectic binary: $DETECTIC_BIN ($(ls -la "$DETECTIC_BIN" | awk '{print $5}') bytes)"
echo "Package root:    $BUILD_DIR (served by package_server.py)"
echo ""

# --- Split binary ---
echo "[1/4] Splitting binary..."
SPLIT_SIZE=$((1024 * 1024))  # 1 MiB per part; produces detectic.aa, .ab, .ac, ...
split -b "$SPLIT_SIZE" "$DETECTIC_BIN" "$BUILD_DIR/detectic."
ls -la "$BUILD_DIR"/detectic.*
echo ""

# --- Copy launcher and config (idempotent; skip when src==dest) ---
echo "[2/4] Copying launcher and config..."
for f in bootstart.sh launcher.sh detectic_watchdog.sh; do
    [ "$SCRIPT_DIR/$f" != "$BUILD_DIR/$f" ] && cp "$SCRIPT_DIR/$f" "$BUILD_DIR/"
done
if [ -n "$ENV_FILE" ] && [ "$ENV_FILE" != "$BUILD_DIR/detectic.env" ]; then
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

# Compute per-part checksums for every split data part (detectic.aa, .ab, .ac…).
# Only two-letter lowercase suffixes are parts; detectic.env / .example / other
# files in this dir must never be treated as parts.
for part in detectic.[a-z][a-z]; do
    [ -f "$part" ] || continue
    sha256sum -b "$part" | awk '{print $1}' > "$part.sha256"
done

# Reassemble all split parts (detectic.aa, .ab, .ac, ...) in sorted order to
# compute the full binary checksum.  detectic.env is configuration, not a part.
SORTED_PARTS=($(ls -1 detectic.* | grep -E '^detectic\.[a-z]{2}$' | sort))
cat "${SORTED_PARTS[@]}" > .detectic.full.tmp
sha256sum -b .detectic.full.tmp | awk '{print $1}' > detectic.sha256
rm -f .detectic.full.tmp

VERSION="$(cat version)"

# Build a FLAT manifest that bootstart.sh's BusyBox-safe `sed` parser expects.
# bootstart.sh extracts:
#   MANIFEST_VERSION = "version"
#   MANIFEST_FULL    = "detectic"        (top-level full-binary hash)
#   PARTS            = "detectic.XX"     -> "<part hash>"  (flat keys)
#
# IMPORTANT: do NOT nest under "files". bootstart.sh greps for flat keys and
# will fail with "manifest_full_hash_missing" if the top-level "detectic" is
# missing. The `version` file must equal MANIFEST_VERSION (self-consistency).
{
    printf '{"version":"%s","detectic":"%s"' "$VERSION" "$(cat detectic.sha256)"
    for part in "${SORTED_PARTS[@]}"; do
        printf ',"%s":"%s"' "$part" "$(cat "$part.sha256")"
    done
    printf '}\n'
} > manifest.json

echo "  $(cat detectic.sha256)  detectic (full)"
for part in "${SORTED_PARTS[@]}"; do
    echo "  $(cat "$part.sha256")  $part"
done
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
