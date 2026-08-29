#!/bin/sh
# Read the sensor environment and POST it back to the package server (read-only).
set -e

CALLBACK="http://192.168.0.27:8080/sensor_log?tag=read_env"
SPOOL="/tmp/read_env_spool.txt"

: > "$SPOOL"

# Redact secret VALUES **and** sensitive KEY NAMES. Sensitive keys (password /
# secret / token / smtp user) are shown only as the marker "[secret-key]"; their
# VALUES are NEVER logged. This script must not leak detectic.env to logs.
for path in /var/tmp/detectic/detectic.env /var/run/misc/misc_rw/detectic/detectic.env /tmp/detectic/detectic.env /var/run/misc/misc_rw/detectic/state/detectic.env; do
    if [ -r "$path" ]; then
        echo "--- $path (env keys; secret values negated) ---" >> "$SPOOL"
        grep -E '^[A-Za-z0-9_]+=' "$path" 2>/dev/null | \
            cut -d'=' -f1 | \
            sed -e 's/^DETECTIC_PASSWORD$/secret-key/' \
                -e 's/^DETECTIC_SECRET$/secret-key/' \
                -e 's/^DETECTIC_BACKEND_TOKEN$/secret-key/' \
                -e 's/^DETECTIC_SMTP_PASSWORD$/secret-key/' \
                -e 's/^DETECTIC_SMTP_USER$/secret-key/' \
                -e 's/^DETECTIC_D1_SYNC_URL$/secret-key/' \
                -e 's/^PASSWORD$/secret-key/' \
                -e 's/^SECRET$/secret-key/' >> "$SPOOL"
        echo "" >> "$SPOOL"
    fi
done

if [ -s "$SPOOL" ]; then
    if command -v curl >/dev/null 2>&1; then
        curl -sS -X POST --data-binary "@$SPOOL" "$CALLBACK" >/dev/null 2>&1 || true
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O - --post-file="$SPOOL" "$CALLBACK" >/dev/null 2>&1 || true
    fi
fi
