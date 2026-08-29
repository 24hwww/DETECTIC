#!/bin/sh
# ============================================================================
# probe_lifecycle.sh — EX520 process-lifecycle forensic probe (PHASE A).
#
# Installed via phoenix/LIFEMOTE. It:
#   1. Snapshots the full /proc process tree (PID PPID PGID SID + cmdline).
#   2. Locates the Detectic sensor + launcher PIDs.
#   3. Installs a signal trap that records EVERY catchable signal delivered
#      to the probe itself (which runs in the SAME process group/session chain
#      as the sensor, delivered through the same phoenix/cos lifecycle).
#   4. Continuously samples the sensor's liveness + proc status (incl. the
#      SigIgn/SigCgt/SigBlk masks) until it dies or the window elapses.
#   5. On sensor death, dumps all captured signal evidence back to the host.
#
# Output is sent to the host package server via GET env_line callbacks
# (POST is unreliable on this BusyBox). Secrets are never logged.
#
# This probe does NOT modify firmware and does NOT persist anything.
# ============================================================================

BB=/bin/busybox
BASE="${DETECTIC_PACKAGE_URL:-http://192.168.0.27:8080}"
PROBE_DIR="/var/tmp/detectic_probe"
OUT="$PROBE_DIR/lifecycle.out"
SENSOR_SIGFILE="$PROBE_DIR/sensor_signals.log"
mkdir -p "$PROBE_DIR" 2>/dev/null
: > "$OUT"
: > "$SENSOR_SIGFILE"

# --- exit-safe signal logging: every catchable signal we can trap ---
_log_sig() {
    echo "PROBE_SIGNAL sig=$1 pid=$$ ppid=$PPID ts=$(cat /proc/uptime 2>/dev/null | awk '{print $1}')" >> "$SENSOR_SIGFILE"
    # re-arm the trap so a later signal is also recorded
    case "$1" in
        HUP)  trap '_log_sig HUP'  HUP ;;
        INT)  trap '_log_sig INT'  INT ;;
        QUIT) trap '_log_sig QUIT' QUIT ;;
        ABRT) trap '_log_sig ABRT' ABRT ;;
        ALRM) trap '_log_sig ALRM' ALRM ;;
        USR1) trap '_log_sig USR1' USR1 ;;
        USR2) trap '_log_sig USR2' USR2 ;;
        PIPE) trap '_log_sig PIPE' PIPE ;;
        TERM) trap '_log_sig TERM' TERM ;;
    esac
}
trap '_log_sig HUP'  HUP
trap '_log_sig INT'  INT
trap '_log_sig QUIT' QUIT
trap '_log_sig ABRT' ABRT
trap '_log_sig ALRM' ALRM
trap '_log_sig USR1' USR1
trap '_log_sig USR2' USR2
trap '_log_sig PIPE' PIPE
trap '_log_sig TERM' TERM

# --- report helper: GET callback (space->underscore, capped length) ---
report() {
    _line="$1"
    _enc="$(echo "$_line" | $BB tr ' ' '_' | $BB tr '\n' ' ' | $BB head -c 300)"
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?d=${_enc}" 2>/dev/null || true
}

# --- capture /proc state of a given PID: PPID PGID SID state + signal masks ---
proc_state() {
    _pid="$1"
    _status="/proc/$_pid/status"
    [ -r "$_status" ] || { echo "pid=$_pid GONE"; return; }
    _ppid="$(grep '^PPid' "$_status" 2>/dev/null | awk '{print $2}')"
    # PGID/SID: prefer NS* names, fall back to Tgid/Tpgid (BusyBox/kernel varlng)
    _pgid="$(sed -n 's/^NSpgid:[[:space:]]*//p' "$_status" 2>/dev/null)"
    [ -z "$_pgid" ] && _pgid="$(sed -n 's/^Tgid:[[:space:]]*//p' "$_status" 2>/dev/null)"
    _sid="$(sed -n 's/^NSsid:[[:space:]]*//p' "$_status" 2>/dev/null)"
    [ -z "$_sid" ] && _sid="$(_pgid)"
    _state="$(sed -n 's/^State:[[:space:]]*//p' "$_status" 2>/dev/null)"
    _sigign="$(grep '^SigIgn:' "$_status" 2>/dev/null | awk '{print $2}')"
    _sigcgt="$(grep '^SigCgt:' "$_status" 2>/dev/null | awk '{print $2}')"
    _sigblk="$(grep '^SigBlk:' "$_status" 2>/dev/null | awk '{print $2}')"
    echo "pid=$_pid ppid=$_ppid pgid=$_pgid sid=$_sid state=$_state SigIgn=$_sigign SigCgt=$_sigcgt SigBlk=$_sigblk"
}

