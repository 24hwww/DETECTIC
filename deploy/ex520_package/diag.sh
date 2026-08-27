#!/bin/sh
# Diagnostic: read detectic.log and send it via GET callbacks.
trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BASE="http://192.168.0.27:8080"
LOG="/var/run/misc/misc_rw/detectic/detectic.log"
PIDF="/var/run/misc/misc_rw/detectic/detectic.pid"
BIN="/var/tmp/detectic/detectic"

send_line() {
    _d="$1"
    # URL-encode spaces as _
    _d="$(printf '%s' "$_d" | $BB sed 's/ /_/g' 2>/dev/null)"
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?d=${_d}" 2>/dev/null || true
}

# Basic info
send_line "diag_uptime=$($BB cat /proc/uptime 2>/dev/null)"
send_line "diag_pidfile=$($BB cat "$PIDF" 2>/dev/null || echo none)"
send_line "diag_bin=$($BB ls -la "$BIN" 2>/dev/null || echo missing)"

# Running processes
for _proc in /proc/[0-9]*; do
    _exe="$($BB readlink "$_proc/exe" 2>/dev/null)"
    case "$_exe" in
        *detectic*)
            pid="$($BB basename "$_proc")"
            send_line "diag_proc pid=$pid exe=$_exe"
            ;;
    esac
done
send_line "diag_no_detectic_proc_found"

# Port check
send_line "diag_port8787=$($BB netstat -tlnp 2>/dev/null | $BB grep 8787 || echo not_listening)"

# Log file - send last 30 lines
if [ -f "$LOG" ]; then
    n=0
    $BB tail -n 30 "$LOG" 2>/dev/null | while IFS= read -r _line; do
        n=$((n + 1))
        send_line "diag_log_${n}=${_line}"
    done
else
    send_line "diag_log=missing"
fi

# Env file check
send_line "diag_env_misc=$($BB ls -la /var/run/misc/misc_rw/detectic/detectic.env 2>/dev/null || echo none)"
send_line "diag_env_tmp=$($BB ls -la /var/tmp/detectic/detectic.env 2>/dev/null || echo none)"

# Split parts
send_line "diag_parts=$($BB ls -la /var/tmp/detectic/detectic.* 2>/dev/null || echo none)"

# Try running the binary directly and capture output
send_line "diag_try_run=$($BIN sensor --once 2>&1 | $BB head -c 200 || echo failed)"

$BB wget -q -T 5 -O /dev/null "${BASE}/done?status=diag_done" 2>/dev/null || true
