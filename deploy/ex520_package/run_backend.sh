#!/usr/bin/env bash
# Start the Detectic HTTP→HTTPS forwarder on 0.0.0.0:8082.
# The EX520 sends HTTP here; the forwarder proxies to Cloudflare Worker.
#
# Primary backend: https://detectic.24hwww.workers.dev (Cloudflare Worker)
# This forwarder bridges the EX520 (no TLS) to the Worker (HTTPS only).
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOCK="${HERE}/.backend.lock"
LOG="${HERE}/backend.log"

exec 9>"${LOCK}"
if ! flock -n 9; then
    echo "backend/forwarder already running" >> "${LOG}" 2>/dev/null
    exit 0
fi

exec python3 -u "${HERE}/forwarder.py" --port 8082 \
    >> "${LOG}" 2>&1
