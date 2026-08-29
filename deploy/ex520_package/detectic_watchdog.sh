#!/bin/sh
# Detectic on-router watchdog for EX520V (BusyBox-safe, phoenix-safe)
#
# Location: /var/run/misc/misc_rw/detectic/detectic_watchdog.sh
#
# This is a lightweight crash-recovery watchdog that runs INSIDE the EX520.
# It monitors the Detectic sensor process and health endpoint, and restarts
# the sensor via launcher.sh if it crashes or becomes unhealthy.
#
# Design constraints:
#   * BusyBox shell only (no Python, no extra dependencies)
#   * Low-frequency polling (default 30s) to minimize CPU/RAM
#   * Health endpoint check (http://127.0.0.1:8787/health) + process check
#   * Exponential backoff to prevent restart storms
#   * Bounded retry count before giving up
#   * Single instance via PID file
#   * Survives SIGHUP/SIGTERM (launched with trap '' 1 2 15 by bootstart.sh)
#
# This watchdog handles CRASH RECOVERY only.
# Cold-boot autostart requires an external trigger (host watchdog or manual so)
# because no stock firmware mechanism starts processes from misc_rw at boot.
# See AGENTS.md "Path 2.A" and "Cold-boot autostart verification checklist".

export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

DIR="/var/run/misc/misc_rw/detectic"
TMPDIR="/var/tmp/detectic"
BIN="$TMPDIR/detectic"
LAUNCHER="$DIR/launcher.sh"
LOG="$DIR/watchdog.log"
PIDF="$DIR/watchdog.pid"
STATEF="$DIR/watchdog.state"

# Config (overridable via environment)
POLL_INTERVAL="${DETECTIC_WD_POLL:-30}"
HEALTH_URL="${DETECTIC_WD_HEALTH_URL:-http://127.0.0.1:8787/health}"
HEALTH_TIMEOUT="${DETECTIC_WD_HEALTH_TIMEOUT:-3}"
# Number of consecutive failed checks before triggering a restart.
FAIL_THRESHOLD="${DETECTIC_WD_FAIL_THRESHOLD:-3}"
# Backoff: seconds between restart attempts. Doubles up to MAX_BACKOFF.
INITIAL_BACKOFF="${DETECTIC_WD_INITIAL_BACKOFF:-30}"
MAX_BACKOFF="${DETECTIC_WD_MAX_BACKOFF:-300}"
# Maximum restart attempts before giving up (prevents infinite loops).
MAX_RESTARTS="${DETECTIC_WD_MAX_RESTARTS:-10}"
# Cooldown after a successful recovery before resuming normal monitoring.
RECOVERY_GRACE="${DETECTIC_WD_RECOVERY_GRACE:-60}"

up() { read u _ < /proc/uptime; echo "$u"; }

log() {
    echo "[$(up)] $*" >> "$LOG" 2>/dev/null
    # Bounded log: keep last 50 KiB.
    if [ -f "$LOG" ]; then
        $BB tail -c 51200 "$LOG" > "$LOG.tmp" 2>/dev/null
        $BB cp "$LOG.tmp" "$LOG" 2>/dev/null
        $BB rm -f "$LOG.tmp" 2>/dev/null
    fi
}

# --- Single instance guard ---
if [ -f "$PIDF" ]; then
    _old=$($BB cat "$PIDF" 2>/dev/null)
    if [ -n "$_old" ] && $BB kill -0 "$_old" 2>/dev/null; then
        # Check it's actually a watchdog process, not a stale PID reuse.
        _cmd="$($BB tr '\0' ' ' < "/proc/$_old/cmdline" 2>/dev/null)"
        case "$_cmd" in
            *detectic_watchdog*) echo "watchdog already running PID=$_old"; exit 0 ;;
        esac
    fi
fi
echo $$ > "$PIDF" 2>/dev/null

# --- Cleanup on exit ---
cleanup() {
    log "watchdog exiting PID=$$"
    $BB rm -f "$PIDF" 2>/dev/null
    exit 0
}
trap cleanup 1 2 15

# --- State helpers ---
# State file format (key=value, one per line):
#   fail_count=N          consecutive failed checks
#   restart_count=N       total restarts this session
#   backoff=N             current backoff seconds
#   last_restart=N        uptime of last restart attempt
#   recovered=0|1         whether last recovery was successful

read_state() {
    _fail=0; _restarts=0; _backoff="$INITIAL_BACKOFF"; _last_restart=0; _recovered=1
    if [ -f "$STATEF" ]; then
        eval "$($BB cat "$STATEF" 2>/dev/null | $BB grep -E '^(fail_count|restart_count|backoff|last_restart|recovered)=')"
    fi
}

write_state() {
    cat > "$STATEF" 2>/dev/null <<EOF
fail_count=$_fail
restart_count=$_restarts
backoff=$_backoff
last_restart=$_last_restart
recovered=$_recovered
EOF
}

reset_state() {
    _fail=0; _backoff="$INITIAL_BACKOFF"; _recovered=1
    write_state
}

