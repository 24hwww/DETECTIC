# PHASE 12D — SPECIFICATION CONSISTENCY AUDIT

## 12D.0 SPECIFICATION CONSISTENCY AUDIT

Lectura de evidencia PHASE12A/B/C:
- Consistencia: fase 12A define capacidad 12 MB, fase 12B usa mismo umbral → OK
- Contradicciones: ninguna
- Asunciones presentadas como hechos:
  * `runtime_data` no existe: confirmado en config → PROVEN
  * `misc_rw` persiste reboot: PROVEN vía rcS
  * Telnet habilitable vía backup: SIMULATED, backupcfg formato conocido pero no testeado live → UNKNOWN
- Comandos no disponibles: `sha256sum` en BusyBox → presente, `df` → presente, `ubinfo` → presente
- Features filesystem: UBI ubifs → PROVEN
- Dependencias clasificación:
  PROVEN: ARM64 static binary 1.3 MB, misc_rw rw, DES backup format
  SIMULATED: Telnet persistencia, external launcher flujo
  UNKNOWN: capacidad exacta misc_rw, tiempo de arranque, estabilidad Telnet

## 12D.1 EX520 COMMAND COMPATIBILITY

BusyBox shell. Comandos verificados en rootfs:
- df → PROVEN
- mount → PROVEN
- ps → PROVEN
- kill → PROVEN
- sha256sum → PROVEN (BusyBox)
- mv → PROVEN
- mkdir → PROVEN
- chmod → PROVEN
- sync → PROVEN
- netstat → PROVEN
- /proc interfaces → PROVEN

Unknowns marcados LIVE-DEPENDENCY: `ubinfo` output format exacto, `pgrep` disponibilidad.

## 12D.2 MANAGEMENT TRANSPORT ABSTRACTION

Controller → ManagementTransport
Operaciones requeridas: connect(), authenticate(), execute(), upload(), download(), disconnect(), reconnect()

Actual implementación asumida: Telnet bootstrap. Futuro SSH.

Detectic deployment NO depende directamente de Telnet, solo de la abstracción.

## 12D.3 COMMAND SAFETY LAYER

Allowlist explícita:
discovery: df, mount, ps, cat /proc/version
deployment: mkdir, mv, chmod, sha256sum
recovery: kill, reboot

No arbitrary command strings. Path validation fijo: `/var/run/misc/misc_rw/detectic/*`. No shell interpolation.

## 12D.4 DEPLOYMENT TRANSACTION

PRECHECK → TRANSFER → VERIFY → SYNC → ATOMIC SWITCH → START → HEALTHCHECK → COMMIT / ROLLBACK

## 12D.5 VERSION STORAGE

Estructura:
- detectic.current
- detectic.previous
- detectic.new

Sin symlink, nunca sobrescribir last-known-good.

## 12D.6 PROCESS IDENTITY

Evitar `kill $(pgrep ...)`. Verificar PID, verificar ejecutable path vía `/proc/<pid>/exe`, prevenir duplicados.

## 12D.7 HEALTH MODEL

Health = process alive + local health state + heartbeat
Estados: DEAD, HUNG, STARTING, HEALTHY, DEGRADED

## 12D.8 RESOURCE POLICY

Targets: CPU 10%, RAM 32 MB, storage 10 MB, bandwidth 1 KB/s
Medición sostenida, alerta antes de matar.

## 12D.9 STORAGE MODEL

Componentes: binary, previous binary, temporary upload, queue, state, logs
Cálculo: required_minimum 12 MB, operational_target 20 MB, emergency_threshold 5 MB
Exhaustión NO debe corromper config router.

## 12D.10 OFFLINE QUEUE

Bounded, atomic writes temp+rename, integrity metadata, corrupción quarantine, oldest-first eviction.

## 12D.11 UPDATE FAILURE INJECTION

Tests: correct, duplicate, corrupt binary, wrong checksum, wrong arch, invalid manifest, interrupted transfer, reboot during transfer, crash after switch, healthcheck fail.
Expectativa: known-good sobrevive, rollback automático.

## 12D.12 NETWORK FAILURE INJECTION

Router unreachable, Telnet unavailable, auth failure, timeout, connection loss mid-upload/restart, repeated reboot → controller converge a HEALTHY.

## 12D.13 PROCESS FAILURE INJECTION

Crash inmediato, repeated crash, hang, stale PID, duplicate process → restart budget, exponential backoff, circuit breaker.

## 12D.14 CONTROLLER STATE PERSISTENCE

Persistir solo: desired_version, active_version, previous_version, deployment_state, restart_counter, last_known_health. Writes atómicos.

## 12D.15 CONTROLLER CRASH RECOVERY

Crash en cualquier fase → restart converge safe.

## 12D.16 IDEMPOTENCY

Repetir discovery/auth/deployment/restart/reboot/rollback → no duplicados, no corrupción, no upload innecesario.

## 12D.17 SECURITY AUDIT

Credenciales secretas, no passwords en logs, no shell injection, path traversal prevention, manifest validation, checksum verification, management LAN-only, no WAN, controller auth, allowlist, audit trail.

## 12D.18 ROUTER SAFETY AUDIT

Detectic/controller NO debe modificar firmware, bootloader, rootFS, firewall, routing, DHCP, DNS, WLAN, WAN, repartition UBI, reboot excepto tests autorizados.

## 12D.19 SIMULATION

Simular EX520, transport, filesystem, reboot, crash, storage full, network loss, upgrade.

## 12D.20 READINESS MATRIX

PROVEN OFFLINE: formato backup, binary ARM64, misc_rw existencia
SIMULATED: flujo launcher, health model
LIVE REQUIRED: capacidad misc_rw exacta, Telnet persistencia, tiempo arranque
UNKNOWN: estabilidad Telnet largo plazo

## 12D.21 FINAL GATE

Controller implementation completo, failure injection passes, rollback passes, idempotency passes, security audit passes, router-safety audit passes, no HIGH-risk assumption sin resolver.

## 12D.22 DECISION

Sin EX520 live → OFFLINE-READY, esperar hardware.
Con EX520 live → ejecutar PHASE 12B.

Estado: auditoría completa, especificaciones coherentes, riesgos clasificados.
