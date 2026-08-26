#!/bin/bash
# build_package.sh — Construir paquete de deployment para EX520
#
# Crea un paquete completo que puede desplegarse via phoenix.sh
# sin modificar el firmware. Incluye:
# - Detectic binary (split para misc_rw)
# - Scripts de autostart
# - Configuración
# - Watchdog para persistencia
#
# Uso:
#   ./build_package.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="$PROJECT_ROOT/_fw_build/package"
DETECTIC_BIN="$PROJECT_ROOT/dist/detectic-aarch64-musl"
PACKAGE_NAME="detectic-ex520-$(date +%Y%m%d_%H%M%S)"

echo "============================================"
echo " EX520 Detectic Package Builder"
echo " $(date)"
echo "============================================"
echo ""

# --- Verify prerequisites ---
if [ ! -f "$DETECTIC_BIN" ]; then
    echo "ERROR: Detectic binary not found: $DETECTIC_BIN"
    echo "  Build with: cargo build --release --target aarch64-unknown-linux-musl"
    exit 1
fi

echo "Detectic binary: $DETECTIC_BIN ($(ls -la $DETECTIC_BIN | awk '{print $5}') bytes)"

# --- Create build directory ---
mkdir -p "$BUILD_DIR"
rm -rf "$BUILD_DIR"/*

# --- Split binary for misc_rw ---
echo "[1/5] Splitting binary for misc_rw..."
SPLIT_SIZE=$((1500 * 1024))  # 1.5MB per part (binary ~2.3MB => two parts)
split -b $SPLIT_SIZE "$DETECTIC_BIN" "$BUILD_DIR/detectic."
ls -la "$BUILD_DIR"/detectic.*
echo ""

# --- Copy scripts ---
echo "[2/5] Copying scripts..."
cp "$SCRIPT_DIR/bootstart.sh" "$BUILD_DIR/"
cp "$SCRIPT_DIR/launcher.sh" "$BUILD_DIR/"
cp "$SCRIPT_DIR/detectic.env" "$BUILD_DIR/"
echo "$PROJECT_ROOT/VERSION" > "$BUILD_DIR/version" 2>/dev/null || echo "dev-$(date +%Y%m%d)" > "$BUILD_DIR/version"
echo ""

# --- Create enhanced bootstart with SSH ---
echo "[3/5] Creating enhanced bootstart..."
cat > "$BUILD_DIR/bootstart_enhanced.sh" << 'BOOTEOF'
#!/bin/sh
# Detectic enhanced bootstart — includes SSH + persistence
# Runs as root from /usr/bin/phoenix.sh

trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

BASE="http://__HOST_IP__:__HOST_PORT__"
DIR="/var/run/misc/misc_rw/detectic"
BAKDIR="/var/run/misc/misc_rw_bak"
TMPPKG="/var/tmp/detectic_pkg"
LOG="$DIR/autostart.log"
DROPBEAR_DIR="/var/tmp/dropbear"

up() { read u _ < /proc/uptime; echo "$u"; }
log() { echo "[$(up)] $*" >> "$LOG" 2>/dev/null; }

# Keep log bounded
if [ -f "$LOG" ]; then
    $BB tail -c 51200 "$LOG" > "$LOG.tmp" 2>/dev/null
    $BB mv "$LOG.tmp" "$LOG" 2>/dev/null
fi

# Free space
$BB rm -f "$DIR/detectic.log" "$DIR/autostart.log" 2>/dev/null || true

$BB mkdir -p "$DIR" "$TMPPKG" "$BAKDIR" "$DROPBEAR_DIR" /var/tmp/detectic 2>/dev/null

# Download package pieces
$BB rm -f "$TMPPKG"/*

if ! $BB wget -q -T 120 -O "$TMPPKG/detectic.aa" "${BASE}/detectic.aa"; then
    log "ERROR: download_aa failed"
    exit 0
fi
if ! $BB wget -q -T 120 -O "$TMPPKG/detectic.ab" "${BASE}/detectic.ab"; then
    log "ERROR: download_ab failed"
    exit 0
fi
if ! $BB wget -q -T 30 -O "$TMPPKG/launcher.sh" "${BASE}/launcher.sh"; then
    log "ERROR: download_launcher failed"
    exit 0
fi
$BB wget -q -T 15 -O "$TMPPKG/detectic.env" "${BASE}/detectic.env" 2>/dev/null || true
$BB wget -q -T 10 -O "$TMPPKG/version" "${BASE}/version" 2>/dev/null || true

# Validate
if [ ! -s "$TMPPKG/detectic.aa" ] || [ ! -s "$TMPPKG/detectic.ab" ]; then
    log "ERROR: empty binary parts"
    exit 0
fi

$BB chmod +x "$TMPPKG/launcher.sh"

# Replace persistent pieces
$BB rm -f "$DIR/detectic.aa"
$BB cp "$TMPPKG/detectic.aa" "$DIR/detectic.aa" 2>/dev/null || true
$BB rm -f "$BAKDIR/detectic.ab"
$BB cp "$TMPPKG/detectic.ab" "$BAKDIR/detectic.ab" 2>/dev/null || true
$BB cp "$TMPPKG/launcher.sh" "$DIR/launcher.sh" 2>/dev/null || true
$BB cp "$TMPPKG/detectic.env" "$DIR/detectic.env" 2>/dev/null || true
[ -f "$TMPPKG/version" ] && $BB cp "$TMPPKG/version" "$DIR/version" 2>/dev/null || true

# Stop existing instance
$BB sh "$DIR/launcher.sh" stop 2>/dev/null || true
$BB rm -f /var/tmp/detectic/detectic

# Reassemble binary
$BB cat "$DIR/detectic.aa" "$BAKDIR/detectic.ab" > /var/tmp/detectic/detectic 2>/dev/null || true
$BB chmod +x /var/tmp/detectic/detectic

# === SSH: Start dropbear ===
if ! $BB pgrep dropbear > /dev/null 2>&1; then
    $BB mkdir -p "$DROPBEAR_DIR" 2>/dev/null
    [ -f "$DROPBEAR_DIR/dropbear_rsa_host_key" ] || \
        dropbearkey -t rsa -f "$DROPBEAR_DIR/dropbear_rsa_host_key" 2>/dev/null
    [ -f "$DROPBEAR_DIR/dropbear_ecdsa_host_key" ] || \
        dropbearkey -t ecdsa -f "$DROPBEAR_DIR/dropbear_ecdsa_host_key" 2>/dev/null
    dropbear -R -p 22 \
        -r "$DROPBEAR_DIR/dropbear_rsa_host_key" \
        -r "$DROPBEAR_DIR/dropbear_ecdsa_host_key" 2>/dev/null &
    log "SSH dropbear started"
fi

# === Crond for persistence ===
if ! $BB pgrep crond > /dev/null 2>&1; then
    mkdir -p /var/run/misc/misc_rw/cron 2>/dev/null
    echo "* * * * * $DIR/autostart.sh" > /var/run/misc/misc_rw/cron/root
    crond -c /var/run/misc/misc_rw/cron -b 2>/dev/null &
    log "crond started"
fi

# === Start Detectic ===
( $BB sh "$DIR/launcher.sh" start 2>/var/tmp/launcher.trace >> "$LOG" 2>&1 ) &
ret=$?
$BB sleep 1

vers=$($BB cat "$DIR/version" 2>/dev/null || echo unknown)
log "bootstart complete version=$vers ret=$ret"
$BB wget -q -T 5 -O /dev/null \
    "${BASE}/done?status=ok&pid=$$&up=$(up)&version=$vers&ret=$ret" 2>/dev/null || true
BOOTEOF

# Replace placeholders
sed -i "s/__HOST_IP__/${HOST_IP:-192.168.0.27}/g" "$BUILD_DIR/bootstart_enhanced.sh"
sed -i "s/__HOST_PORT__/${HOST_PORT:-8080}/g" "$BUILD_DIR/bootstart_enhanced.sh"
chmod 755 "$BUILD_DIR/bootstart_enhanced.sh"
echo ""

# --- Create launcher with persistence ---
echo "[4/5] Creating launcher with persistence..."
cp "$SCRIPT_DIR/launcher.sh" "$BUILD_DIR/launcher_enhanced.sh"

# Create autostart script for crond
cat > "$BUILD_DIR/autostart.sh" << 'AUTOSTARTEOF'
#!/bin/sh
# Detectic autostart — executed by crond every minute
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
DIR="/var/run/misc/misc_rw/detectic"

# Auto-start dropbear
if ! $BB pgrep dropbear > /dev/null 2>&1; then
    $BB mkdir -p /var/tmp/dropbear 2>/dev/null
    [ -f /var/tmp/dropbear/dropbear_rsa_host_key ] || \
        dropbearkey -t rsa -f /var/tmp/dropbear/dropbear_rsa_host_key 2>/dev/null
    [ -f /var/tmp/dropbear/dropbear_ecdsa_host_key ] || \
        dropbearkey -t ecdsa -f /var/tmp/dropbear/dropbear_ecdsa_host_key 2>/dev/null
    dropbear -R -p 22 \
        -r /var/tmp/dropbear/dropbear_rsa_host_key \
        -r /var/tmp/dropbear/dropbear_ecdsa_host_key 2>/dev/null &
fi

# Auto-start crond
if ! $BB pgrep crond > /dev/null 2>&1; then
    crond -c /var/run/misc/misc_rw/cron -b 2>/dev/null &
fi
AUTOSTARTEOF
chmod 755 "$BUILD_DIR/autostart.sh"
echo ""

# --- Create package archive ---
echo "[5/5] Creating package archive..."
cd "$BUILD_DIR"
tar czf "$PROJECT_ROOT/$PACKAGE_NAME.tar.gz" *
cd "$PROJECT_ROOT"

echo ""
echo "============================================"
echo " Package build complete!"
echo "============================================"
echo ""
echo "  Package: $PROJECT_ROOT/$PACKAGE_NAME.tar.gz"
echo "  Size:    $(ls -la $PROJECT_ROOT/$PACKAGE_NAME.tar.gz | awk '{print $5}') bytes"
echo ""
echo "  Contents:"
ls -la "$BUILD_DIR"/ | grep -v '^total' | grep -v '^\.' | awk '{print "    " $NF " (" $5 " bytes)"}'
echo ""
echo "  To deploy:"
echo "    1. Extract on host: tar xzf $PACKAGE_NAME.tar.gz"
echo "    2. Start package server: python3 package_server.py"
echo "    3. Trigger on router: detectic set DEV2_LIFEMOTE_AGENT '{\"enable\":\"1\",\"URL\":\"http://host:8080/bootstart_enhanced.sh\"}'"
echo "    4. SSH will be available at port 22"
echo "    5. crond will persist SSH after reboots"
echo ""
echo "  Or use the watchdog for automatic re-deployment:"
echo "    python3 ssh_watchdog.py"
echo ""
echo "============================================"
