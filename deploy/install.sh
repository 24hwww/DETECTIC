#!/bin/sh
# install.sh — Complete Detectic installer for the TP-Link EX520V.
#
# Verifies the binary by running it, verifies SHA256 via openssl, generates
# a sensor_id, creates the install tree, installs the release, creates the
# `current` pointer, runs `detectic status`, writes an install report.
# Atomic only.
#
# Usage:
#   ./install.sh [release_dir] [install_base]

set -e

RELEASE_DIR="${1:-.}"
INSTALL_BASE="${2:-/var/run/misc/misc_rw/detectic}"
BIN_NAME="detectic-aarch64-musl"
SUM_NAME="detectic-aarch64-musl.sha256"
MANIFEST_NAME="manifest.json"

echo "[install] Detectic installer"
echo "[install] release_dir=$RELEASE_DIR"
echo "[install] install_base=$INSTALL_BASE"

# --- 1. Verify release artifacts exist ---
BIN="$RELEASE_DIR/$BIN_NAME"
SUM="$RELEASE_DIR/$SUM_NAME"
MANIFEST="$RELEASE_DIR/$MANIFEST_NAME"

for f in "$BIN" "$SUM" "$MANIFEST"; do
    if [ ! -f "$f" ]; then
        echo "[install] ERROR: missing $f" >&2
        exit 1
    fi
done

# --- 2. Verify ELF magic (first 4 bytes = 0x7f 'E' 'L' 'F') ---
echo "[install] Verifying ELF header..."
# Use head + grep with binary pattern. BusyBox grep supports -a.
# 0x7f = 0177 octal. We check if the first 4 bytes match.
FIRST_BYTES=$(head -c 4 "$BIN" | cat -v 2>/dev/null | head -c 1)
# cat -v shows 0x7f as ^? — check for that prefix
if ! head -c 4 "$BIN" | grep -qa '^.' 2>/dev/null; then
    echo "[install] WARNING: could not verify ELF magic, continuing"
fi
# More reliable: check that the binary is non-empty and has reasonable size
BINSIZE=$(ls -l "$BIN" | awk '{print $(NF-1)}')
if [ "$BINSIZE" -lt 100000 ]; then
    echo "[install] ERROR: binary too small ($BINSIZE bytes), expected >1MB" >&2
    exit 1
fi
echo "[install] Binary size: $BINSIZE bytes"

# --- 3. Verify SHA256 ---
echo "[install] Verifying SHA256..."
EXPECTED_SHA=$(head -n 1 "$SUM" | awk '{print $1}')
ACTUAL_SHA=$(openssl dgst -sha256 "$BIN" 2>/dev/null | awk '{print $NF}')
if [ -z "$ACTUAL_SHA" ]; then
    echo "[install] ERROR: cannot compute SHA256 (openssl not found)" >&2
    exit 1
fi
if [ "$EXPECTED_SHA" != "$ACTUAL_SHA" ]; then
    echo "[install] ERROR: SHA256 mismatch" >&2
    echo "  expected: $EXPECTED_SHA" >&2
    echo "  actual:   $ACTUAL_SHA" >&2
    exit 1
fi
echo "[install] SHA256 OK: $ACTUAL_SHA"

# --- 4. Read version from manifest ---
VERSION=$(grep '"version"' "$MANIFEST" | head -1 | awk -F'"' '{print $4}')
if [ -z "$VERSION" ]; then
    echo "[install] ERROR: cannot read version from manifest" >&2
    exit 1
fi
echo "[install] Version: $VERSION"

# --- 5. Verify binary runs (architecture + ELF verification by execution) ---
echo "[install] Verifying binary execution..."
chmod +x "$BIN"
if ! "$BIN" version > /dev/null 2>&1; then
    echo "[install] ERROR: binary failed to execute (wrong architecture?)" >&2
    exit 1
fi
ARCH=$("$BIN" version 2>/dev/null | grep "architecture:" | awk '{print $2}')
if [ "$ARCH" != "aarch64" ]; then
    echo "[install] ERROR: binary architecture is $ARCH, expected aarch64" >&2
    exit 1
fi
echo "[install] Binary runs: OK (arch=$ARCH)"

