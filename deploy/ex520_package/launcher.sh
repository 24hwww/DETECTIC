#!/bin/sh
# Detectic launcher for EX520V (BusyBox-safe, phoenix-safe)
# Location: /var/run/misc/misc_rw/detectic/launcher.sh

# survive SIGHUP after bootstart/phoenix exits
trap '' 1

export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

DIR="/var/run/misc/misc_rw/detectic"
BAKDIR="/var/run/misc/misc_rw_bak"
TMPDIR="/var/tmp/detectic"
BIN="$TMPDIR/detectic"
LOG="$DIR/detectic.log"
ENVF="$DIR/detectic.env"
PIDF="$DIR/detectic.pid"
RFILE="$DIR/restart_count"
MAX_RESTART=5

up() { read u _ < /proc/uptime; echo "$u"; }

log() {
    echo "[$(up)] $*" >> "$LOG" 2>/dev/null
    # keep last 50KB
    if [ -f "$LOG" ]; then
        $BB tail -c 51200 "$LOG" > "$LOG.tmp" 2>/dev/null
        $BB mv "$LOG.tmp" "$LOG" 2>/dev/null
    fi
}

get_pid() {
    if [ -f "$PIDF" ]; then
        p=$($BB cat "$PIDF" 2>/dev/null)
        # Verify PID exists AND its executable is the actual Detectic binary,
        # not a stale PID that has been reused by a shell or another process.
        if [ -n "$p" ] && [ -d "/proc/$p" ] && \
           [ "$($BB readlink "/proc/$p/exe" 2>/dev/null)" = "$BIN" ]; then
            echo "$p"
            return 0
        fi
    fi
    return 1
}

is_running() { get_pid >/dev/null 2>&1; }

gcount() { [ -f "$RFILE" ] && $BB cat "$RFILE" 2>/dev/null || echo 0; }
scount() { echo "$1" > "$RFILE" 2>/dev/null; }

ensure_bin() {
    [ -x "$BIN" ] && return 0
    $BB rm -f "$BIN" 2>/dev/null
    $BB mkdir -p "$TMPDIR" 2>/dev/null
    if [ -s "$TMPDIR/detectic.aa" ] && [ -s "$TMPDIR/detectic.ab" ]; then
        $BB cat "$TMPDIR/detectic.aa" "$TMPDIR/detectic.ab" > "$BIN" 2>/dev/null
        $BB chmod +x "$BIN"
    fi
    [ -x "$BIN" ]
}

do_start() {
    if is_running; then
        pid=$(get_pid)
        log "already running PID=$pid"
        echo "already running PID=$pid"
        return 0
    fi

    if ! ensure_bin; then
        log "FAIL: binary missing or decompression failed"
        echo "FAIL: binary missing"
        return 1
    fi

    scount 0
    log "Starting Detectic"

    # Source env: prefer /var/tmp copy (may have newer config), then misc_rw
    if [ -f "/var/tmp/detectic/detectic.env" ]; then
        set -a
        . "/var/tmp/detectic/detectic.env" 2>/dev/null
        set +a
    elif [ -f "$ENVF" ]; then
        set -a
        . "$ENVF" 2>/dev/null
        set +a
    fi
    # Observability: confirm the backend upload URL is visible to the sensor.
    log "env_check upload_url=${DETECTIC_UPLOAD_URL:-UNSET} backend_url=${DETECTIC_BACKEND_URL:-UNSET} interval=${DETECTIC_INTERVAL:-UNSET}"

    ( trap '' 1; exec "$BIN" sensor >> "$LOG" 2>&1 ) &
    new_pid=$!
    echo "$new_pid" > "$PIDF" 2>/dev/null

    $BB sleep 1
    if $BB kill -0 "$new_pid" 2>/dev/null; then
        log "started PID=$new_pid"
        echo "started PID=$new_pid"
        vers=$($BB cat "$DIR/version" 2>/dev/null || echo unknown)
        CALLBACK_BASE="${DETECTIC_CALLBACK_BASE:-https://detectic.24hwww.workers.dev}"
        $BB wget -q -T 5 -O /dev/null "${CALLBACK_BASE}/done?t=launcher&status=running&pid=$new_pid&version=$vers" 2>/dev/null || true

        # best-effort, non-blocking email notifications
        EMAILD=${DETECTIC_EMAILD:-${DETECTIC_CALLBACK_BASE:-https://detectic.24hwww.workers.dev}/email}
        EMAIL_INTERVAL=${DETECTIC_EMAIL_INTERVAL:-300}

        # Export the variables the reporter subshell and get_pid need.
        export BB BIN DIR LOG PIDF EMAILD EMAIL_INTERVAL

        # startup email (do not block if emaild is unavailable)
        ( $BB wget -q -T 10 -O /dev/null "${EMAILD}?type=startup&up=$(up)&version=$vers&pid=$new_pid&status=running" 2>/dev/null || true ) &

        # 5-minute report loop (best-effort; stops if Detectic disappears)
        ( while :; do
            $BB sleep "$EMAIL_INTERVAL"
            p=$(get_pid 2>/dev/null || echo 0)
            [ "$p" = "0" ] && break
            u=$(up)
            v=$($BB cat "$DIR/version" 2>/dev/null || echo unknown)
            devs=$($BB tail -n 200 "$LOG" 2>/dev/null | $BB grep -c 'nearby_observations' || echo 0)
            $BB wget -q -T 10 -O /dev/null "${EMAILD}?type=report&up=$u&version=$v&pid=$p&devices=$devs&interval=$EMAIL_INTERVAL" 2>/dev/null || true
        done ) &

        # Keep launcher alive so the background report loop survives
        wait
    fi

    log "failed to start"
    $BB rm -f "$PIDF"
    return 1
}

do_stop() {
    pid=$(get_pid 2>/dev/null)
    if [ -z "$pid" ]; then
        echo "not running"
        return 0
    fi
    log "Stopping PID=$pid"
    $BB kill "$pid" 2>/dev/null
    i=0
    while [ $i -lt 5 ]; do
        if ! $BB kill -0 "$pid" 2>/dev/null; then
            $BB rm -f "$PIDF"
            return 0
        fi
        $BB sleep 1
        i=$((i + 1))
    done
    $BB kill -9 "$pid" 2>/dev/null
    $BB rm -f "$PIDF"
    return 0
}

do_restart() {
    do_stop
    count=$(gcount)
    if [ "$count" -ge "$MAX_RESTART" ]; then
        log "restart budget exhausted"
        echo "FAIL: restart budget exhausted"
        return 1
    fi
    scount $((count + 1))
    do_start
}

do_status() {
    if is_running; then
        pid=$(get_pid)
        echo "running PID=$pid"
        return 0
    fi
    echo "not running"
    return 1
}

case "${1:-status}" in
    start)   do_start ;;
    stop)    do_stop ;;
    restart) do_restart ;;
    status)  do_status ;;
    *)       echo "usage: $0 {start|stop|restart|status}"; exit 1 ;;
esac
