#!/usr/bin/env bash
# Start the Detectic package server for EX520 autostart downloads.
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOCK="${HERE}/.package_server.lock"
LOG="${HERE}/package_server.log"

exec 9>"${LOCK}"
if ! flock -n 9; then
    echo "package_server already running" >> "${LOG}" 2>/dev/null
    exit 0
fi

cd "${HERE}" || exit 1
export PACKAGE_HOST=0.0.0.0
exec python3 -u "${HERE}/package_server.py" >> "${LOG}" 2>&1
