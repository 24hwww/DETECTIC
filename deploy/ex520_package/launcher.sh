#!/bin/sh
# Detectic hardened launcher for EX520V (BusyBox-safe, phoenix-safe).
# Location: /var/run/misc/misc_rw/detectic/launcher.sh
#
# Responsibilities:
#   - Ensure detectic.env is owner-only readable (chmod 600).
#   - Redact secrets from any diagnostics/log output.
#   - Start/stop/restart/status the sensor.
#   - Avoid duplicate instances by verifying /proc/<pid>/exe.
#   - Maintain a bounded detectic.log.
#   - Report health via best-effort callbacks.

# Survive SIGHUP after bootstart/phoenix exits.
trap '' 1

export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

DIR="/var/run/misc/misc_rw/detectic"
TMPDIR="/var/tmp/detectic"
BIN="$TMPDIR/detectic"
LOG="$DIR/detectic.log"
ENVF="$DIR/detectic.env"
PIDF="$DIR/detectic.pid"
RFILE="$DIR/restart_count"
MAX_RESTART=5

up() { read u _ < /proc/uptime; echo "$u"; }

# Redact a single value for logging (secrets / sensitive IDs).
redact_value() {
    _v="$1"
    _len="${#_v}"
    if [ "$_len" -le 8 ]; then
        echo "***"
    else
        printf "%s***%s\n" "${_v%????????}" "${_v#????????}"
    fi
}

