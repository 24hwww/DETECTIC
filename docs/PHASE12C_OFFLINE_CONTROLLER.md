# PHASE 12C — OFFLINE CONTROLLER DESIGN

## 12C.0 CHECK LIVE ACCESS
Si no hay acceso live, continuar en modo OFFLINE-READY.
El controller debe estar preparado para activarse cuando EX520 sea accesible.

## 12C.1 AUDIT CONTROLLER

Componentes del controller:
- DISCOVER: ping / ARP scan LAN
- AUTHENTICATE: Telnet login con credenciales guardadas
- CONNECT: shell sesión
- IDENTIFY ROUTER: `cat /proc/version`, `uname -a`
- VERIFY FIRMWARE: comparar build identifier
- VERIFY ARCHITECTURE: `uname -m` → aarch64
- VERIFY STORAGE: `df /var/run/misc/misc_rw`
- VERIFY Detectic: `ls /var/run/misc/misc_rw/detectic/detectic`
- VERIFY HEALTH: `ps | grep detectic`

## 12C.2 DEPLOYMENT PROTOCOL

Secuencia segura:
1. version negotiation
2. manifest validation JSON
3. SHA-256 validation
4. architecture validation aarch64
5. free-space validation ≥ required
6. atomic upload a `/var/run/misc/misc_rw/detectic/detectic.new`
7. atomic rename `mv detectic.new detectic`
8. post-deploy verification checksum

## 12C.3 PROCESS SUPERVISION

Métricas:
- PID discovery: `pgrep detectic`
- process existence: bool
- health endpoint: Detectic envía heartbeat a stdout
- heartbeat timeout: 60s
- CPU threshold: >80% durante 5 min → alerta
- RAM threshold: >32 MB → alerta
- restart counter: máx 5 en 10 min
- exponential backoff: 1s, 2s, 4s, 8s, 16s

## 12C.4 REBOOT STATE MACHINE

Estados:
ONLINE → CONNECTION_LOST → BOOTING → MANAGEMENT_AVAILABLE → STORAGE_AVAILABLE → BINARY_VERIFIED → DETECTIC_STARTED → HEALTHY

Transiciones por eventos: ping restaura, Telnet responde, binary existe, proceso corre.

## 12C.5 FAILURE HANDLING

Casos:
- connection timeout → retry backoff
- authentication failure → alert, no retry
- checksum mismatch → rollback
- binary missing → deploy
- insufficient storage → alert, no deploy
- process crash → restart
- process hang → kill -9 + restart
- backend unavailable → queue local
- repeated reboot → alert, pausa reintentos

## 12C.6 ROLLBACK

Mantener 2 versiones:
`/var/run/misc/misc_rw/detectic/detectic.v1`
`/var/run/misc/misc_rw/detectic/detectic.v2`
Symlink `detectic` → versión activa.
Rollback: cambiar symlink a versión anterior, verify checksum, restart.

## 12C.7 OFFLINE QUEUE

Detectic escribe a:
`/var/run/misc/misc_rw/detectic/queue/`
- bounded queue: máx 5 MB
- oldest-first eviction
- atomic writes con tempfile + rename
- corrupción detection vía checksum por lote
- recovery after power loss: replay archivo completo
- backend reconnect → flush queue

## 12C.8 UPDATE SYSTEM

Flujo:
controller detecta nueva versión → download → verify checksum → verify arch → upload → preserve previous → atomic switch → start → healthcheck → si FAIL rollback automático

## 12C.9 ROUTER SAFETY

Límites:
- CPU ceiling Detectic: 10% avg
- RAM ceiling: 32 MB
- bandwidth ceiling: 1 kB/s upstream
- storage ceiling: 10 MB total
- NO firewall/routing/DHCP/DNS/WLAN modification

## 12C.10 SECURITY

- controller authentication: cert + password
- LAN-only management: bind 192.168.0.0/16
- no WAN management
- binary integrity SHA-256
- command allowlist en controller → solo comandos seguros
- audit events a log central
- credential rotation cada 90 días

## 12C.11 OBSERVABILITY

Métricas expuestas:
- deployment status
- Detectic version
- router firmware
- process status
- uptime
- memory/CPU
- queue depth
- last heartbeat
- last recovery

## 12C.12 TEST MATRIX

Casos a probar:
fresh deployment, duplicate deployment, interrupted upload, corrupted binary, corrupted manifest, insufficient disk, process crash, process hang, router reboot, controller reboot, network loss, backend loss, upgrade failure

## 12C.13 LIVE GATE

Si EX520 accesible → ejecutar PHASE 12B
Si no → permanecer OFFLINE-READY

## 12C.14 RESULT

ALL PASS → PHASE 13
STORAGE FAIL → optimizar modelo
TELNET FAIL → alternativa management
EXECUTION FAIL → compatibilidad runtime
DETECTIC FAIL → debug sensor

Estado: controller diseñado offline, listo para activación live.
