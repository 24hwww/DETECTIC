#!/usr/bin/env bash
# Start the Detectic emaild daemon for EX520 callbacks.
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(dirname "$(dirname "$HERE")")"
LOCK="${HERE}/.emaild.lock"
LOG="${HERE}/emaild.log"

exec 9>"${LOCK}"
if ! flock -n 9; then
    echo "emaild already running" >> "${LOG}" 2>/dev/null
    exit 0
fi

# Load environment (SMTP credentials, etc.)
if [ -f "${REPO}/.env" ]; then
    set -a
    . "${REPO}/.env" 2>/dev/null || true
    set +a
fi

cd "${HERE}" || exit 1
export DETECTIC_EMAILD_HOST=0.0.0.0
exec python3 -u "${HERE}/emaild.py" >> "${LOG}" 2>&1
