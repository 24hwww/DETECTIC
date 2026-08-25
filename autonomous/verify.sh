#!/usr/bin/env bash
# External verification view — authoritative evidence, independent of email.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(dirname "$HERE")"
cd "${REPO}" || exit 1
exec python3 "${HERE}/collector.py" verify "$@"