# --- Health checks ---

# Check if the Detectic process is alive via /proc/<pid>/cmdline scan.
process_alive() {
    for _cmdline in /proc/[0-9]*/cmdline; do
        [ -f "$_cmdline" ] || continue
        _cmd="$($BB tr '\0' ' ' < "$_cmdline" 2>/dev/null)"
        case "$_cmd" in
            *"detectic"*"sensor"*) return 0 ;;
        esac
    done
    return 1
}

# Check the health endpoint.  Returns 0 if healthy, 1 otherwise.
# Uses wget (BusyBox) to probe http://127.0.0.1:8787/health.
health_ok() {
    _resp=$($BB wget -q -T "$HEALTH_TIMEOUT" -O - "$HEALTH_URL" 2>/dev/null)
    case "$_resp" in
        *healthy*) return 0 ;;
    esac
    return 1
}

# Combined health check: process must exist AND health endpoint must respond.
sensor_healthy() {
    if ! process_alive; then
        log "health_check: process absent"
        return 1
    fi
    if ! health_ok; then
        log "health_check: process alive but health endpoint failed"
        return 1
    fi
    return 0
}

# --- Recovery ---

restart_sensor() {
    log "RECOVERY: restarting sensor (attempt $_restarts of $MAX_RESTARTS, backoff ${_backoff}s)"

    if [ "$_restarts" -ge "$MAX_RESTARTS" ]; then
        log "RECOVERY_FAILED: max restarts ($MAX_RESTARTS) exceeded, giving up"
        return 1
    fi

    # Kill any stale detectic process before restarting.
    for _proc in /proc/[0-9]*/cmdline; do
        if $BB grep -ql detectic "$_proc" 2>/dev/null; then
            _spid="$($BB echo "$_proc" | $BB sed 's|/proc/||;s|/cmdline||')"
            case "$($BB tr '\0' ' ' < "$_proc" 2>/dev/null)" in
                *detectic_watchdog*) ;;  # don't kill ourselves
                *) $BB kill -9 "$_spid" 2>/dev/null || true ;;
            esac
        fi
    done
    $BB sleep 1

    # Restart via launcher.sh (which handles firewall, env, etc.).
    if [ -x "$LAUNCHER" ]; then
        $BB sh "$LAUNCHER" stop 2>/dev/null || true
        $BB sleep 1
        $BB sh "$LAUNCHER" start 2>>"$LOG" >>"$LOG"
    else
        log "RECOVERY_FAILED: launcher.sh not found at $LAUNCHER"
        return 1
    fi

    _restarts=$((_restarts + 1))
    _last_restart=$(up)
    _recovered=0
    write_state

    # Wait for the sensor to come back up (bounded by backoff).
    log "RECOVERY: waiting ${RECOVERY_GRACE}s for sensor to stabilize"
    $BB sleep "$RECOVERY_GRACE"

    if sensor_healthy; then
        log "RECOVERY_SUCCESS: sensor healthy after restart"
        _recovered=1
        _fail=0
        _backoff="$INITIAL_BACKOFF"
        write_state
        return 0
    else
        log "RECOVERY_FAILED: sensor still unhealthy after restart"
        # Increase backoff for next attempt (exponential, capped).
        _backoff=$((_backoff * 2))
        if [ "$_backoff" -gt "$MAX_BACKOFF" ]; then
            _backoff="$MAX_BACKOFF"
        fi
        write_state
        return 1
    fi
}

# --- Main loop ---

log "watchdog starting PID=$$ poll=${POLL_INTERVAL}s health=$HEALTH_URL"
log "config: fail_threshold=$FAIL_THRESHOLD backoff=$INITIAL_BACKOFF..$MAX_BACKOFF max_restarts=$MAX_RESTARTS"

# Initialize state.
read_state
write_state

while :; do
    if sensor_healthy; then
        # Sensor is healthy.  Reset fail count if we were previously failing.
        if [ "$_fail" -gt 0 ]; then
            log "sensor healthy, resetting fail_count (was $_fail)"
        fi
        _fail=0
        write_state
    else
        _fail=$((_fail + 1))
        log "sensor unhealthy, fail_count=$_fail/$FAIL_THRESHOLD"
        write_state

        if [ "$_fail" -ge "$FAIL_THRESHOLD" ]; then
            log "fail threshold reached, attempting recovery"
            if ! restart_sensor; then
                if [ "$_restarts" -ge "$MAX_RESTARTS" ]; then
                    log "DEGRADED: max restarts exceeded, entering long sleep"
                    # Sleep for a long time before trying again to avoid
                    # consuming resources in a hopeless loop.
                    $BB sleep "$MAX_BACKOFF"
                    # Reset restart count to allow another cycle.
                    _restarts=0
                    write_state
                fi
            fi
            # After a recovery attempt (success or fail), sleep the backoff
            # before the next check to avoid hammering.
            $BB sleep "$_backoff"
            continue
        fi
    fi

    $BB sleep "$POLL_INTERVAL"
done
