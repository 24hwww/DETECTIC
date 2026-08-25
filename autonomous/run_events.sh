#!/usr/bin/env bash
# Detectic event-driven email reporter — single instance, runs as daemon.
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(dirname "$HERE")"
LOCK="${HERE}/.event.lock"
LOG="${HERE}/logs/event_reporter.log"

export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"

mkdir -p "${HERE}/logs"

exec 9>"${LOCK}"
if ! flock -n 9; then
    echo "$(date -Is) [EVENT_SKIP] event_reporter already running" >> "${LOG}"
    exit 0
fi

cd "${REPO}" || exit 1
exec python3 "${HERE}/event_reporter.py" >> "${LOG}" 2>&1
