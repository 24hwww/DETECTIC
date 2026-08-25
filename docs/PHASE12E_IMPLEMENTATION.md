# PHASE 12E — CONTROLLER IMPLEMENTATION SPEC

## 12E.0 READ ALL EVIDENCE
Evidence: 12A inventory, 12B live plan, 12C controller design, 12D consistency audit.
No new firmware discovery.

## 12E.1 DEFINE CONTROLLER CONTRACT

Tipos:
- RouterIdentity { firmware, build, arch, mac, ip }
- ManagementTransport { connect, authenticate, execute, upload, download, disconnect }
- RouterFilesystem { path, capacity, free }
- DetecticArtifact { version, sha256, arch, size, manifest }
- DeploymentState { desired, active, previous, state }
- HealthState { DEAD, HUNG, STARTING, HEALTHY, DEGRADED }
- RecoveryPolicy { backoff, max_restarts }
- Metrics { cpu, ram, storage, queue }

## 12E.2 MANAGEMENT TRANSPORT

Interface abstracta:
- Telnet adapter: login, execute command, upload via scp-like
- SSH adapter placeholder
- Timeout 30s, reconnect 3 intentos
- Authentication failure → alert, no retry

No exponer raw shell API.

## 12E.3 COMMAND ALLOWLIST

Categorías:
discovery: df, mount, ps, cat /proc/version, cat /proc/mtd
verification: sha256sum, ls, stat
deployment: mkdir, mv, chmod, cp
process: pidof, kill
recovery: sync

Rechazar comandos desconocidos, argumentos no seguros, paths fuera `/var/run/misc/misc_rw/detectic`.

## 12E.4 ARTIFACT MANAGER

Validar manifest:
- version semver
- sha256 match
- arch aarch64
- size < 10 MB
- compatibilidad firmware

## 12E.5 DEPLOYMENT TRANSACTION

DISCOVER → PRECHECK → TRANSFER .new → VERIFY → SYNC → SWITCH → START → HEALTHCHECK → COMMIT / ROLLBACK

## 12E.6 VERSION MANAGEMENT

Archivos:
detectic.current
detectic.previous
detectic.new

Reglas: nunca destruir last-known-good, nunca activar binario no verificado, cleanup solo tras healthcheck OK.

## 12E.7 PROCESS SUPERVISOR

Descubrir PID via `pidof detectic`, verificar `/proc/<pid>/exe` apunta a binario esperado, detectar duplicados, detectar DEAD/HUNG, restart gradual con exponential backoff, circuit breaker tras 5 fallos.

## 12E.8 HEALTH ENGINE

Health = process alive + local health file + heartbeat.
Timeout startup 30s, heartbeat 60s.
Estados DEAD/HUNG/STARTING/HEALTHY/DEGRADED.
Stdout no es autoritativo.

## 12E.9 RESOURCE MONITOR

CPU 10%, RAM 32 MB, storage 10 MB, bandwidth 1 KB/s.
Política NORMAL/WARNING/DEGRADED/RECOVERY. No matar inmediatamente.

## 12E.10 OFFLINE QUEUE

Bounded 5 MB, atomic writes temp+rename, metadata checksum, quarantine corrupción, oldest-first eviction, replay ordering, duplicate protection.

## 12E.11 PERSISTED CONTROLLER STATE

Persistir: desired_version, active_version, previous_version, deployment_state, restart_counter, last_known_health.
Writes atómicos con checksum.

## 12E.12 SIMULATED EX520

Simulador debe emular BusyBox, filesystem misc_rw, Telnet, reboot, process table, /proc/pid/exe, storage capacity, network availability.

## 12E.13 HAPPY-PATH TEST

DISCOVER → AUTH → VERIFY → DEPLOY → START → HEALTHY

## 12E.14 FAILURE INJECTION

Tests: auth failure, router unavailable, storage insufficient, corrupt binary, wrong checksum/arch, invalid manifest, interrupted upload, reboot during/after upload, crash during switch, immediate crash, hang, backend unavailable, queue full, controller crash.

## 12E.15 ROLLBACK TEST

v1 HEALTHY → deploy v2 → v2 FAILS → rollback v1 → verify checksum → restart → v1 HEALTHY

## 12E.16 IDEMPOTENCY TEST

Repetir discovery/deployment/restart/reboot/rollback → sin duplicados, sin deployment innecesario, estado consistente.

## 12E.17 SECURITY TEST

Command injection, path traversal, malformed manifest, malicious filename, invalid checksum, credential leakage, unauthorized command.

## 12E.18 LONG-RUN SIMULATION

Rebotes repetidos, crashes, red intermitente, backend outages, crecimiento queue, ciclos update/recovery.

## 12E.19 EVIDENCE GENERATION

Documentos a generar:
- PHASE12E_IMPLEMENTATION.md
- PHASE12E_TEST_MATRIX.md
- PHASE12E_FAILURE_RESULTS.md
- PHASE12E_SECURITY_RESULTS.md
- PHASE12E_READINESS.md

## 12E.20 FINAL GATE

ALL simulated tests PASS, rollback PASS, idempotency PASS, security PASS, long-run PASS, no HIGH-risk unresolved.

Estado: especificación completa, lista para implementación simulada.

Próximo: implementar simulador y ejecutar tests.
