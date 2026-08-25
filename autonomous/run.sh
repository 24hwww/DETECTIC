#!/usr/bin/env bash
# Detectic autonomous EX520 job — one execution per cron tick.
# Enforces a single job at a time with flock; exit 2 = already running (skip).
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(dirname "$HERE")"
LOCK="${HERE}/.job.lock"
LOG="${HERE}/logs/collector.log"

export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"

exec 9>"${LOCK}"
if ! flock -n 9; then
    echo "$(date -Is) [JOB_SKIP] another collector execution is still running (flock held)" >> "${LOG}"
    exit 2
fi

cd "${REPO}" || exit 1
exec python3 "${HERE}/collector.py" run