# Log only non-secret variables.  Secrets are masked.
log() {
    # Mask anything that looks like a secret token / password value in the text.
    _msg="$*"
    # Simple busybox-safe redaction: KEY=value or KEY"value".
    _msg="$(printf '%s' "$_msg" | $BB sed \
        -e 's/\(password\|passwd\|pwd\|secret\|token\|key\|api_key\|auth\)=[^ ]*/\1=***/g' \
        -e 's/\(Password\|password\|token\|secret\|key\)"[^"]*"/\1"***"/g' \
        2>/dev/null)"
    echo "[$(up)] $_msg" >> "$LOG" 2>/dev/null

    # Keep last 50KB.
    if [ -f "$LOG" ]; then
        $BB tail -c 51200 "$LOG" > "$LOG.tmp" 2>/dev/null
        $BB mv "$LOG.tmp" "$LOG" 2>/dev/null
    fi
}

get_pid() {
    if [ -f "$PIDF" ]; then
        p=$($BB cat "$PIDF" 2>/dev/null)
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

# Ensure the env file has the right permissions when launcher runs.
secure_env() {
    if [ -f "$ENVF" ]; then
        $BB chmod 600 "$ENVF" 2>/dev/null || log "chmod 600 $ENVF failed"
    fi
    if [ -f "/var/tmp/detectic/detectic.env" ]; then
        $BB chmod 600 "/var/tmp/detectic/detectic.env" 2>/dev/null || true
    fi
}

ensure_bin() {
    [ -x "$BIN" ] && return 0
    $BB rm -f "$BIN" 2>/dev/null
    $BB mkdir -p "$TMPDIR" 2>/dev/null
    # Reassemble all split parts that exist (aa, ab, ac, ad, ...).
    PARTS="$($BB ls -1 "$TMPDIR"/detectic.* 2>/dev/null | $BB grep -E '^/var/tmp/detectic/detectic\.[a-z]{2}$' | $BB sed 's|.*/||' | sort)"
    if [ -n "$PARTS" ]; then
        $BB rm -f "$BIN.tmp" 2>/dev/null
        for _p in $PARTS; do
            $BB cat "$TMPDIR/$_p" >> "$BIN.tmp" 2>/dev/null || log "reassemble_failed $_p"
        done
        $BB chmod +x "$BIN.tmp" 2>/dev/null
        $BB mv -f "$BIN.tmp" "$BIN" 2>/dev/null
    fi
    [ -x "$BIN" ]
}

do_start() {
    secure_env

    if is_running; then
        pid=$(get_pid)
        log "already running PID=$pid"
        echo "already running PID=$pid"
        return 0
    fi

    if ! ensure_bin; then
        log "FAIL: binary missing or reassembly failed"
        echo "FAIL: binary missing"
        return 1
    fi

    scount 0
    log "Starting Detectic"

    # Prefer /var/tmp copy (fresher) then misc_rw persisted copy.
    # Unset key vars first to clear any stale values inherited from the
    # parent process (phoenix.sh / cos may carry env from a previous deploy).
    unset DETECTIC_BACKEND_URL DETECTIC_UPLOAD_URL DETECTIC_BACKEND_TOKEN
    if [ -f "/var/tmp/detectic/detectic.env" ]; then
        set -a
        . "/var/tmp/detectic/detectic.env" 2>/dev/null
        set +a
        export DETECTIC_ENV_FILE="/var/tmp/detectic/detectic.env"
    elif [ -f "$ENVF" ]; then
        set -a
        . "$ENVF" 2>/dev/null
        set +a
        export DETECTIC_ENV_FILE="$ENVF"
    fi

    # Observability: confirm backend upload URL is set, but redact any secret.
    _upload_url="${DETECTIC_UPLOAD_URL:-UNSET}"
    _backend_url="${DETECTIC_BACKEND_URL:-UNSET}"
    _interval="${DETECTIC_INTERVAL:-UNSET}"
    _sensor_id="${DETECTIC_SENSOR_ID:-UNSET}"
    if [ "$_sensor_id" != "UNSET" ]; then
        _sensor_id="$(redact_value "$_sensor_id")"
    fi
    log "env_check upload_url=$_upload_url backend_url=$_backend_url interval=$_interval sensor_id=$_sensor_id"

    # Diagnostic: send backend_url to package server via GET callback.
    _cb="${DETECTIC_CALLBACK_BASE:-${DETECTIC_PACKAGE_URL:-http://192.168.0.27:8080}}"
    _enc="$(echo "$_backend_url" | $BB tr ' ' '_')"
    $BB wget -q -T 3 -O /dev/null "${_cb}/env_line?n=50&d=launcher_backend_url=${_enc}" 2>/dev/null || true
    # Check if env file was actually sourced
    _envf="${DETECTIC_ENV_FILE:-none}"
    _enc="$(echo "$_envf" | $BB tr ' ' '_')"
    $BB wget -q -T 3 -O /dev/null "${_cb}/env_line?n=49&d=launcher_env_file=${_enc}" 2>/dev/null || true

    ( trap '' 1; exec "$BIN" sensor >> "$LOG" 2>&1 ) &
    new_pid=$!
    echo "$new_pid" > "$PIDF" 2>/dev/null

    $BB sleep 1
    if $BB kill -0 "$new_pid" 2>/dev/null; then
        log "started PID=$new_pid"
        echo "started PID=$new_pid"
        vers=$($BB cat "$DIR/version" 2>/dev/null || echo unknown)
        CALLBACK_BASE="${DETECTIC_CALLBACK_BASE:-${DETECTIC_PACKAGE_URL:-http://192.168.0.27:8080}}"
        $BB wget -q -T 5 -O /dev/null "${CALLBACK_BASE}/done?t=launcher&status=running&pid=$new_pid&version=$vers" 2>/dev/null || true

        # Best-effort, non-blocking email notifications.
        EMAILD=${DETECTIC_EMAILD:-${DETECTIC_CALLBACK_BASE:-https://detectic.24hwww.workers.dev}/email}
        EMAIL_INTERVAL=${DETECTIC_EMAIL_INTERVAL:-300}

        # Export the variables the reporter subshell needs.
        export BB BIN DIR LOG PIDF EMAILD EMAIL_INTERVAL

        # Startup email.
        ( $BB wget -q -T 10 -O /dev/null \
            "${EMAILD}?type=startup&up=$(up)&version=$vers&pid=$new_pid&status=running" 2>/dev/null || true ) &

        # Report loop.
        ( while :; do
            $BB sleep "$EMAIL_INTERVAL"
            p=$(get_pid 2>/dev/null || echo 0)
            [ "$p" = "0" ] && break
            u=$(up)
            v=$($BB cat "$DIR/version" 2>/dev/null || echo unknown)
            devs=$($BB tail -n 200 "$LOG" 2>/dev/null | $BB grep -c 'nearby_observations' || echo 0)
            $BB wget -q -T 10 -O /dev/null \
                "${EMAILD}?type=report&up=$u&version=$v&pid=$p&devices=$devs&interval=$EMAIL_INTERVAL" 2>/dev/null || true
        done ) &

        wait
    fi

    log "failed to start"
    $BB rm -f "$PIDF"
    return 1
}

do_stop() {
    pid=$(get_pid 2>/dev/null)
    if [ -z "$pid" ]; then
        # Fallback: find any running detectic binary by /proc scan.
        for _proc in /proc/[0-9]*; do
            if [ "$($BB readlink "$_proc/exe" 2>/dev/null)" = "$BIN" ]; then
                pid="$($BB basename "$_proc")"
                break
            fi
        done
    fi
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
    $BB sleep 1
    # Aggressive fallback: kill ANY detectic sensor process still alive.
    for _proc in /proc/[0-9]*/cmdline; do
        if $BB grep -ql detectic "$_proc" 2>/dev/null; then
            _spid="$($BB echo "$_proc" | $BB sed 's|/proc/||;s|/cmdline||')"
            if [ "$_spid" != "$$" ]; then
                $BB kill -9 "$_spid" 2>/dev/null || true
            fi
        fi
    done
    $BB sleep 1
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
