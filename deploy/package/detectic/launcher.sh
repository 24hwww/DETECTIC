#!/bin/sh
# Detectic Launcher — minimal POSIX shell for TP-Link EX520
# Location: /var/run/misc/misc_rw/detectic/launcher.sh
# Purpose: start/restart Detectic, prevent duplicates, bounded restart
#
# Usage:
#   launcher.sh start    — start Detectic (skip if already running)
#   launcher.sh stop     — send SIGTERM, wait, SIGKILL if needed
#   launcher.sh status   — print PID or "not running"
#   launcher.sh restart  — stop then start
#   launcher.sh probe    — verify binary exists and is executable

DETECTIC_DIR="/var/run/misc/misc_rw/detectic"
DETECTIC_BIN="${DETECTIC_DIR}/detectic"
PID_FILE="${DETECTIC_DIR}/detectic.pid"
LOG_FILE="${DETECTIC_DIR}/detectic.log"
MAX_LOG_SIZE=102400  # 100 KB max log
MAX_RESTART=5
RESTART_FILE="${DETECTIC_DIR}/restart_count"

# --- helpers ---

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >> "$LOG_FILE" 2>/dev/null
    # rotate log if too large
    if [ -f "$LOG_FILE" ]; then
        sz=$(wc -c < "$LOG_FILE" 2>/dev/null || echo 0)
        if [ "$sz" -gt "$MAX_LOG_SIZE" ] 2>/dev/null; then
            tail -c 51200 "$LOG_FILE" > "${LOG_FILE}.tmp" 2>/dev/null
            mv "${LOG_FILE}.tmp" "$LOG_FILE" 2>/dev/null
        fi
    fi
}

get_pid() {
    if [ -f "$PID_FILE" ]; then
        pid=$(cat "$PID_FILE" 2>/dev/null)
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            echo "$pid"
            return 0
        fi
    fi
    # fallback: grep ps
    pid=$(ps 2>/dev/null | grep '[d]etectic' | awk '{print $1}' | head -1)
    if [ -n "$pid" ]; then
        echo "$pid"
        return 0
    fi
    return 1
}

is_running() {
    get_pid > /dev/null 2>&1
}

get_restart_count() {
    if [ -f "$RESTART_FILE" ]; then
        cat "$RESTART_FILE" 2>/dev/null || echo 0
    else
        echo 0
    fi
}

set_restart_count() {
    echo "$1" > "$RESTART_FILE" 2>/dev/null
}

# --- commands ---

do_probe() {
    if [ ! -f "$DETECTIC_BIN" ]; then
        echo "FAIL: binary not found at $DETECTIC_BIN"
        return 1
    fi
    if [ ! -x "$DETECTIC_BIN" ]; then
        echo "FAIL: binary not executable at $DETECTIC_BIN"
        return 1
    fi
    sz=$(wc -c < "$DETECTIC_BIN" 2>/dev/null || echo 0)
    echo "OK: binary exists, ${sz} bytes, executable"
    return 0
}

do_start() {
    if is_running; then
        pid=$(get_pid)
        echo "Detectic already running (PID $pid)"
        return 0
    fi

    # reset restart counter after successful long run
    set_restart_count 0

    if ! do_probe; then
        return 1
    fi

    log "Starting Detectic"
    
    # Start in background, redirect stdout/stderr to log
    nohup "$DETECTIC_BIN" sensor >> "$LOG_FILE" 2>&1 &
    new_pid=$!
    
    # Save PID
    echo "$new_pid" > "$PID_FILE" 2>/dev/null
    
    # Brief wait to verify it started
    sleep 1
    if kill -0 "$new_pid" 2>/dev/null; then
        log "Detectic started PID=$new_pid"
        echo "Detectic started (PID $new_pid)"
        return 0
    else
        log "Detectic failed to start"
        echo "FAIL: Detectic exited immediately"
        rm -f "$PID_FILE" 2>/dev/null
        return 1
    fi
}

do_stop() {
    pid=$(get_pid 2>/dev/null)
    if [ -z "$pid" ]; then
        echo "Detectic not running"
        return 0
    fi
    
    log "Stopping Detectic PID=$pid"
    
    # SIGTERM first (graceful — Detectic handles it)
    kill "$pid" 2>/dev/null
    
    # Wait up to 5 seconds for graceful exit
    i=0
    while [ $i -lt 5 ]; do
        if ! kill -0 "$pid" 2>/dev/null; then
            log "Detectic stopped gracefully"
            rm -f "$PID_FILE" 2>/dev/null
            echo "Detectic stopped"
            return 0
        fi
        sleep 1
        i=$((i + 1))
    done
    
    # Force kill
    kill -9 "$pid" 2>/dev/null
    sleep 0.5
    rm -f "$PID_FILE" 2>/dev/null
    log "Detectic force-killed"
    echo "Detectic killed (SIGKILL)"
    return 0
}

do_status() {
    pid=$(get_pid 2>/dev/null)
    if [ -n "$pid" ]; then
        echo "running PID=$pid"
        # Show memory if /proc exists
        if [ -f "/proc/$pid/status" ]; then
            grep -E "^(VmRSS|VmSize|Threads)" "/proc/$pid/status" 2>/dev/null
        fi
        return 0
    else
        echo "not running"
        return 1
    fi
}

do_restart() {
    do_stop
    sleep 1
    
    # Check restart budget
    count=$(get_restart_count)
    if [ "$count" -ge "$MAX_RESTART" ]; then
        log "Restart budget exhausted ($count/$MAX_RESTART)"
        echo "FAIL: restart budget exhausted ($count/$MAX_RESTART). Manual intervention required."
        return 1
    fi
    
    set_restart_count $((count + 1))
    do_start
}

# --- main ---

case "${1:-status}" in
    start)   do_start ;;
    stop)    do_stop ;;
    restart) do_restart ;;
    status)  do_status ;;
    probe)   do_probe ;;
    *)
        echo "Usage: $0 {start|stop|restart|status|probe}"
        exit 1
        ;;
esac
