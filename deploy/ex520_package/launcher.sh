#!/bin/sh
# Detectic launcher for EX520V (BusyBox-safe, phoenix-safe)
# Location: /var/run/misc/misc_rw/detectic/launcher.sh
# Target: /var/tmp/detectic/detectic (downloaded & reassembled by bootstart.sh)

export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

# Remove stale environment that a previous phoenix/launcher may have inherited.
unset DETECTIC_BACKEND_URL DETECTIC_UPLOAD_URL DETECTIC_BACKEND_TOKEN 2>/dev/null

DIR="/var/run/misc/misc_rw/detectic"
TMPDIR="/var/tmp/detectic"
BIN="$TMPDIR/detectic"
LOG="$DIR/detectic.log"
ENVF="$DIR/detectic.env"
PIDF="$DIR/detectic.pid"
RFILE="$DIR/restart_count"
MAX_RESTART=5

# Package/heartbeat server (same host:port where bootstart.sh was downloaded)
PACKAGE_URL="${DETECTIC_PACKAGE_URL:-http://192.168.0.27:8080}"
HEARTBEAT_INTERVAL=30

up() { read u _ < /proc/uptime; echo "$u"; }

# Ensure the sensor's HTTP control port (DETECTIC_HTTP_PORT, default 8787) is
# reachable from the LAN.  The stock EX520 firewall filters inbound ports other
# than the management port (80), so without this the host cannot reach
# 192.168.0.1:8787 / [fe80::...]:8787.  Idempotent: re-applied on every start.
# Also clears net.ipv6.bindv6only so the dual-stack [::] socket accepts IPv4.
open_firewall() {
    _port="${DETECTIC_HTTP_PORT:-8787}"
    # Prefer the real xtables-multi binaries over BusyBox (which on the EX520
    # has NO iptables/ip6tables applet).  Fall back to $BB just in case.
    _ipt="/usr/sbin/iptables"; [ -x "$_ipt" ] || _ipt="/usr/bin/iptables"; [ -x "$_ipt" ] || _ipt="$BB iptables"
    _ip6t="/usr/sbin/ip6tables"; [ -x "$_ip6t" ] || _ip6t="/usr/bin/ip6tables"; [ -x "$_ip6t" ] || _ip6t="$BB ip6tables"
    # Allow the kernel to map IPv4 connections onto the IPv6 listener.
    echo 0 > /proc/sys/net/ipv6/bindv6only 2>/dev/null
    # IPv4: accept inbound TCP/_port on the LAN bridge (br0).
    $_ipt -C INPUT -i br0 -p tcp --dport "$_port" -j ACCEPT 2>/dev/null \
        || $_ipt -I INPUT 1 -i br0 -p tcp --dport "$_port" -j ACCEPT 2>/dev/null
    # IPv6: accept inbound TCP/_port on the LAN bridge (br0).
    $_ip6t -C INPUT -i br0 -p tcp --dport "$_port" -j ACCEPT 2>/dev/null \
        || $_ip6t -I INPUT 1 -i br0 -p tcp --dport "$_port" -j ACCEPT 2>/dev/null
    log "firewall opened for tcp/$_port on br0 (v4+v6)"
}

log() {
    echo "[$(up)] $*" >> "$LOG" 2>/dev/null
    if [ -f "$LOG" ]; then
        $BB tail -c 51200 "$LOG" > "$LOG.tmp" 2>/dev/null
        $BB cp "$LOG.tmp" "$LOG" 2>/dev/null
        $BB rm -f "$LOG.tmp" 2>/dev/null
    fi
}

get_pid() {
    if [ -f "$PIDF" ]; then
        p=$($BB cat "$PIDF" 2>/dev/null)
        if [ -n "$p" ] && $BB kill -0 "$p" 2>/dev/null; then
            _cmd="$($BB tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null)"
            case "$_cmd" in
                *"detectic"*"sensor"*) echo "$p"; return 0 ;;
            esac
        fi
    fi
    for _cmdline in /proc/[0-9]*/cmdline; do
        [ -f "$_cmdline" ] || continue
        _cmd="$($BB tr '\0' ' ' < "$_cmdline" 2>/dev/null)"
        case "$_cmd" in
            *"detectic"*"sensor"*)
                p=$($BB echo "$_cmdline" | $BB sed 's|/proc/||;s|/cmdline||')
                if [ -n "$p" ] && $BB kill -0 "$p" 2>/dev/null; then
                    echo "$p"
                    return 0
                fi
                ;;
        esac
    done
    return 1
}

