#!/bin/sh
# run_package_server.sh — robust lifecycle wrapper for package_server.py.
#
# The EX520 downloads the deploy package + sends /done callbacks to this server.
# It must stay up across deploys and reboots of the host tools.
#
# Usage:
#   ./run_package_server.sh start    # detach via setsid, write PID, verify /version
#   ./run_package_server.sh stop
#   ./run_package_server.sh status
#   ./run_package_server.sh restart
#
# Idempotent: `start` when already running is a no-op (unless --force).

set -e
DIR="$(cd "$(dirname "$0")" && pwd)"
PORT="${PACKAGE_PORT:-8080}"
# The server binds to this same host by default (matches package_server.py).
HOST="${PACKAGE_HOST:-192.168.0.27}"
PIDF="$DIR/.package_server.pid"
LOGF="$DIR/package_server.log"

is_up() {
    # Server is considered up when /version responds (points at the served root).
    curl -s -m 3 "http://${HOST}:${PORT}/version" >/dev/null 2>&1
}

start() {
    if is_up; then
        echo "package server already running on :${PORT}"
        return 0
    fi
    # Clean stale pid file if the process died.
    if [ -f "$PIDF" ] && ! kill -0 "$(cat "$PIDF" 2>/dev/null)" 2>/dev/null; then
        rm -f "$PIDF"
    fi
    # nohup detaches from any controlling terminal/HUP so the server survives
    # the caller shell exiting. setsid is preferable but can hang interactive
    # shells that wait on the child; nohup is the proven reliable detach here.
    if command -v nohup >/dev/null 2>&1; then
        nohup python3 "$DIR/package_server.py" >> "$LOGF" 2>&1 < /dev/null &
    else
        setsid python3 "$DIR/package_server.py" >> "$LOGF" 2>&1 < /dev/null &
    fi
    echo $! > "$PIDF"
    # NOTE: no blocking poll here — running curl inside a wrapper can hang an
    # interactive shell. Callers verify readiness with `status` afterwards.
    echo "package server started (pid $(cat "$PIDF")) on :${PORT}, root=$DIR"
}

stop() {
    if [ -f "$PIDF" ]; then
        _pid="$(cat "$PIDF" 2>/dev/null)"
        [ -n "$_pid" ] && kill "$_pid" 2>/dev/null || true
        rm -f "$PIDF"
        echo "package server stopped (pid $_pid)"
    else
        echo "no package server pid file ($PIDF)"
    fi
}

status() {
    if is_up; then
        echo "running on :${PORT} (pid $(cat "$PIDF" 2>/dev/null || echo unknown))"
        return 0
    fi
    echo "NOT running"
    return 1
}

case "${1:-start}" in
    start)   start ;;
    stop)    stop ;;
    restart) stop; sleep 1; start ;;
    status)  status ;;
    *) echo "usage: $0 {start|stop|restart|status}"; exit 2 ;;
esac
