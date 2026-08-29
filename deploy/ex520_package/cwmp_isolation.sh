#!/bin/sh
# CWMP Isolation Experiment — Stop sensor, observe cwmp, restart sensor
# Runs as root via phoenix.sh on EX520
trap '' 1 15
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

BASE="${DETECTIC_PACKAGE_URL:-http://192.168.0.27:8080}"
DIR="/var/run/misc/misc_rw/detectic"
TMPDIR="/var/tmp/detectic"
LOG="$DIR/cwmp_experiment.log"
RESULTS="$DIR/cwmp_experiment_results.txt"

up() { read u _ < /proc/uptime; echo "$u"; }
log() { echo "[$(up)] $*" >> "$LOG" 2>/dev/null; }
report() { echo "$*" >> "$RESULTS" 2>/dev/null; }
callback() {
    _enc="$(echo "$*" | $BB tr ' ' '_' | $BB head -c 300)"
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=${1:-0}&d=${_enc}" 2>/dev/null || true
}

# Clean previous results
$BB rm -f "$RESULTS" 2>/dev/null
log "=== CWMP ISOLATION EXPERIMENT START ==="

# ======================================================================
# PHASE 1: Record pre-stop state
# ======================================================================
report "=========================================="
report "CWMP STARTUP ISOLATION TEST"
report "Date: $(up)s uptime"
report "=========================================="
report ""

# Sensor state
report "Sensor before:"
if [ -f "$DIR/detectic.pid" ]; then
    _pid="$($BB cat "$DIR/detectic.pid" 2>/dev/null)"
    if [ -n "$_pid" ] && $BB kill -0 "$_pid" 2>/dev/null; then
        report "RUNNING (PID=$_pid)"
    else
        report "NOT RUNNING (stale PID)"
    fi
else
    report "NOT RUNNING (no PID file)"
fi

# Port 7547
_7547="$($BB netstat -tln 2>/dev/null | $BB grep 7547 || echo refused)"
report "TCP/7547 before: $_7547"

# Launcher state
_launcher_pids=""
for _cmdline in /proc/[0-9]*/cmdline; do
    [ -f "$_cmdline" ] || continue
    _cmd="$($BB tr '\0' ' ' < "$_cmdline" 2>/dev/null)"
    case "$_cmd" in
        *"launcher.sh"*|*"bootstart.sh"*)
            _spid="$($BB echo "$_cmdline" | $BB sed 's|/proc/||;s|/cmdline||')"
            _launcher_pids="$_launcher_pids $_spid"
            ;;
    esac
done
if [ -n "$_launcher_pids" ]; then
    report "Launcher before: RUNNING (PIDs:$_launcher_pids)"
else
    report "Launcher before: NOT RUNNING"
fi

report ""

# ======================================================================
# PHASE 2: Kill sensor + launcher
# ======================================================================
log "PHASE 2: Killing sensor and launcher"
report "--- PHASE 2: KILLING SENSOR ---"

# Kill detectic sensor processes
_kill_count=0
for _cmdline in /proc/[0-9]*/cmdline; do
    [ -f "$_cmdline" ] || continue
    _cmd="$($BB tr '\0' ' ' < "$_cmdline" 2>/dev/null)"
    case "$_cmd" in
        *"detectic"*"sensor"*)
            _spid="$($BB echo "$_cmdline" | $BB sed 's|/proc/||;s|/cmdline||')"
            log "Killing sensor PID=$_spid"
            $BB kill "$_spid" 2>/dev/null
            _kill_count=$((_kill_count + 1))
            ;;
    esac
done
$BB sleep 2

# Force kill any remaining
for _cmdline in /proc/[0-9]*/cmdline; do
    [ -f "$_cmdline" ] || continue
    _cmd="$($BB tr '\0' ' ' < "$_cmdline" 2>/dev/null)"
    case "$_cmd" in
        *"detectic"*"sensor"*)
            _spid="$($BB echo "$_cmdline" | $BB sed 's|/proc/||;s|/cmdline||')"
            $BB kill -9 "$_spid" 2>/dev/null || true
            ;;
    esac
