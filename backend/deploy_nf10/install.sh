#!/bin/bash
# ============================================================================
# Detectic Backend — nf-compute-10 Deployment Script
# Target: 0.1 vCPU / 256 MB RAM / 1024 MB storage
# ============================================================================
set -euo pipefail

# --- Configuration (override via env vars) ---
DETECTIC_PORT="${DETECTIC_PORT:-8080}"
DETECTIC_DB="${DETECTIC_DB:-/opt/detectic/data/backend.db}"
DETECTIC_SENSORS="${DETECTIC_SENSORS:-}"
DETECTIC_MASTER_SECRET="${DETECTIC_MASTER_SECRET:-}"
DETECTIC_HOST="${DETECTIC_HOST:-0.0.0.0}"
DETECTIC_MAX_THREADS="${DETECTIC_MAX_THREADS:-8}"
DETECTIC_RATE_BURST="${DETECTIC_RATE_BURST:-30}"
DETECTIC_INSTALL_DIR="${DETECTIC_INSTALL_DIR:-/opt/detectic}"

# --- Colors ---
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log()  { echo -e "${GREEN}[detectic]${NC} $*"; }
warn() { echo -e "${YELLOW}[detectic]${NC} $*"; }
err()  { echo -e "${RED}[detectic]${NC} $*" >&2; }

# --- Pre-flight checks ---
check_system() {
    log "Checking system resources..."

    local total_mem_kb
    total_mem_kb=$(awk '/^MemTotal:/ {print $2}' /proc/meminfo 2>/dev/null || echo "0")
    local total_mem_mb=$((total_mem_kb / 1024))
    log "  RAM: ${total_mem_mb} MB"

    local avail_disk_mb
    avail_disk_mb=$(df -BM /opt 2>/dev/null | awk 'NR==2 {gsub(/M/,"",$4); print $4}' || echo "0")
    log "  Available disk: ${avail_disk_mb} MB"

    local cpus
    cpus=$(nproc 2>/dev/null || echo "1")
    log "  CPUs: ${cpus}"

    if [ "$total_mem_mb" -lt 64 ]; then
        err "Insufficient RAM (${total_mem_mb} MB < 64 MB minimum)"
        exit 1
    fi
    if [ "$avail_disk_mb" -lt 50 ]; then
        err "Insufficient disk (${avail_disk_mb} MB < 50 MB minimum)"
        exit 1
    fi

    log "System check passed"
}

# --- Install Python if needed ---
install_python() {
    if command -v python3 &>/dev/null; then
        local pyver
        pyver=$(python3 --version 2>&1 | awk '{print $2}')
        log "Python3 found: ${pyver}"
        return 0
    fi

    log "Installing Python3..."
    if command -v apt-get &>/dev/null; then
        apt-get update -qq
        apt-get install -y -qq python3 python3-minimal
    elif command -v yum &>/dev/null; then
        yum install -y -q python3
    elif command -v dnf &>/dev/null; then
        dnf install -y -q python3
    elif command -v apk &>/dev/null; then
        apk add --no-cache python3
    else
        err "Cannot install Python3 automatically"
        exit 1
    fi
}