is_running() { get_pid >/dev/null 2>&1; }

gcount() { [ -f "$RFILE" ] && $BB cat "$RFILE" 2>/dev/null || echo 0; }
scount() { echo "$1" > "$RFILE" 2>/dev/null; }

# Best-effort heartbeat callback to the package server.
# Does not block or fail the launcher if the server is down.
heartbeat() {
    _pid="${1:-}"
    _status="${2:-running}"
    _up=$(up)
    _vers=$($BB cat "$DIR/version" 2>/dev/null || echo unknown)
    /usr/sbin/curl -m 5 -s -o /dev/null \
        "${PACKAGE_URL}/heartbeat?t=launcher&status=${_status}&pid=${_pid}&up=${_up}&version=${_vers}" 2>/dev/null || true
}

do_probe() {
    if [ ! -x "$BIN" ]; then
        echo "FAIL: binary not found or not executable at $BIN"
        return 1
    fi
    sz=$($BB ls -la "$BIN" 2>/dev/null | $BB awk '{print $5}')
    echo "OK: binary $BIN, size=$sz, executable"
    return 0
}

monitor_loop() {
    _mpid="$1"
    ( trap '' 1 2 15
      while :; do
          $BB sleep "$HEARTBEAT_INTERVAL"
          if $BB kill -0 "$_mpid" 2>/dev/null; then
              heartbeat "$_mpid" "running"
          else
              break
          fi
      done ) &
}

do_start() {
    if is_running; then
        pid=$(get_pid)
        log "already running PID=$pid"
        echo "already running PID=$pid"
        vers=$($BB cat "$DIR/version" 2>/dev/null || echo unknown)
        heartbeat "$pid" "running"
        monitor_loop "$pid"
        return 0
    fi

    scount 0

    if [ ! -x "$BIN" ]; then
        log "FAIL: binary missing or not executable: $BIN"
        echo "FAIL: binary missing"
        return 1
    fi

    # Source env: prefer the volatile copy (may be newer), then the persisted copy.
    # Use `set -a` so every assignment is exported to the Detectic child process.
    set -a
    if [ -f "$TMPDIR/.env" ]; then
        . "$TMPDIR/.env" 2>/dev/null
    elif [ -f "$TMPDIR/detectic.env" ]; then
        . "$TMPDIR/detectic.env" 2>/dev/null
    elif [ -f "$DIR/.env" ]; then
        . "$DIR/.env" 2>/dev/null
    elif [ -f "$ENVF" ]; then
        . "$ENVF" 2>/dev/null
    fi
    set +a

    # Unset stale backend tokens in case dotenv did not catch them.
    unset DETECTIC_BACKEND_URL DETECTIC_UPLOAD_URL DETECTIC_BACKEND_TOKEN 2>/dev/null
    set -a; [ -f "$DIR/.env" ] && . "$DIR/.env" 2>/dev/null; set +a

    # Open the firewall for the HTTP control port before binding.
    open_firewall

    log "Starting Detectic"

    # Start in background; run from $DIR so dotenv/config files are found.
    # stdout/stderr go to log.
    ( trap '' 1; cd "$DIR" && exec "$BIN" sensor >> "$LOG" 2>&1 ) &
    new_pid=$!
    echo "$new_pid" > "$PIDF" 2>/dev/null

    $BB sleep 1
    if $BB kill -0 "$new_pid" 2>/dev/null; then
        log "started PID=$new_pid"
        echo "started PID=$new_pid"
        vers=$($BB cat "$DIR/version" 2>/dev/null || echo unknown)
        heartbeat "$new_pid" "running"
        monitor_loop "$new_pid"
        return 0
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
            heartbeat "$pid" "stopped"
            return 0
        fi
        $BB sleep 1
        i=$((i + 1))
    done
    $BB kill -9 "$pid" 2>/dev/null
    $BB rm -f "$PIDF"
    heartbeat "$pid" "killed"
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
    probe)   do_probe ;;
    *)       echo "usage: $0 {start|stop|restart|status|probe}"; exit 1 ;;
esac