done

# Kill launcher loops
for _pid in $_launcher_pids; do
    log "Killing launcher PID=$_pid"
    $BB kill "$_pid" 2>/dev/null || true
    $BB kill -9 "$_pid" 2>/dev/null || true
done

# Remove PID file so launcher won't think sensor is running
$BB rm -f "$DIR/detectic.pid" 2>/dev/null

# Also kill any monitor_loop children (heartbeat loops)
for _cmdline in /proc/[0-9]*/cmdline; do
    [ -f "$_cmdline" ] || continue
    _cmd="$($BB tr '\0' ' ' < "$_cmdline" 2>/dev/null)"
    case "$_cmd" in
        *"sleep"*"30"*)
            _spid="$($BB echo "$_cmdline" | $BB sed 's|/proc/||;s|/cmdline||')"
            _ppid="$($BB cat /proc/$_spid/status 2>/dev/null | $BB grep PPid | $BB awk '{print $2}')"
            # Only kill orphaned sleep processes that were children of launcher
            if [ -n "$_ppid" ]; then
                _pcmd="$($BB tr '\0' ' ' < /proc/$_ppid/cmdline 2>/dev/null)"
                case "$_pcmd" in
                    *"launcher.sh"*)
                        $BB kill "$_spid" 2>/dev/null || true
                        ;;
                esac
            fi
            ;;
    esac
done

$BB sleep 1

# Verify sensor is stopped
_sensor_stopped="YES"
for _cmdline in /proc/[0-9]*/cmdline; do
    [ -f "$_cmdline" ] || continue
    _cmd="$($BB tr '\0' ' ' < "$_cmdline" 2>/dev/null)"
    case "$_cmd" in
        *"detectic"*"sensor"*)
            _sensor_stopped="NO"
            ;;
    esac
done
report "Sensor stopped: $_sensor_stopped"
report "Launcher stopped: YES (killed PIDs:$_launcher_pids)"
report ""

callback 1 "phase2_sensor_stopped=$_sensor_stopped"

# ======================================================================
# PHASE 3: Record network state (EX520 interfaces)
# ======================================================================
log "PHASE 3: Recording network state"
report "--- PHASE 3: NETWORK STATE ---"

report "ip link:"
$BB ip link 2>/dev/null | while IFS= read -r _line; do
    report "  $_line"
done

report ""
report "ip addr:"
$BB ip addr 2>/dev/null | while IFS= read -r _line; do
    report "  $_line"
done

report ""
report "ip route:"
$BB ip route 2>/dev/null | while IFS= read -r _line; do
    report "  $_line"
done

report ""
# Check specific interfaces
for _if in ppp0 eth0 eth1 br-lan ra0 rai0 rax0; do
    _exists="$($BB ip link show "$_if" 2>/dev/null | $BB head -1 || echo "NOT FOUND")"
    report "Interface $_if: $_exists"
done

report ""
callback 2 "phase3_network_recorded"

# ======================================================================
# PHASE 4: Monitor port 7547 for cwmp startup
# ======================================================================
log "PHASE 4: Monitoring port 7547 for cwmp startup"
report "--- PHASE 4: CWMP MONITORING (90 seconds) ---"
report "enableCWMP: 1 (persisted)"
report "ACS URL: http://192.168.0.1:8787/cwmp"
report ""

_monitor_start="$(up)"
report "Monitor start uptime: ${_monitor_start}s"
report ""

