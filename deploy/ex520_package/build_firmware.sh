#!/bin/bash
# build_firmware.sh — Construir firmware EX520 modificado con Detectic integrado
#
# Este script:
# 1. Copia el rootfs extraído
# 2. Agrega Detectic binary + scripts de autostart
# 3. Modifica rcS para auto-ejecutar Detectic al boot
# 4. Repacka como SquashFS con los mismos parámetros
# 5. Reconstruye la imagen de firmware completa
#
# ⚠️  ADVERTENCIA: Flashear firmware modificado puede brick el router.
#     Solo usar con precaución y tener acceso UART para recovery.
#
# Uso:
#   ./build_firmware.sh [--with-ssh] [--with-telnet]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
FW_IMAGE="$PROJECT_ROOT/EX520_UP_BOOT_2025-07-31_11.34.16.bin"
ROOTFS_DIR="$PROJECT_ROOT/_rootfs"
BUILD_DIR="$PROJECT_ROOT/_fw_build"
OUTPUT_FW="$BUILD_DIR/EX520_DETECTIC_$(date +%Y%m%d_%H%M%S).bin"

# Parse arguments
WITH_SSH=0
WITH_TELNET=0
for arg in "$@"; do
    case $arg in
        --with-ssh) WITH_SSH=1 ;;
        --with-telnet) WITH_TELNET=1 ;;
    esac
done

echo "============================================"
echo " EX520 Firmware Builder — Detectic Edition"
echo " $(date)"
echo "============================================"
echo ""

# --- Step 1: Verify prerequisites ---
echo "[1/8] Verifying prerequisites..."

if [ ! -f "$FW_IMAGE" ]; then
    echo "ERROR: Firmware image not found: $FW_IMAGE"
    exit 1
fi

if [ ! -d "$ROOTFS_DIR" ]; then
    echo "ERROR: Extracted rootfs not found: $ROOTFS_DIR"
    exit 1
fi

# Check for Detectic binary
DETECTIC_BIN="$PROJECT_ROOT/dist/detectic-aarch64-musl"
if [ ! -f "$DETECTIC_BIN" ]; then
    echo "WARNING: Detectic binary not found at $DETECTIC_BIN"
    echo "  Building without Detectic binary (scripts only)"
    DETECTIC_BIN=""
fi

echo "  Firmware: $FW_IMAGE"
echo "  Rootfs:   $ROOTFS_DIR"
echo "  Binary:   ${DETECTIC_BIN:-none}"
echo "  SSH:      $WITH_SSH"
echo "  Telnet:   $WITH_TELNET"
echo ""

