#!/bin/sh
# uart_persist_setup.sh — Configurar persistencia total después de acceder por UART
#
# Ejecutar ESTE SCRIPT desde la shell del UART después de hacer login.
# Configura:
#   1. SSH (dropbear) permanente
#   2. crond para auto-reinicio
#   3. Script de autostart en misc_rw
#   4. Detectic binario (si se copió previamente)
#
# Uso desde UART shell:
#   wget -O - http://<host>:8080/uart_persist_setup.sh | sh
#   o copiar y pegar cada bloque manualmente

export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

DIR="/var/run/misc/misc_rw/detectic"
CRON_DIR="/var/run/misc/misc_rw/cron"
DROPBEAR_DIR="/var/tmp/dropbear"
HOST="${HOST:-192.168.0.27}"
LOG="/var/tmp/uart_setup.log"

log() { echo "[$(date)] $*" | tee -a "$LOG"; }

log "=== EX520 UART Persist Setup ==="
log "Host: $HOST"

# --- 1. Crear directorios ---
log "Creating directories..."
$BB mkdir -p "$DIR" "$CRON_DIR" "$DROPBEAR_DIR" /var/tmp/detectic 2>/dev/null

# --- 2. Generar host keys ---
log "Generating SSH host keys..."
if [ ! -f "$DROPBEAR_DIR/dropbear_rsa_host_key" ]; then
    dropbearkey -t rsa -f "$DROPBEAR_DIR/dropbear_rsa_host_key" 2>/dev/null
    log "RSA key generated"
fi
if [ ! -f "$DROPBEAR_DIR/dropbear_ecdsa_host_key" ]; then
    dropbearkey -t ecdsa -f "$DROPBEAR_DIR/dropbear_ecdsa_host_key" 2>/dev/null
    log "ECDSA key generated"
fi

# --- 3. Iniciar dropbear ---
log "Starting dropbear SSH..."
killall dropbear 2>/dev/null || true
$BB sleep 1
dropbear -R -p 22 \
    -r "$DROPBEAR_DIR/dropbear_rsa_host_key" \
    -r "$DROPBEAR_DIR/dropbear_ecdsa_host_key" 2>/dev/null &
$BB sleep 2
if $BB pgrep dropbear > /dev/null 2>&1; then
    log "dropbear OK (PID=$($BB pgrep dropbear))"
else
    log "ERROR: dropbear failed to start"
fi

# --- 4. Crear script de autostart ---
log "Creating autostart script..."
cat > "$DIR/autostart.sh" << 'AUTOSTART'
#!/bin/sh
# Detectic autostart — ejecutado por crond cada minuto
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
DIR="/var/run/misc/misc_rw/detectic"
DROPBEAR_DIR="/var/tmp/dropbear"

# Auto-start dropbear
if ! $BB pgrep dropbear > /dev/null 2>&1; then
    $BB mkdir -p "$DROPBEAR_DIR"
    [ -f "$DROPBEAR_DIR/dropbear_rsa_host_key" ] || \
        dropbearkey -t rsa -f "$DROPBEAR_DIR/dropbear_rsa_host_key" 2>/dev/null
    [ -f "$DROPBEAR_DIR/dropbear_ecdsa_host_key" ] || \
        dropbearkey -t ecdsa -f "$DROPBEAR_DIR/dropbear_ecdsa_host_key" 2>/dev/null
    dropbear -R -p 22 \
        -r "$DROPBEAR_DIR/dropbear_rsa_host_key" \
        -r "$DROPBEAR_DIR/dropbear_ecdsa_host_key" 2>/dev/null &
fi

# Auto-start crond
if ! $BB pgrep crond > /dev/null 2>&1; then
    crond -c /var/run/misc/misc_rw/cron -b 2>/dev/null &
fi
AUTOSTART
chmod +x "$DIR/autostart.sh"
log "autostart.sh created"

# --- 5. Configurar crontab ---
log "Setting up crontab..."
echo "* * * * * $DIR/autostart.sh" > "$CRON_DIR/root"
log "crontab configured"

# --- 6. Iniciar crond ---
log "Starting crond..."
if ! $BB pgrep crond > /dev/null 2>&1; then
    crond -c "$CRON_DIR" -b 2>/dev/null &
    log "crond started"
else
    log "crond already running"
fi

# --- 7. Intentar descargar Detectic binario ---
log "Attempting to download Detectic binary..."
if [ ! -x "$DIR/detectic" ]; then
    $BB wget -q -T 30 -O "$DIR/detectic" "http://$HOST:8080/detectic.aa" 2>/dev/null && \
    $BB wget -q -T 30 -O "$DIR/detectic.tmp" "http://$HOST:8080/detectic.ab" 2>/dev/null && \
    $BB cat "$DIR/detectic" "$DIR/detectic.tmp" > /var/tmp/detectic/detectic 2>/dev/null && \
    $BB rm -f "$DIR/detectic.tmp" && \
    chmod +x /var/tmp/detectic/detectic && \
    log "Detectic binary downloaded" || \
    log "Detectic download failed (optional)"
else
    log "Detectic binary already exists"
fi

# --- 8. Verificar estado ---
log ""
log "=== Verification ==="
log "dropbear: $($BB pgrep dropbear > /dev/null 2>&1 && echo RUNNING || echo STOPPED)"
log "crond:    $($BB pgrep crond > /dev/null 2>&1 && echo RUNNING || echo STOPPED)"
log "SSH port: $(nc -z -w2 127.0.0.1 22 2>/dev/null && echo OPEN || echo CLOSED)"
log ""
log "SSH access: ssh admin@<router_ip> -p 22"
log "Persistence: crond will restart dropbear every minute"
log ""
log "=== Setup complete ==="
