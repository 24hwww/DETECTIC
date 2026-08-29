#!/bin/bash
# start_watchdog.sh — starts the host-side Edge Supervisor as a daemon
cd "$(dirname "$0")"
cd /home/soporte24hwww/Documentos/Repositorios/detectic
set -a; . .env; set +a
export DETECTIC_BIN="/home/soporte24hwww/Documentos/Repositorios/detectic/target/release/detectic"
cd deploy/ex520_package
rm -f watchdog.pid
exec python3 -u watchdog.py