# --- Generate secrets ---
generate_secrets() {
    if [ -z "$DETECTIC_MASTER_SECRET" ]; then
        DETECTIC_MASTER_SECRET=$(openssl rand -hex 32 2>/dev/null || head -c 64 /dev/urandom | od -An -tx1 | tr -d ' \n' | head -c 64)
        log "Generated DETECTIC_MASTER_SECRET"
    fi
    if [ -z "$DETECTIC_SENSORS" ]; then
        local sensor_secret
        sensor_secret=$(openssl rand -hex 16 2>/dev/null || head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' | head -c 32)
        DETECTIC_SENSORS="{\"ex520-001\":\"${sensor_secret}\"}"
        log "Generated sensor secret for ex520-001"
        log "  SAVE THIS: DETECTIC_SENSORS=${DETECTIC_SENSORS}"
    fi
}

# --- Create directories ---
setup_dirs() {
    log "Creating directories..."
    mkdir -p "${DETECTIC_INSTALL_DIR}"/{bin,data,logs,config}
    chmod 700 "${DETECTIC_INSTALL_DIR}/data"
    chmod 700 "${DETECTIC_INSTALL_DIR}/config"
}

# --- Deploy backend ---
deploy_backend() {
    log "Deploying backend..."
    local script_dir
    script_dir=$(cd "$(dirname "$0")" && pwd)

    cp "${script_dir}/../server.py" "${DETECTIC_INSTALL_DIR}/bin/server.py"
    chmod +x "${DETECTIC_INSTALL_DIR}/bin/server.py"

    # Write sensors.json
    echo "${DETECTIC_SENSORS}" | python3 -m json.tool > "${DETECTIC_INSTALL_DIR}/config/sensors.json" 2>/dev/null \
        || echo "${DETECTIC_SENSORS}" > "${DETECTIC_INSTALL_DIR}/config/sensors.json"
}

# --- Create env file ---
create_env() {
    log "Creating environment file..."
    cat > "${DETECTIC_INSTALL_DIR}/config/detectic.env" <<EOF
DETECTIC_MASTER_SECRET=${DETECTIC_MASTER_SECRET}
DETECTIC_SENSORS=${DETECTIC_SENSORS}
EOF
    chmod 600 "${DETECTIC_INSTALL_DIR}/config/detectic.env"
}

# --- Create systemd service ---
create_service() {
    if [ ! -d /etc/systemd/system ]; then
        warn "systemd not found — creating init.d script instead"
        create_initd
        return
    fi

    log "Creating systemd service..."
    cat > /etc/systemd/system/detectic-backend.service <<EOF
[Unit]
Description=Detectic Backend API
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=root
WorkingDirectory=${DETECTIC_INSTALL_DIR}
EnvironmentFile=${DETECTIC_INSTALL_DIR}/config/detectic.env
ExecStart=$(which python3) -u ${DETECTIC_INSTALL_DIR}/bin/server.py \\
    --host ${DETECTIC_HOST} \\
    --port ${DETECTIC_PORT} \\
    --db ${DETECTIC_DB} \\
    --max-threads ${DETECTIC_MAX_THREADS} \\
    --rate-burst ${DETECTIC_RATE_BURST}
Restart=always
RestartSec=5
StartLimitInterval=60
StartLimitBurst=5

# Memory limits for 256 MB system
MemoryMax=128M
MemoryHigh=96M

# I/O limits
IOWeight=50
CPUWeight=50

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ReadWritePaths=${DETECTIC_INSTALL_DIR}/data
ProtectHome=yes
PrivateTmp=yes

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=detectic-backend

[Install]
WantedBy=multi-user.target
EOF

    systemctl daemon-reload
    systemctl enable detectic-backend
    log "Service created and enabled"
}

create_initd() {
    cat > /etc/init.d/detectic-backend <<'INITEOF'
#!/bin/sh
### BEGIN INIT INFO
# Provides:          detectic-backend
# Required-Start:    $network
# Required-Stop:     $network
# Default-Start:     2 3 4 5
# Default-Stop:      0 1 6
# Description:       Detectic Backend API
### END INIT INFO

DETECTIC_DIR="/opt/detectic"
PIDFILE="${DETECTIC_DIR}/data/backend.pid"

start() {
    if [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
        echo "detectic-backend already running (PID $(cat "$PIDFILE"))"
        return 0
    fi
    . "${DETECTIC_DIR}/config/detectic.env"
    cd "${DETECTIC_DIR}"
    nohup python3 -u bin/server.py \
        --host 0.0.0.0 --port 8080 \
        --db "${DETECTIC_DIR}/data/backend.db" \
        --max-threads 8 \
        --rate-burst 30 \
        >> "${DETECTIC_DIR}/logs/backend.log" 2>&1 &
    echo $! > "$PIDFILE"
    echo "detectic-backend started (PID $!)"
}

stop() {
    if [ -f "$PIDFILE" ]; then
        kill "$(cat "$PIDFILE")" 2>/dev/null
        rm -f "$PIDFILE"
        echo "detectic-backend stopped"
    fi
}

status() {
    if [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
        echo "detectic-backend running (PID $(cat "$PIDFILE"))"
    else
        echo "detectic-backend not running"
    fi
}

case "$1" in
    start)   start ;;
    stop)    stop ;;
    restart) stop; sleep 1; start ;;
    status)  status ;;
    *)       echo "Usage: $0 {start|stop|restart|status}" ;;
