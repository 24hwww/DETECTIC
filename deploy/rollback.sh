#!/bin/sh
# rollback.sh — Revert to the previous verified release.

set -e

INSTALL_BASE="${1:-/var/run/misc/misc_rw/detectic}"

if [ ! -d "$INSTALL_BASE/previous" ]; then
    echo "[rollback] ERROR: no previous release to roll back to" >&2
    exit 1
fi

PREVIOUS=$(readlink -f "$INSTALL_BASE/previous" 2>/dev/null || echo "$INSTALL_BASE/previous")

echo "[rollback] Rolling back to $PREVIOUS"

rm -f "$INSTALL_BASE/current"
ln -sfn "$PREVIOUS" "$INSTALL_BASE/current"

# Move the old previous to a backup
PREVIOUS_BACKUP="$INSTALL_BASE/backup/rolled-back-$(date +%s)"
mv "$INSTALL_BASE/previous" "$PREVIOUS_BACKUP" 2>/dev/null || true

echo "[rollback] Complete. Current is now $PREVIOUS"
