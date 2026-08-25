#!/usr/bin/env python3
"""EX520 SSH Watchdog — Mantiene SSH habilitado después de cada reboot.

El problema: SSH (dropbear) se inicia vía phoenix.sh pero NO sobrevive reboot.
Solución: Este watchdog detecta cuando el router hace reboot y re-envía el
comando para iniciar dropbear.

Flujo:
  1. Monitoriza conectividad del router (ping6 + GTPR)
  2. Detecta cold boot (DOWN → UP)
  3. Espera a que phoenix.sh esté disponible
  4. Re-envía GTPR set para iniciar dropbear
  5. Verifica que SSH funcione
  6. Repite después de cada reboot

Uso:
  python3 ssh_watchdog.py
  # o con variables de entorno:
  SSH_PORT=22 python3 ssh_watchdog.py
"""

import os
import subprocess
import sys
import time
import socket
import json
from urllib.parse import urljoin

# --- Config ---
EX520_URL = os.environ.get("EX520_URL", 
    "http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]")
EX520_USER = os.environ.get("EX520_USER", "user")
DETECTIC_PASSWORD = os.environ.get("DETECTIC_PASSWORD", "")
EX520_IPV6 = os.environ.get("EX520_IPV6", "fe80::3e6a:d2ff:fe5f:abc1%enp2s0")
HOST_IP = os.environ.get("HOST_IP", "192.168.0.27")
SSH_PORT = int(os.environ.get("SSH_PORT", "22"))
SCRIPT_PORT = int(os.environ.get("SCRIPT_PORT", "8084"))
POLL_INTERVAL = int(os.environ.get("POLL_INTERVAL", "30"))
DOWN_THRESHOLD = int(os.environ.get("DOWN_THRESHOLD", "30"))
PHOENIX_GRACE = int(os.environ.get("PHOENIX_GRACE", "45"))
DETECTIC_BIN = os.environ.get("DETECTIC_BIN", "./dist/detectic-aarch64-musl")

# --- State ---
router_up = False
down_since = None
ssh_enabled_for_boot = False


def log(msg):
    ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    print(f"{ts} {msg}", flush=True)


def ping_reachable():
    try:
        ret = subprocess.run(
            ["ping6", "-c", "1", "-W", "2", EX520_IPV6],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5
        )
        return ret.returncode == 0
    except Exception:
        return False


def gtpr_query():
    try:
        env = {**os.environ, "DETECTIC_PASSWORD": DETECTIC_PASSWORD}
        ret = subprocess.run(
            [DETECTIC_BIN, "--url", EX520_URL, "--user", EX520_USER,
             "query", "DEV2_WIFI_APDEV_ASSOCDEV"],
            env=env, capture_output=True, text=True, timeout=20
        )
        return ret.returncode == 0
    except Exception:
        return False


def ssh_port_open():
    try:
        s = socket.socket(socket.AF_INET6, socket.SOCK_STREAM)
        s.settimeout(3)
        s.connect((EX520_IPV6, SSH_PORT))
        s.close()
        return True
    except Exception:
        return False


def start_dropbear_via_phoenix():
    """Iniciar dropbear vía phoenix.sh"""
    
    # Crear script de inicio de dropbear
    script_content = f"""#!/bin/sh
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

# Kill existing
killall dropbear 2>/dev/null || true
$BB sleep 1

# Generate keys if needed
$BB mkdir -p /var/tmp/dropbear 2>/dev/null
[ -f /var/tmp/dropbear/dropbear_rsa_host_key ] || \\
    /usr/bin/dropbearkey -t rsa -f /var/tmp/dropbear/dropbear_rsa_host_key 2>/dev/null
[ -f /var/tmp/dropbear/dropbear_ecdsa_host_key ] || \\
    /usr/bin/dropbearkey -t ecdsa -f /var/tmp/dropbear/dropbear_ecdsa_host_key 2>/dev/null

# Start dropbear
/usr/bin/dropbearmulti dropbear -R -p {SSH_PORT} \\
    -r /var/tmp/dropbear/dropbear_rsa_host_key \\
    -r /var/tmp/dropbear/dropbear_ecdsa_host_key 2>/dev/null &

$BB sleep 2
echo "DROPBEAR_STARTED"
"""
    
    # Guardar script localmente para servirlo
    script_dir = "/tmp/ex520_ssh_watchdog"
    os.makedirs(script_dir, exist_ok=True)
    script_path = os.path.join(script_dir, "start_dropbear.sh")
    with open(script_path, "w") as f:
        f.write(script_content)
    os.chmod(script_path, 0o755)
    
    # Iniciar servidor HTTP si no está corriendo
    _ensure_http_server(script_dir)
    
    script_url = f"http://{HOST_IP}:{SCRIPT_PORT}/start_dropbear.sh"
    
    # Enviar GTPR set
    payload = json.dumps({
        "enable": "1",
        "URL": script_url,
        "stack": "0,0,0,0,0,0",
        "pstack": "0,0,0,0,0,0",
    })
    
    try:
        env = {**os.environ, "DETECTIC_PASSWORD": DETECTIC_PASSWORD}
        ret = subprocess.run(
            [DETECTIC_BIN, "--url", EX520_URL, "--user", EX520_USER,
             "set", "DEV2_LIFEMOTE_AGENT", payload],
            env=env, capture_output=True, text=True, timeout=30
        )
        if ret.returncode == 0:
            log("GTPR trigger sent (DEV2_LIFEMOTE_AGENT)")
            return True
        else:
            log(f"GTPR trigger failed: {ret.stderr}")
            return False
    except Exception as e:
        log(f"GTPR error: {e}")
        return False


