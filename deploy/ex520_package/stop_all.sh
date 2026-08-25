#!/usr/bin/env bash
# Stop all host-side Detectic services started by run_all.sh.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

for name in package_server emaild backend watchdog; do
    pidf="${HERE}/.${name}.pid"
    if [ -f "$pidf" ]; then
        pid="$(cat "$pidf" 2>/dev/null)"
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            echo "stopping ${name} (${pid})"
            kill "$pid" 2>/dev/null || true
        fi
        rm -f "$pidf"
    fi
done
echo "stop requested"
