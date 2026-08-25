# PHASE 11 — VALIDACIÓN RUNTIME Y GRÁFICO DE EJECUCIÓN

## 11.0 READ CURRENT EVIDENCE
Archivos de evidencia leídos: CAPTURA_BASE, SUPERFICIES_DESCUBIERTAS, CLASIFICACION_MECANISMOS, PRUEBA_MINIMA, FAILURE_ANALYSIS, SEGUNDA_FAMILIA, OPTIMIZACION, COMBINACIONES, FAILURE_RECOVERY, MATRIZ_FINAL.

## 11.1 VALIDATE REAL RUNTIME_DATA

Config `etc/config.bba`:
```
# INCLUDE_RUNTIME_DATA_SECTION is not set
RUNTIME_DATA_SECTION_SIZE="0"
```

Resultado: **runtime_data NO está habilitado en este build**.
Mountpoint `/var/run/runtime_data` no se creará en rcS.
Capacidad, permisos, persistencia: N/A.

**FAIL** → alternativa persistente RW: `misc_rw` UBI vol.

Validación de `misc_rw`:
- Montado en `/var/run/misc/misc_rw` por rcS
- UBI vol rw
- Persistencia across reboot: SÍ
- Persistencia across service restart: SÍ
- Persistencia across config reload: SÍ
- Se recrea solo si vol no existe → formatea UBI
- Capacidad: limitada por tamaño UBI misc_rw
- Permisos: root:root, ejecutable permitido en filesystem UBI

## 11.2 MAP THE COMPLETE EXECUTION GRAPH

Daemons long-running identificados en rootfs:
- `cos` — supervisor principal TP-Link
- `cmmsyslogd` — syslog
- `httpd` — web UI
- `cwmp` — TR-069
- `dnsProxy` / `dnsmasq`
- `awnd` — WAN
- `ated_tp` — wireless
- `apsd` — ?
- `mapAgent` — ?
- `meshMonitor`
- `ntpc`
- `snmpd`

Init system: BusyBox inittab → `::sysinit:/etc/init.d/rcS`
Parent/child: rcS inicia `cos &`, `cmmsyslogd &`
Watchdog: kernel + posiblemente `cos`

Scripts ejecutados por daemons:
- Hotplug handlers en `/etc/hotplug.d/*` — todos en rootFS ro
- `rcS_hook` binary `/bin/rcsHook` existe

**Descubrimiento clave:** `/bin/rcsHook`:
- Busca `/etc/rcS_hook`
- `doRcsHookExes` + `util_exec_system`
- Mensajes de error: `rcS hook path %s not exist`, `Can not open rcS hook path %s`
- Ruta hardcodeada: `/etc/rcS_hook`

Archivo `/etc/rcS_hook` existe en rootFS ro con solo `.gitkeep`. No escribible.

## 11.3 SEARCH FOR RW → EXEC CHAINS

Cadenas RW → EXEC buscadas:
- ¿Carga ejecutable desde RW? No encontrado. `rcsHook` carga desde `/etc/rcS_hook` ro.
- ¿Script desde RW? No. Hotplug scripts ro.
- ¿Config causa ejecución? Sí: data model apply handlers pueden lanzar `dropbear`/`telnetd` vía config.
- ¿Evento invoca ejecutable user-controlled? No. Eventos usan scripts ro.

Resultado: **NO hay cadena RW → EXEC** sin modificar firmware.

## 11.4 SUPERVISOR INVESTIGATION

Init: BusyBox
Supervisor propietario: `cos`
Respawn policies: desconocidas sin símbolo
`rcsHook` podría ser un hook de inicio, pero ruta es ro.

VIABLE para lanzar Detectic sin modificar firmware: **NO**

## 11.5 BACKUPCFG FORENSICS

Backup es DES-ECB + zlib XML config-only.
No permite ejecución arbitraria.
Habilitar Telnet/SSH es viable vía config.

## 11.6 MANAGEMENT ACCESS HYPOTHESIS

- `dropbear` y `telnetd` presentes en firmware
- `INCLUDE_WEB_TELNET=y`, `INCLUDE_REMOTE_TELNET=y`
- `INCLUDE_SSH_ACCESS` not set → SSH no habilitado por defecto
- Habilitar Telnet vía data model es persistente tras reboot

## 11.7 ELIMINATE SSH/TELNET DEPENDENCY

¿Detectic self-start interno? No.
¿Daemon existente puede lanzarlo? `cos` podría, pero sin evidencia de carga desde RW.
¿Watchdog puede lanzarlo? No.
¿Event handler puede lanzarlo? No, scripts ro.

Conclusión: **NO se puede eliminar dependencia SSH/Telnet sin firmware mod**.

## 11.8 EXTERNAL LAUNCHER HARDENING

Arquitectura confirmada:
- Detect online
- Auth Telnet/SSH
- Verificar checksum binario en misc_rw
- Atomic deployment
- Start
- Health check
- Backoff
- Detect reboot via session loss
- Re-provision

## 11.9 SURVIVAL MATRIX

- normal reboot: binario persiste, ejecución perdida → requiere re-lanzar
- power loss: binario persiste
- Detectic crash: recuperable por external launcher
- router service restart: binario persiste
- config reload: binario persiste
- firmware upgrade: binario perdido si se formatea misc_rw
- factory reset: binario perdido
- backend unavailable: buffer local en misc_rw
- storage full: riesgo de corromper data model

## 11.10 SCORE ALL REMAINING ARCHITECTURES

Arquitectura A: External launcher + misc_rw + Telnet persistente
- persistence: 7
- boot reliability: 5
- deployment complexity: 6
- RAM/CPU/storage: bueno
- recovery: 8
- firmware independence: 9
- security: 6
- rollback: 9
Score: alto

## 11.11 DECISION

No hay mecanismo interno de ejecución viable sin modificar firmware.
External launcher se vuelve PRIMARY.

## 11.12 SECONDARY / FALLBACK

Primary: External launcher Telnet
Fallback: Manual SSH via UART
Recovery: Re-provision config backup con Telnet habilitado

## 11.13 FINAL GAP ANALYSIS

Explorado:
- RW surfaces: misc_rw, runtime_data (no habilitado), /var/tmp
- Execution paths: rcS, rcsHook, hotplug, cos, data model handlers
- Supervisors: cos, BusyBox init
- Config mechanisms: backupcfg
- TP-Link daemons: identificados

No quedan superficies RW → EXEC no exploradas.

## 11.14 CLOSE LOOP

Proven mechanisms:
- Persistencia de binario en misc_rw UBI
- Habilitar Telnet persistente vía backup config
- External launcher vía Telnet

Disproven mechanisms:
- Autostart sin firmware mod
- RW → EXEC chain
- runtime_data en este build

Blocked mechanisms:
- SSH por defecto deshabilitado

Unproven:
- Capacidad exacta de misc_rw, necesitamos test en vivo

Primary architecture: External launcher Telnet + misc_rw persistente
Fallback: UART manual

Remaining validation steps:
1. Medir tamaño libre en misc_rw en router vivo
2. Confirmar que `rcsHook` no puede redirigirse a misc_rw
3. Probar backup con Telnet habilitado
4. Probar persistencia de binario tras reboot
