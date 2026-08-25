#!/usr/bin/env bash
# Start all host-side Detectic services for autonomous EX520 operation:
#   package server :8080  (serves detectic.aa/.ab/launcher.sh/bootstart.sh to router)
#   backend        :8082  (ingestion API; router uploads here)
#   watchdog               (detects cold boot -> GTPR trigger -> phoenix -> bootstart)
#   email is handled by Cloudflare Worker (detectic.24hwww.workers.dev/email)
#
# Each service is detached with setsid so it survives the launching shell.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

start() {
    local name="$1" script="$2"
    if [ -f "${HERE}/.${name}.pid" ] && kill -0 "$(cat "${HERE}/.${name}.pid" 2>/dev/null)" 2>/dev/null; then
        echo "${name} already running"
        return 0
    fi
    echo "starting ${name}..."
    setsid bash "${HERE}/${script}" >/dev/null 2>&1 &
    echo $! > "${HERE}/.${name}.pid"
}

start package_server run_package_server.sh
start backend run_backend.sh
start watchdog run_watchdog.sh

echo "all services launched; logs in ${HERE}/*.log and ${HERE}/.*.log"