esac
INITEOF
    chmod +x /etc/init.d/detectic-backend
}

# --- Create cron for DB maintenance ---
create_maintenance_cron() {
    log "Creating maintenance cron..."
    cat > /etc/cron.daily/detectic-maintenance <<'EOF'
#!/bin/sh
# Daily maintenance: vacuum DB, check size
DB="/opt/detectic/data/backend.db"
if [ -f "$DB" ]; then
    # Check DB size
    SIZE=$(stat -c%s "$DB" 2>/dev/null || echo "0")
    MAX_SIZE=$((500 * 1024 * 1024))  # 500 MB limit
    if [ "$SIZE" -gt "$MAX_SIZE" ]; then
        echo "$(date): detectic DB exceeded ${MAX_SIZE} bytes, rotating" >> /opt/detectic/logs/maintenance.log
        mv "$DB" "${DB}.$(date +%Y%m%d).bak"
    fi
    # Vacuum to reclaim space
    sqlite3 "$DB" "VACUUM;" 2>/dev/null
fi
EOF
    chmod +x /etc/cron.daily/detectic-maintenance
}

# --- Print summary ---
print_summary() {
    echo ""
    log "============================================"
    log "  Detectic Backend — Installation Complete"
    log "============================================"
    echo ""
    log "  Install dir:  ${DETECTIC_INSTALL_DIR}"
    log "  Database:     ${DETECTIC_DB}"
    log "  Listen:       http://${DETECTIC_HOST}:${DETECTIC_PORT}"
    log "  Threads:      ${DETECTIC_MAX_THREADS}"
    log "  Rate burst:   ${DETECTIC_RATE_BURST}"
    echo ""
    log "  Endpoints:"
    log "    POST /api/v1/events       — ingest snapshot"
    log "    POST /api/v1/events/batch — batch ingest"
    log "    GET  /api/v1/devices      — device history"
    log "    GET  /api/v1/presence     — presence analytics"
    log "    GET  /api/v1/sensors      — sensor list"
    log "    GET  /api/v1/stats        — global stats"
    log "    GET  /api/v1/healthz      — health check"
    echo ""
    log "  Sensor config (EX520):"
    log "    DETECTIC_UPLOAD_URL=http://<this-server>:${DETECTIC_PORT}/api/v1/events"
    log "    DETECTIC_SECRET=<your-sensor-secret>"
    echo ""

    if [ -d /etc/systemd/system ]; then
        log "  Start: systemctl start detectic-backend"
        log "  Logs:  journalctl -u detectic-backend -f"
    else
        log "  Start: /etc/init.d/detectic-backend start"
        log "  Logs:  tail -f ${DETECTIC_INSTALL_DIR}/logs/backend.log"
    fi
    echo ""
}

# --- Main ---
main() {
    log "Detectic Backend Installer (nf-compute-10 optimized)"
    echo ""

    check_system
    install_python
    generate_secrets
    setup_dirs
    deploy_backend
    create_env
    create_service
    create_maintenance_cron
    print_summary

    log "To start now:"
    if [ -d /etc/systemd/system ]; then
        log "  systemctl start detectic-backend"
    else
        log "  /etc/init.d/detectic-backend start"
    fi
}

main "$@"
