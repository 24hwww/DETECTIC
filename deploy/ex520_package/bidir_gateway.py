#!/usr/bin/env python3
"""EX520 Bidirectional Gateway — Comando directo vía phoenix.sh + HTTP.

Envía comandos shell al EX520 usando el mecanismo `phoenix.sh` ya probado.
El router ejecuta comandos y retorna resultados por HTTP.

Uso:
    # Iniciar el gateway (escucha en puerto 8082)
    python3 bidir_gateway.py

    # Desde otro terminal:
    curl http://localhost:8082/exec?cmd=uname+-a
    curl http://localhost:8082/exec?cmd=ps
    curl http://localhost:8082/exec?cmd=ls+/var/run/misc/misc_rw

    # Ver historial
    curl http://localhost:8082/history
"""

import os
import sys
import time
import json
import hashlib
import secrets
import subprocess
import threading
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse, parse_qs, unquote

# --- Configuración ---
HOST = os.environ.get("GATEWAY_HOST", "192.168.0.27")
PORT = int(os.environ.get("GATEWAY_PORT", "8082"))
EX520_URL = os.environ.get("EX520_URL", "http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]")
EX520_USER = os.environ.get("EX520_USER", "user")
DETECTIC_PASSWORD = os.environ.get("DETECTIC_PASSWORD", "")
DETECTIC_BIN = os.environ.get("DETECTIC_BIN", "detectic")
COMMAND_TIMEOUT = int(os.environ.get("COMMAND_TIMEOUT", "30"))
MAX_HISTORY = 100

# --- Estado ---
history = []
history_lock = threading.Lock()
pending_commands = {}  # cmd_id -> {"cmd": ..., "result": ..., "status": "pending|done"}
pending_lock = threading.Lock()


def log(msg):
    ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    print(f"{ts} {msg}", flush=True)


def gtpr_set(oid, data):
    """Ejecutar GTPR set via detectic CLI."""
    env = {**os.environ, "DETECTIC_PASSWORD": DETECTIC_PASSWORD}
    try:
        ret = subprocess.run(
            [DETECTIC_BIN, "--url", EX520_URL, "--user", EX520_USER, "set", oid, data],
            env=env, capture_output=True, text=True, timeout=30
        )
        return ret.returncode == 0, ret.stdout + ret.stderr
    except Exception as e:
        return False, str(e)


def gtpr_query(oid):
    """Ejecutar GTPR query via detectic CLI."""
    env = {**os.environ, "DETECTIC_PASSWORD": DETECTIC_PASSWORD}
    try:
        ret = subprocess.run(
            [DETECTIC_BIN, "--url", EX520_URL, "--user", EX520_USER, "query", oid],
            env=env, capture_output=True, text=True, timeout=20
        )
        return ret.stdout.strip() if ret.returncode == 0 else None
    except Exception:
        return None


def send_command_via_phoenix(command, timeout=COMMAND_TIMEOUT):
    """Enviar un comando al router via phoenix.sh + HTTP callback.
    
    Flujo:
    1. Genera ID único para el comando
    2. Crea script que ejecuta el comando y envía resultado por HTTP
    3. Usa GTPR para activar phoenix.sh con URL del script
    4. El router descarga y ejecuta el script
    5. El script envía resultado a nuestro servidor HTTP
    """
    cmd_id = secrets.token_hex(8)
    
    # Script que el router ejecutará
    # Usa wget (BusyBox) para enviar resultado por HTTP POST
    escaped_cmd = command.replace("'", "'\\''")
    script = f"""#!/bin/sh
# Detectic bidirectional command agent
CMD_ID="{cmd_id}"
RESULT_URL="http://{HOST}:{PORT}/result/$CMD_ID"
LOG="/var/tmp/bidir_cmd_{{CMD_ID}}.log"

echo "[$(date)] Executing: {escaped_cmd}" > "$LOG" 2>&1

# Execute command and capture output
OUTPUT=$(eval '{escaped_cmd}' 2>&1)
RC=$?

# Send result back to host
RESULT_SIZE=$(echo "$OUTPUT" | wc -c)
wget -q -T {timeout} -O /dev/null --post-data="id=$CMD_ID&rc=$RC&output=$(echo $OUTPUT | head -c 8192 | sed 's/&/\\&/g')" "$RESULT_URL" 2>/dev/null

# Also try curl if wget fails
curl -s -m {timeout} -X POST -d "id=$CMD_ID&rc=$RC" -d "output=$OUTPUT" "$RESULT_URL" 2>/dev/null

echo "[$(date)] Done rc=$RC size=$RESULT_SIZE" >> "$LOG"
rm -f "$LOG"
"""
    
    # Write script to a temp file for phoenix to download
    script_url = f"http://{HOST}:{PORT}/scripts/cmd_{cmd_id}.sh"
    
    # Store pending command
    with pending_lock:
        pending_commands[cmd_id] = {
            "cmd": command,
            "result": None,
            "status": "pending",
            "sent_at": time.time(),
        }
    
    # Write the script to a place phoenix can fetch it
    # (we serve it from our HTTP server)
    scripts_dir = os.path.join(os.path.dirname(__file__), "_scripts")
    os.makedirs(scripts_dir, exist_ok=True)
    script_path = os.path.join(scripts_dir, f"cmd_{cmd_id}.sh")
    with open(script_path, "w") as f:
        f.write(script)
    os.chmod(script_path, 0o755)
    
    # Trigger phoenix.sh via GTPR
    payload = json.dumps({
        "enable": "1",
        "URL": script_url,
        "stack": "0,0,0,0,0,0",
        "pstack": "0,0,0,0,0,0",
    })
    
    ok, output = gtpr_set("DEV2_LIFEMOTE_AGENT", payload)
    if not ok:
        with pending_lock:
            pending_commands[cmd_id]["status"] = "error"
            pending_commands[cmd_id]["result"] = f"GTPR set failed: {output}"
        return cmd_id, False
    
    log(f"Command {cmd_id} sent: {command}")
    
    # Wait for result (poll with timeout)
    start = time.time()
    while time.time() - start < timeout + 10:
        with pending_lock:
            cmd = pending_commands.get(cmd_id, {})
            if cmd.get("status") == "done":
                return cmd_id, True
        time.sleep(1)
    
    # Timeout — disable phoenix
    gtpr_set("DEV2_LIFEMOTE_AGENT", json.dumps({
        "enable": "0", "URL": "",
        "stack": "0,0,0,0,0,0", "pstack": "0,0,0,0,0,0",
    }))
    
    with pending_lock:
        pending_commands[cmd_id]["status"] = "timeout"
        pending_commands[cmd_id]["result"] = "Command timed out"
    
    return cmd_id, False


class GatewayHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        parsed = urlparse(self.path)
        path = parsed.path
        params = parse_qs(parsed.query)
        
        if path == "/":
            self._respond(200, "text/plain", 
                "EX520 Bidirectional Gateway\n"
                "Commands:\n"
                "  GET /exec?cmd=<command>  — Execute command on router\n"
                "  GET /status/<cmd_id>     — Check command status\n"
                "  GET /history             — Command history\n"
                "  GET /health              — Gateway health\n"
                "  GET /test                — Test connectivity\n"
            )
        
        elif path == "/exec":
            cmd = params.get("cmd", [""])[0]
            if not cmd:
                self._respond(400, "text/plain", "Missing ?cmd= parameter")
                return
            
            cmd = unquote(cmd)
            log(f"Execute request: {cmd}")
            cmd_id, success = send_command_via_phoenix(cmd)
            
            with pending_lock:
                result = pending_commands.get(cmd_id, {})
            
            self._respond(200, "application/json", json.dumps({
                "cmd_id": cmd_id,
                "command": cmd,
                "status": result.get("status", "unknown"),
                "result": result.get("result"),
            }, indent=2))
        
        elif path.startswith("/status/"):
            cmd_id = path.split("/")[-1]
            with pending_lock:
                result = pending_commands.get(cmd_id)
            if result:
                self._respond(200, "application/json", json.dumps(result, indent=2))
            else:
                self._respond(404, "text/plain", f"Command {cmd_id} not found")
        
        elif path == "/history":
            with history_lock:
                recent = history[-MAX_HISTORY:]
            self._respond(200, "application/json", json.dumps(recent, indent=2))
        
        elif path == "/health":
            self._respond(200, "application/json", json.dumps({
                "status": "ok",
                "host": HOST,
                "port": PORT,
                "uptime": time.time(),
                "pending_commands": len(pending_commands),
            }))
        
        elif path == "/test":
            # Test GTPR connectivity
            result = gtpr_query("DEV2_WIFI_APDEV_ASSOCDEV")
            if result:
                self._respond(200, "text/plain", f"GTPR OK\n{result[:500]}")
            else:
                self._respond(500, "text/plain", "GTPR unreachable")
        
        # Serve scripts for phoenix
        elif path.startswith("/scripts/"):
            script_name = path.split("/")[-1]
            script_path = os.path.join(
                os.path.dirname(__file__), "_scripts", script_name
            )
            if os.path.exists(script_path):
                with open(script_path, "rb") as f:
                    content = f.read()
                self.send_response(200)
                self.send_header("Content-Type", "application/x-sh")
                self.send_header("Content-Length", str(len(content)))
                self.end_headers()
                self.wfile.write(content)
                # Clean up served script
                os.remove(script_path)
            else:
                self._respond(404, "text/plain", "Script not found")
        
        else:
            self._respond(404, "text/plain", "Not found")
    
    def do_POST(self):
        parsed = urlparse(self.path)
        path = parsed.path
        
        if path.startswith("/result/"):
            cmd_id = path.split("/")[-1]
            length = int(self.headers.get("Content-Length", 0))
            body = self.rfile.read(length).decode("utf-8", "replace")
            params = parse_qs(body)
            
            rc = params.get("rc", ["?"])[0]
            output = params.get("output", [""])[0]
            
            with pending_lock:
                if cmd_id in pending_commands:
                    pending_commands[cmd_id]["result"] = f"rc={rc}\n{output}"
                    pending_commands[cmd_id]["status"] = "done"
                    pending_commands[cmd_id]["completed_at"] = time.time()
            
            with history_lock:
                history.append({
                    "cmd_id": cmd_id,
                    "command": pending_commands.get(cmd_id, {}).get("cmd", "?"),
                    "rc": rc,
                    "output_preview": output[:500],
                    "received_at": time.time(),
                })
                if len(history) > MAX_HISTORY:
                    history.pop(0)
            
            log(f"Result received for {cmd_id}: rc={rc} size={len(output)}")
            self._respond(200, "text/plain", "ok")
        
        else:
            self._respond(404, "text/plain", "Not found")
    
    def _respond(self, code, content_type, body):
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        self.end_headers()
        self.wfile.write(body.encode("utf-8"))
    
    def log_message(self, fmt, *args):
        log(fmt % args)


def main():
    log(f"EX520 Bidirectional Gateway starting on {HOST}:{PORT}")
    log(f"Router: {EX520_URL}")
    
    # Test connectivity first
    result = gtpr_query("DEV2_WIFI_APDEV_ASSOCDEV")
    if result:
        log("GTPR connectivity: OK")
    else:
        log("WARNING: GTPR connectivity failed — commands will fail")
    
    server = HTTPServer((HOST, PORT), GatewayHandler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        log("Shutting down")
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