_cwmp_detected="NO"
for _i in $(seq 1 45); do
    _t="$($BB cat /proc/uptime 2>/dev/null | $BB awk '{print $1}')"
    _7547="$($BB netstat -tln 2>/dev/null | $BB grep 7547 || echo refused)"
    _sensor_check="not_running"
    for _cmdline in /proc/[0-9]*/cmdline; do
        [ -f "$_cmdline" ] || continue
        _cmd="$($BB tr '\0' ' ' < "$_cmdline" 2>/dev/null)"
        case "$_cmd" in
            *"detectic"*"sensor"*)
                _sensor_check="RUNNING"
                ;;
        esac
    done
    
    if [ "$_7547" != "refused" ] && [ -n "$_7547" ]; then
        report "[${_t}s] port7547=$_7547 sensor=$_sensor_check **CWMP DETECTED**"
        _cwmp_detected="YES"
        callback 3 "cwmp_detected_at_uptime=${_t}"
        break
    fi
    
    # Log every 5th poll to avoid flooding
    _mod=$((_i % 5))
    if [ "$_mod" -eq 1 ] || [ "$_i" -eq 45 ]; then
        report "[${_t}s] port7547=refused sensor=$_sensor_check (poll $_i/45)"
    fi
    
    $BB sleep 2
done

report ""
_monitor_end="$(up)"
report "Monitor end uptime: ${_monitor_end}s"
report ""

# ======================================================================
# PHASE 5: Final state check
# ======================================================================
log "PHASE 5: Final state check"
report "--- PHASE 5: FINAL STATE ---"

# Port 7547 final
_7547_final="$($BB netstat -tln 2>/dev/null | $BB grep 7547 || echo refused)"
report "TCP/7547 final: $_7547_final"

# Check for any new processes
report ""
report "All processes (ps):"
$BB ps 2>/dev/null | while IFS= read -r _line; do
    case "$_line" in
        *"cwmp"*|*"cos"*|*"httpd"*|*"detectic"*)
            report "  $_line"
            ;;
    esac
done

report ""
report "CWMP with sensor OFF: $_cwmp_detected"
if [ "$_cwmp_detected" = "YES" ]; then
    report "TCP/7547: LISTENING"
else
    report "TCP/7547: REFUSED"
fi

callback 4 "final_7547=$_7547_final cwmp=$_cwmp_detected"

# ======================================================================
# PHASE 6: Restart sensor
# ======================================================================
log "PHASE 6: Restarting sensor"
report ""
report "--- PHASE 6: RECOVERY ---"

if [ -f "$DIR/launcher.sh" ]; then
    # Restart via launcher
    ( trap '' 1 2 15; $BB sh "$DIR/launcher.sh" start >> "$LOG" 2>&1 ) &
    $BB sleep 5
    
    _sensor_pid="none"
    if [ -f "$DIR/detectic.pid" ]; then
        _sensor_pid="$($BB cat "$DIR/detectic.pid" 2>/dev/null || echo none)"
    fi
    
    if [ "$_sensor_pid" != "none" ] && $BB kill -0 "$_sensor_pid" 2>/dev/null; then
        report "Sensor restarted: YES (PID=$_sensor_pid)"
    else
        report "Sensor restarted: CHECKING..."
        # Try direct start
        ( trap '' 1; cd "$DIR" && exec "$TMPDIR/detectic" sensor >> "$LOG" 2>&1 ) &
        $BB sleep 3
        _sensor_pid="$!"
        if $BB kill -0 "$_sensor_pid" 2>/dev/null; then
            report "Sensor restarted (direct): YES (PID=$_sensor_pid)"
        else
            report "Sensor restarted: FAILED"
        fi
    fi
else
    report "Recovery: launcher.sh not found!"
fi

# Verify sensor health
$BB sleep 5
_8787="$($BB netstat -tln 2>/dev/null | $BB grep 8787 || echo refused)"
report "Sensor port 8787: $_8787"

report ""
report "=========================================="
report "EXPERIMENT COMPLETE"
report "=========================================="

# Upload results
log "Uploading results"
$BB wget -q -T 10 -O /dev/null --post-file="$RESULTS" "${BASE}/env_line?n=200&d=cwmp_experiment_results" 2>/dev/null || true
$BB sleep 2
$BB wget -q -T 30 -O /dev/null --post-file="$RESULTS" "${BASE}/sensor_log?f=cwmp_experiment_results.txt" 2>/dev/null || true

log "=== CWMP ISOLATION EXPERIMENT END ==="
callback 250 "experiment_complete"