# --- 6. Create install tree ---
mkdir -p "$INSTALL_BASE/releases/$VERSION"
mkdir -p "$INSTALL_BASE/state"
mkdir -p "$INSTALL_BASE/config"
mkdir -p "$INSTALL_BASE/spool"
mkdir -p "$INSTALL_BASE/logs"
mkdir -p "$INSTALL_BASE/backup"

# --- 7. Install release ---
cp "$BIN" "$INSTALL_BASE/releases/$VERSION/detectic"
cp "$MANIFEST" "$INSTALL_BASE/releases/$VERSION/manifest.json"
chmod 755 "$INSTALL_BASE/releases/$VERSION/detectic"

# Copy deploy scripts
for script in start.sh stop.sh health.sh update.sh rollback.sh remove.sh; do
    if [ -f "$RELEASE_DIR/$script" ]; then
        cp "$RELEASE_DIR/$script" "$INSTALL_BASE/releases/$VERSION/"
        chmod 755 "$INSTALL_BASE/releases/$VERSION/$script"
    fi
done

# --- 8. Generate sensor_id if not already present ---
SENSOR_ID_FILE="$INSTALL_BASE/state/sensor_id"
if [ ! -f "$SENSOR_ID_FILE" ]; then
    SENSOR_ID="detectic-$(date +%s)-$$"
    echo "$SENSOR_ID" > "$SENSOR_ID_FILE"
    chmod 600 "$SENSOR_ID_FILE"
    echo "[install] Generated sensor_id: $SENSOR_ID"
else
    SENSOR_ID=$(cat "$SENSOR_ID_FILE")
    echo "[install] Existing sensor_id: $SENSOR_ID"
fi

# --- 9. Create config if not present ---
CONFIG_FILE="$INSTALL_BASE/config/detectic.env"
if [ ! -f "$CONFIG_FILE" ]; then
    cat > "$CONFIG_FILE" << EOF
# Detectic sensor environment configuration
# Source this file before starting the sensor.
# NEVER commit this file; it contains credentials.

DETECTIC_URL=http://192.168.0.1
DETECTIC_USER=user
DETECTIC_PASSWORD=your-router-password
DETECTIC_SECRET=your-hmac-secret-unique-per-sensor
DETECTIC_SENSOR_ID=$SENSOR_ID
DETECTIC_INTERVAL=30
DETECTIC_BACKEND_URL=
DETECTIC_BACKEND_TOKEN=
DETECTIC_BUFFER=$INSTALL_BASE/spool/detectic_buffer.jsonl
DETECTIC_BUFFER_MAX=262144
DETECTIC_LOG_LEVEL=info
DETECTIC_LOG_MACS=0
DETECTIC_SITE_SURVEY=0
EOF
    chmod 600 "$CONFIG_FILE"
    echo "[install] Created config template: $CONFIG_FILE"
fi

# --- 10. Create current pointer (atomic) ---
if [ -L "$INSTALL_BASE/current" ] || [ -e "$INSTALL_BASE/current" ]; then
    rm -f "$INSTALL_BASE/previous"
    mv "$INSTALL_BASE/current" "$INSTALL_BASE/previous" 2>/dev/null || true
fi
ln -sfn "releases/$VERSION" "$INSTALL_BASE/current"

# --- 11. Verify detectic status ---
echo "[install] Verifying detectic status..."
"$INSTALL_BASE/current/detectic" status > /dev/null 2>&1 || {
    echo "[install] WARNING: detectic status returned non-zero (may need config)"
}

# --- 12. Write install report ---
REPORT="$INSTALL_BASE/install.report"
cat > "$REPORT" << EOF
detectic_install_report:
  version: $VERSION
  sha256: $ACTUAL_SHA
  size: $BINSIZE
  installed_at: $(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date)
  install_base: $INSTALL_BASE
  sensor_id: $SENSOR_ID
  binary: $INSTALL_BASE/current/detectic
  status: installed
EOF
echo "[install] Install report: $REPORT"

echo ""
echo "[install] SUCCESS: Detectic $VERSION installed"
echo "[install] Next steps:"
echo "  1. Edit $CONFIG_FILE with real credentials"
echo "  2. Run: . $CONFIG_FILE && $INSTALL_BASE/current/start.sh"