def disable_phoenix():
    """Deshabilitar phoenix.sh después de usar"""
    payload = json.dumps({
        "enable": "0",
        "URL": "",
        "stack": "0,0,0,0,0,0",
        "pstack": "0,0,0,0,0,0",
    })
    try:
        env = {**os.environ, "DETECTIC_PASSWORD": DETECTIC_PASSWORD}
        subprocess.run(
            [DETECTIC_BIN, "--url", EX520_URL, "--user", EX520_USER,
             "set", "DEV2_LIFEMOTE_AGENT", payload],
            env=env, capture_output=True, timeout=20
        )
    except Exception:
        pass


_http_server = None
_http_pid = None

def _ensure_http_server(serve_dir):
    global _http_server, _http_pid
    if _http_pid is not None:
        try:
            os.kill(_http_pid, 0)
            return  # Already running
        except OSError:
            pass
    
    # Fork a simple HTTP server
    pid = os.fork()
    if pid == 0:
        # Child process
        os.chdir(serve_dir)
        os.execvp("python3", [
            "python3", "-c", f"""
import os
from http.server import HTTPServer, SimpleHTTPRequestHandler
os.chdir('{serve_dir}')
class H(SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
    def end_headers(self):
        self.send_header('Content-Type', 'application/x-sh')
        super().end_headers()
HTTPServer(('0.0.0.0', {SCRIPT_PORT}), H).serve_forever()
"""
        ])
    else:
        _http_pid = pid
        time.sleep(1)


def main():
    global router_up, down_since, ssh_enabled_for_boot
    
    log("SSH Watchdog starting")
    log(f"Router: {EX520_IPV6} SSH port: {SSH_PORT}")
    log(f"Poll: {POLL_INTERVAL}s Down threshold: {DOWN_THRESHOLD}s")
    
    # Check initial state
    reachable = ping_reachable() or gtpr_query()
    if reachable:
        router_up = True
        down_since = None
        
        # Check if SSH is already running
        if ssh_port_open():
            ssh_enabled_for_boot = True
            log("SSH already running at startup")
        else:
            log("Router up but SSH not running — will enable")
    
    while True:
        reachable = ping_reachable() or gtpr_query()
        now = time.time()
        
        if reachable:
            if not router_up:
                # Router came back up
                if down_since is not None:
                    down_for = now - down_since
                    if down_for >= DOWN_THRESHOLD:
                        log(f"COLD BOOT detected (down for {int(down_for)}s)")
                        ssh_enabled_for_boot = False  # Reset for new boot
                    else:
                        log("Brief blip, not a cold boot")
                router_up = True
                down_since = None
            
            # If SSH not enabled for this boot, try to enable it
            if not ssh_enabled_for_boot:
                log(f"Waiting {PHOENIX_GRACE}s for phoenix...")
                time.sleep(PHOENIX_GRACE)
                
                if start_dropbear_via_phoenix():
                    # Wait for dropbear to start
                    for i in range(30):
                        time.sleep(1)
                        if ssh_port_open():
                            log(f"SSH port {SSH_PORT} is OPEN!")
                            ssh_enabled_for_boot = True
                            
                            # Disable phoenix (cleanup)
                            disable_phoenix()
                            log("Phoenix disabled, SSH running independently")
                            break
                    else:
                        log("SSH port did not open within 30s")
                else:
                    log("Failed to trigger phoenix")
        
        else:
            if router_up:
                router_up = False
                down_since = now
                ssh_enabled_for_boot = False
                log("Router went DOWN")
            elif down_since is not None:
                down_for = int(now - down_since)
                if down_for >= DOWN_THRESHOLD and down_for % 60 == 0:
                    log(f"Router down for {down_for}s, armed for re-trigger")
        
        time.sleep(POLL_INTERVAL)


if __name__ == "__main__":
    main()