# --- Step 2: Create working rootfs ---
echo "[2/8] Creating working rootfs..."
mkdir -p "$BUILD_DIR/rootfs"
rm -rf "$BUILD_DIR/rootfs"/*
cp -a "$ROOTFS_DIR"/* "$BUILD_DIR/rootfs/"

# Fix permissions (SquashFS needs specific permissions)
chmod 755 "$BUILD_DIR/rootfs/etc/init.d/"* 2>/dev/null || true
chmod 755 "$BUILD_DIR/rootfs/bin/busybox" 2>/dev/null || true
echo "  Working rootfs created"

# --- Step 3: Add Detectic files ---
echo "[3/8] Adding Detectic files..."

# Create Detectic directory
mkdir -p "$BUILD_DIR/rootfs/var/run/misc/misc_rw/detectic"
mkdir -p "$BUILD_DIR/rootfs/var/run/misc/misc_rw/detectic/state"
mkdir -p "$BUILD_DIR/rootfs/var/run/misc/misc_rw/detectic/spool"
mkdir -p "$BUILD_DIR/rootfs/var/run/misc/misc_rw/cron"

# Copy Detectic binary if available
if [ -n "$DETECTIC_BIN" ]; then
    cp "$DETECTIC_BIN" "$BUILD_DIR/rootfs/var/run/misc/misc_rw/detectic/detectic"
    chmod 755 "$BUILD_DIR/rootfs/var/run/misc/misc_rw/detectic/detectic"
    echo "  Detectic binary added"
fi

# Create Detectic environment file
cat > "$BUILD_DIR/rootfs/var/run/misc/misc_rw/detectic/detectic.env" << 'ENVEOF'
DETECTIC_URL=http://127.0.0.1
DETECTIC_USER=user
DETECTIC_PASSWORD=CHANGE_ME
DETECTIC_SECRET=CHANGE_ME
DETECTIC_INTERVAL=60
DETECTIC_SENSOR_ID=ex520-001
DETECTIC_LOG_LEVEL=info
DETECTIC_LOG_MACS=false
ENVEOF
echo "  Environment file created"

# Create autostart script
cat > "$BUILD_DIR/rootfs/var/run/misc/misc_rw/detectic/autostart.sh" << 'AUTOSTARTEOF'
#!/bin/sh
# Detectic autostart — ejecutado por crond cada minuto
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
DIR="/var/run/misc/misc_rw/detectic"
DROPBEAR_DIR="/var/tmp/dropbear"
LOG="$DIR/autostart.log"

up() { read u _ < /proc/uptime; echo "$u"; }
log() { echo "[$(up)] $*" >> "$LOG" 2>/dev/null; }

# Keep log bounded
if [ -f "$LOG" ]; then
    $BB tail -c 32768 "$LOG" > "$LOG.tmp" 2>/dev/null
    $BB mv "$LOG.tmp" "$LOG" 2>/dev/null
fi

# Auto-start dropbear (SSH)
if ! $BB pgrep dropbear > /dev/null 2>&1; then
    $BB mkdir -p "$DROPBEAR_DIR" 2>/dev/null
    [ -f "$DROPBEAR_DIR/dropbear_rsa_host_key" ] || \
        dropbearkey -t rsa -f "$DROPBEAR_DIR/dropbear_rsa_host_key" 2>/dev/null
    [ -f "$DROPBEAR_DIR/dropbear_ecdsa_host_key" ] || \
        dropbearkey -t ecdsa -f "$DROPBEAR_DIR/dropbear_ecdsa_host_key" 2>/dev/null
    dropbear -R -p 22 \
        -r "$DROPBEAR_DIR/dropbear_rsa_host_key" \
        -r "$DROPBEAR_DIR/dropbear_ecdsa_host_key" 2>/dev/null &
    log "dropbear started"
fi

# Auto-start crond
if ! $BB pgrep crond > /dev/null 2>&1; then
    crond -c /var/run/misc/misc_rw/cron -b 2>/dev/null &
fi

# Auto-start Detectic (if binary exists)
if [ -x "$DIR/detectic" ] && ! $BB pgrep -f "detectic.*sensor" > /dev/null 2>&1; then
    if [ -f "$DIR/detectic.env" ]; then
        . "$DIR/detectic.env" 2>/dev/null
    fi
    ( trap '' 1; exec "$DIR/detectic" sensor >> "$DIR/detectic.log" 2>&1 ) &
    log "detectic started"
fi
AUTOSTARTEOF
chmod 755 "$BUILD_DIR/rootfs/var/run/misc/misc_rw/detectic/autostart.sh"
echo "  Autostart script created"

# Create crontab
cat > "$BUILD_DIR/rootfs/var/run/misc/misc_rw/cron/root" << 'CRONEOF'
* * * * * /var/run/misc/misc_rw/detectic/autostart.sh
CRONEOF
echo "  Crontab created"

# --- Step 4: Modify rcS for autostart ---
echo "[4/8] Modifying init scripts..."

# Add autostart hook at the end of rcS
# We append to rcS.model (which is sourced by rcS)
RCS_MODEL="$BUILD_DIR/rootfs/etc/init.d/rcS.model"

# Create a new init script that starts crond
cat > "$BUILD_DIR/rootfs/etc/init.d/detectic_init" << 'INITEOF'
#!/bin/sh
# Detectic initialization — started by rcS
# Wait for cos and network to be ready
sleep 5

# Start crond for persistence
mkdir -p /var/run/misc/misc_rw/cron 2>/dev/null
crond -c /var/run/misc/misc_rw/cron -b 2>/dev/null &

# Run autostart once
/var/run/misc/misc_rw/detectic/autostart.sh 2>/dev/null &
INITEOF
chmod 755 "$BUILD_DIR/rootfs/etc/init.d/detectic_init"

# Append to rcS.model (after existing content)
if [ -f "$RCS_MODEL" ]; then
    echo "" >> "$RCS_MODEL"
    echo "################################" >> "$RCS_MODEL"
    echo "# Detectic autostart" >> "$RCS_MODEL"
    echo "################################" >> "$RCS_MODEL"  
    echo "sleep 5 && /etc/init.d/detectic_init &" >> "$RCS_MODEL"
    echo "  rcS.model modified"
else
    echo "  WARNING: rcS.model not found"
fi

# --- Step 5: Enable SSH/Telnet if requested ---
if [ "$WITH_SSH" = "1" ]; then
    echo "[5/8] Enabling SSH (dropbear)..."
    # Create a dropbear startup script
    cat > "$BUILD_DIR/rootfs/etc/init.d/dropbear_init" << 'DBEOF'
#!/bin/sh
# Start dropbear SSH at boot
sleep 8
mkdir -p /var/tmp/dropbear 2>/dev/null
[ -f /var/tmp/dropbear/dropbear_rsa_host_key ] || \
    dropbearkey -t rsa -f /var/tmp/dropbear/dropbear_rsa_host_key 2>/dev/null
[ -f /var/tmp/dropbear/dropbear_ecdsa_host_key ] || \
    dropbearkey -t ecdsa -f /var/tmp/dropbear/dropbear_ecdsa_host_key 2>/dev/null
dropbear -R -p 22 \
    -r /var/tmp/dropbear/dropbear_rsa_host_key \
    -r /var/tmp/dropbear/dropbear_ecdsa_host_key 2>/dev/null &
DBEOF
    chmod 755 "$BUILD_DIR/rootfs/etc/init.d/dropbear_init"
    echo "" >> "$RCS_MODEL"
    echo "/etc/init.d/dropbear_init &" >> "$RCS_MODEL"
    echo "  SSH enabled at boot"
else
    echo "[5/8] SSH not requested (use --with-ssh to enable)"
fi

if [ "$WITH_TELNET" = "1" ]; then
    echo "  Enabling Telnet..."
    cat > "$BUILD_DIR/rootfs/etc/init.d/telnet_init" << 'TELEOF'
#!/bin/sh
# Start telnetd at boot
sleep 8
telnetd -p 23 -l /bin/sh 2>/dev/null &
TELEOF
    chmod 755 "$BUILD_DIR/rootfs/etc/init.d/telnet_init"
    echo "/etc/init.d/telnet_init &" >> "$RCS_MODEL"
    echo "  Telnet enabled at boot"
fi

# --- Step 6: Repack SquashFS ---
echo "[6/8] Repacking SquashFS..."

# Remove the old rootfs.squashfs
rm -f "$BUILD_DIR/rootfs_new.squashfs"

# Repack with same parameters as original:
# -comp xz (XZ compression)
# -b 262144 (256KB block size)
# -no-xattrs (no extended attributes)
mksquashfs "$BUILD_DIR/rootfs" "$BUILD_DIR/rootfs_new.squashfs" \
    -comp xz -b 262144 -no-xattrs -noappend \
    -wildcards -all-root 2>&1 | tail -5

echo "  New SquashFS: $(ls -la $BUILD_DIR/rootfs_new.squashfs | awk '{print $5}') bytes"
echo "  Original:     $(ls -la $BUILD_DIR/rootfs.squashfs | awk '{print $5}') bytes"

# --- Step 7: Rebuild firmware image ---
echo "[7/8] Rebuilding firmware image..."

# The firmware structure:
# [0x0000 - 0x0A08] MediaTek BootROM header (2568 bytes)
# [0x0A08 - 0x323C] U-Boot + padding
# [0x323C - 0xB6D5D] Kernel (XZ compressed)
# [0xB6D5D - 0xC44E7] CRC32 table + LZO compressed data
# [0xC44E7 - 0xD2E89] LZO compressed data  
# [0xD2E89 - 0x100200] Device Tree Blob (6672 bytes)
# [0x100200 - end] UBI volumes (rootfs is SquashFS inside UBI)

# Strategy: Keep everything before the SquashFS, replace the SquashFS
# We need to find where the SquashFS starts inside the UBI and replace it

SQUASHFS_OFFSET=6033920  # 0x5C1840
FW_SIZE=$(stat -c%s "$FW_IMAGE")

echo "  Firmware size: $FW_SIZE bytes"
echo "  SquashFS offset: $SQUASHFS_OFFSET"

# Create new firmware by concatenating:
# [0 - SQUASHFS_OFFSET] + [new SquashFS]
dd if="$FW_IMAGE" bs=1 count=$SQUASHFS_OFFSET 2>/dev/null > "$OUTPUT_FW"
cat "$BUILD_DIR/rootfs_new.squashfs" >> "$OUTPUT_FW"

# Pad to original size (fill with FF)
NEW_SIZE=$(stat -c%s "$OUTPUT_FW")
if [ "$NEW_SIZE" -lt "$FW_SIZE" ]; then
    PADDING=$((FW_SIZE - NEW_SIZE))
    dd if=/dev/zero bs=1 count=$PADDING 2>/dev/null | tr '\0' '\377' >> "$OUTPUT_FW"
fi

echo "  New firmware: $(ls -la $OUTPUT_FW | awk '{print $5}') bytes"
echo "  Original:     $FW_SIZE bytes"

# --- Step 8: Verify ---
echo "[8/8] Verifying..."

# Check that the new firmware has valid structure
NEW_FW_SIZE=$(stat -c%s "$OUTPUT_FW")
echo "  Size check: original=$FW_SIZE new=$NEW_FW_SIZE"

if [ "$NEW_FW_SIZE" -ge "$FW_SIZE" ]; then
    echo "  ✅ Firmware size OK"
else
    echo "  ❌ Firmware too small"
    exit 1
fi

# Verify SquashFS magic in new firmware
SQUASH_MAGIC=$(dd if="$OUTPUT_FW" bs=1 skip=$SQUASHFS_OFFSET count=4 2>/dev/null | xxd -p)
if [ "$SQUASH_MAGIC" = "68737173" ]; then
    echo "  ✅ SquashFS magic (hsqs) found at expected offset"
else
    echo "  ❌ SquashFS magic not found (got: $SQUASH_MAGIC)"
    exit 1
fi

# Calculate SHA256
sha256sum "$OUTPUT_FW" | awk '{print "  SHA256: " $1}'

echo ""
echo "============================================"
echo " Firmware build complete!"
echo "============================================"
echo ""
echo "  Output: $OUTPUT_FW"
echo ""
echo "  To flash (DANGEROUS — requires UART for recovery):"
echo "    1. Connect via UART serial"
echo "    2. Interrupt U-Boot (press Enter during boot)"
echo "    3. Set up TFTP:"
echo "       setenv ipaddr 192.168.1.1"
echo "       setenv serverip 192.168.1.100"
echo "       tftpboot 0x48000000 $(basename $OUTPUT_FW)"
echo "    4. Flash:"
echo "       nand erase.part firmware"
echo "       nand write 0x48000000 firmware"
echo "    5. Reset"
echo ""
echo "  OR via web UI (if accessible):"
echo "    Upload $(basename $OUTPUT_FW) via firmware upgrade page"
echo ""
echo "  ⚠️  WARNING: Always keep UART access for recovery!"
echo "============================================"
