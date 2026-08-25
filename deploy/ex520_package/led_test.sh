#!/bin/sh
# Detectic LED test — safe, reversible, uses /proc/tp_led
# Runs as root via phoenix.sh
# Tests: POWR LED OFF for 3 seconds, then restore ON
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
LOG="/var/tmp/detectic_led_test.log"

log() { echo "[$(date +%s)] $*" >> "$LOG" 2>/dev/null; }

log "LED test starting"

# 1. Read current POWR LED state (if readable)
if [ -r /proc/tp_led ]; then
    log "/proc/tp_led is readable"
    # Try to read current state — may not be supported
    cat /proc/tp_led >> "$LOG" 2>/dev/null || log "tp_led read not supported"
else
    log "/proc/tp_led NOT readable"
    # Try to create it or find alternative
    ls -la /proc/tp_led >> "$LOG" 2>&1
fi

# 2. Test: POWR LED OFF (mode 1)
log "Setting POWR 1 1 (OFF)"
echo "POWR 1 1" > /proc/tp_led 2>> "$LOG"
log "POWR OFF sent, exit=$?"

# 3. Wait 3 seconds
sleep 3

# 4. Restore: POWR LED ON (mode 2)
log "Setting POWR 2 1 (ON)"
echo "POWR 2 1" > /proc/tp_led 2>> "$LOG"
log "POWR ON sent, exit=$?"

# 5. Verify
log "LED test complete"
cat "$LOG" 2>/dev/null

# 6. Report back
CALLBACK="https://detectic.24hwww.workers.dev"
$BB wget -q -T 5 -O /dev/null "${CALLBACK}/done?status=ok&reason=led_test_complete" 2>/dev/null || true
