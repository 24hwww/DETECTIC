#!/usr/bin/env bash
# Start the Detectic backend ingestion API on 0.0.0.0:8082.
# The EX520 uploads directly to Cloudflare Worker via HTTPS.
# This local backend is optional (for local testing/development).
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(dirname "$(dirname "$HERE")")"
BACKEND="${REPO}/backend"
LOCK="${HERE}/.backend.lock"
LOG="${HERE}/backend.log"

exec 9>"${LOCK}"
if ! flock -n 9; then
    echo "backend already running" >> "${LOG}" 2>/dev/null
    exit 0
fi

cd "${BACKEND}" || exit 1
exec python3 -u "${BACKEND}/server.py" \
    --host 0.0.0.0 --port 8082 --db "${BACKEND}/backend.db" \
    >> "${LOG}" 2>&1
