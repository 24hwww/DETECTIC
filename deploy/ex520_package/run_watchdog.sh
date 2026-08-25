#!/usr/bin/env bash
# Start the Detectic cold-boot autostart watchdog for the EX520.
# Sources .env (DETECTIC_PASSWORD, DETECTIC_DIALECT, ...) and puts the host
# `detectic` CLI (used for GTPR query/set) on PATH.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(dirname "$(dirname "$HERE")")"
LOCK="${HERE}/.watchdog.lock"
LOG="${HERE}/watchdog.log"

exec 9>"${LOCK}"
if ! flock -n 9; then
    echo "watchdog already running" >> "${LOG}" 2>/dev/null
    exit 0
fi

# Load environment (SMTP creds, DETECTIC_PASSWORD, DETECTIC_DIALECT, etc.)
if [ -f "${REPO}/.env" ]; then
    set -a
    . "${REPO}/.env" 2>/dev/null || true
    set +a
fi

# Ensure host `detectic` CLI is reachable for GTPR query/set.
export PATH="${REPO}/target/release:${PATH}"

cd "${HERE}" || exit 1
exec python3 "${HERE}/watchdog.py" >> "${LOG}" 2>&1
