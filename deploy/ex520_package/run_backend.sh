#!/usr/bin/env bash
# Start the Detectic local relay on 0.0.0.0:8082.
# Security: EX520 → HTTP → relay → cloudflared → Cloudflare Worker
# EX520 never accesses the internet directly.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOCK="${HERE}/.backend.lock"
LOG="${HERE}/backend.log"

exec 9>"${LOCK}"
if ! flock -n 9; then
    echo "relay already running" >> "${LOG}" 2>/dev/null
    exit 0
fi

exec python3 -u "${HERE}/relay.py" --port 8082 \
    >> "${LOG}" 2>&1