# --- full process tree snapshot (reads /proc/<n>/stat for tpgid/session/pgrp) ---
tree() {
    _tag="$1"
    echo "=== TREE $_tag ===" >> "$OUT"
    for _p in /proc/[0-9]*; do
        _pid="${_p#/proc/}"
        _stat="/proc/$_pid/stat"
        [ -r "$_stat" ] || continue
        # stat fields: pid (comm) state ppid pgrp session ...
        _comm="$(cut -d' ' -f2 "$_stat" | tr -d '()')"
        _state="$(awk '{print $3}' "$_stat")"
        _ppid="$(awk '{print $4}' "$_stat")"
        _pgrp="$(awk '{print $5}' "$_stat")"
        _sess="$(awk '{print $6}' "$_stat")"
        _cmdline="$(cat "/proc/$_pid/cmdline" 2>/dev/null | tr '\0' ' ' | $BB head -c 80)"
        [ -n "$_cmdline" ] || _cmdline="[$_comm]"
        echo "pid=$_pid ppid=$_ppid pgrp=$_pgrp sess=$_sess state=$_state cmd=$_cmdline" >> "$OUT"
    done
}

echo "PROBE_START pid=$$ ppid=$PPID ts=$(cat /proc/uptime 2>/dev/null | awk '{print $1}')" >> "$OUT"
tree "before"
report "PROBE_START pid=$$ ppid=$PPID base=$BASE"

# --- Start the real sensor via the launcher so probe+launcher+sensor share the
# --- same phoenix/cos lifecycle chain. This lets the probe observe the exact
# --- termination signal/order. (Optional: set PROBE_WATCH_ONLY to observe an
# --- already-running launcher instead of starting one.)
DIR="/var/run/misc/misc_rw/detectic"
BIN="/var/tmp/detectic/detectic"
LAUNCHER="$DIR/launcher.sh"

if [ -z "${PROBE_WATCH_ONLY:-}" ]; then
    report "PROBE_LAUNCH launching $LAUNCHER"
    ( trap '' 1 2 15; $BB sh "$LAUNCHER" restart 2>/var/tmp/launcher.trace >> "$DIR/detectic.log" 2>&1 ) &
    $BB sleep 2
fi

# Find the launcher + sensor by scanning /proc cmdline for our binary/script.
find_procs() {
    _launcher=""
    _sensor=""
    for _p in /proc/[0-9]*; do
        _pid="${_p#/proc/}"
        _cmdline="$(cat "/proc/$_pid/cmdline" 2>/dev/null | tr '\0' ' ')"
        case "$_cmdline" in
            *launcher.sh*) _launcher="$_pid" ;;
            *"$BIN"*sensor*|*detectic*sensor*) _sensor="$_pid" ;;
        esac
    done
    echo "launcher=$_launcher sensor=$_sensor"
}

SAMPLE=0
LAST_SENSOR=""
while [ "$SAMPLE" -lt 60 ]; do
    SAMPLE=$((SAMPLE + 1))
    NOW="$(cat /proc/uptime 2>/dev/null | awk '{print $1}')"
    PROCS="$(find_procs)"
    SENSOR_PID="$(echo "$PROCS" | $BB sed 's/.*sensor=//')"
    LAUNCHER_PID="$(echo "$PROCS" | $BB sed 's/launcher=//;s/ sensor=.*//')"

    if [ -n "$SENSOR_PID" ] && [ "$SENSOR_PID" != "$LAST_SENSOR" ]; then
        report "SENSOR_FOUND pid=$SENSOR_PID at_ts=$NOW"
        proc_state "$SENSOR_PID" >> "$OUT"
        report "SENSOR_STATE $(proc_state "$SENSOR_PID")"
        LAST_SENSOR="$SENSOR_PID"
    fi

    # Track launcher too.
    if [ -n "$LAUNCHER_PID" ]; then
        report "LAUNCHER_STATE $(proc_state "$LAUNCHER_PID")" 2>/dev/null
    fi

    # If we HAD a sensor but it's gone now, record the transition + probe signals.
    if [ -n "$LAST_SENSOR" ] && [ -z "$SENSOR_PID" ]; then
        report "SENSOR_GONE last_pid=$LAST_SENSOR at_ts=$NOW"
        tree "after_sensor_death"
        # Dump every signal the probe (same group/session chain) received.
        while IFS= read -r _l; do
            report "$_l"
        done < "$SENSOR_SIGFILE"
        # snapshot any detectic remnants
        for _p in /proc/[0-9]*; do
            _pid="${_p#/proc/}"
            _cmdline="$(cat "/proc/$_pid/cmdline" 2>/dev/null | tr '\0' ' ')"
            case "$_cmdline" in
                *detectic*|*launcher*|*phoenix*|*cos*) report "REMNANT $(proc_state "$_pid") cmd=$_cmdline" ;;
            esac
        done
        report "SENSOR_DEATH_STOP"
        break
    fi

    $BB sleep 1
done

tree "final"
report "PROBE_DONE sample=$SAMPLE last_sensor=$LAST_SENSOR"
