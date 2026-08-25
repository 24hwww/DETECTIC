# MATRIZ FINAL — Detectic Architecture

## Last updated: Phase 12F (Offline Audit Complete)

---

## Architecture status: OFFLINE-READY

All offline validation complete. Live validation BLOCKED by lack of physical access to EX520.

---

## Selected Architecture: External Launcher + misc_rw + Telnet

**Primary:**
- Binary persistent in `/var/run/misc/misc_rw/detectic/`
- External controller manages lifecycle
- Telnet (or SSH if available) as management transport
- HTTP for sensor data (GTPR API + backend)

**Fallback:**
- Manual SSH via UART
- Physical access for recovery

---

## Evidence Matrix:

| Component | Evidence | Status |
|-----------|----------|--------|
| **Binary** | | |
| Architecture aarch64 | ELF analysis | PROVEN-OFFLINE |
| Static linking | ELF analysis | PROVEN-OFFLINE |
| Size ~1.2 MB | File stats | PROVEN-OFFLINE |
| No C deps | ELF analysis | PROVEN-OFFLINE |
| Executes on EX520 | — | UNKNOWN |
| **Storage** | | |
| misc_rw exists | Code analysis | PROVEN-OFFLINE |
| misc_rw persists | Code analysis | PROVEN-OFFLINE |
| misc_rw capacity | — | UNKNOWN |
| Can store binary | — | UNKNOWN |
| **Management** | | |
| Telnet in firmware | Rootfs analysis | PROVEN-OFFLINE |
| Telnet enabled | — | UNKNOWN |
| Telnet persists | — | UNKNOWN |
| SSH available | — | UNKNOWN |
| Transfer mechanism | — | UNKNOWN |
| **Runtime** | | |
| CLI arguments | Source code | PROVEN-FROM-SOURCE |
| Signal handling | Source code | PROVEN-FROM-SOURCE |
| Health model | Source code | PROVEN-FROM-SOURCE |
| Spool volatile | Source code | PROVEN-FROM-SOURCE |
| No --daemon | Source code | PROVEN-FROM-SOURCE |
| No heartbeat | Source code | PROVEN-FROM-SOURCE |
| **Controller** | | |
| State machine | Implementation | PROVEN-OFFLINE |
| Deployment transaction | Implementation | PROVEN-OFFLINE |
| Rollback | Implementation | PROVEN-OFFLINE |
| Simulator tests | Test suite | SIMULATED |
| Live tests | — | BLOCKED |
| **Backup** | | |
| Format understood | Reverse engineering | PROVEN-OFFLINE |
| Key derivation | Reverse engineering | PROVEN-OFFLINE |
| Restore works | — | UNKNOWN |
| **Recovery** | | |
| Reboot recovery | — | UNKNOWN |
| Crash recovery | — | SIMULATED |
| Controller recovery | — | SIMULATED |
| UART recovery | — | UNKNOWN |

---

## Superficies investigadas (updated):

| Superficie | Writable | Ejecutable | Persiste reboot | Requiere firmware mod | Estado |
|------------|----------|------------|-----------------|----------------------|--------|
| /var/run/misc/misc_rw | Sí | Sí (UBIFS) | Sí | No | PROVEN-OFFLINE (existencia), UNKNOWN (capacidad) |
| /var/run/runtime_data | Sí (no habilitado) | Sí | Sí | No | PROVEN-OFFLINE (no habilitado en este build) |
| /var/tmp | Sí | Sí | No | No | PROVEN-OFFLINE |
| /etc/init.d | No | Sí | Sí | Sí | PROVEN-OFFLINE |
| Hotplug scripts | No | Sí | Sí | Sí | PROVEN-OFFLINE |
| Config backup | Sí | Indirecto | Sí | No | PROVEN-OFFLINE |
| Telnet/SSH via config | No | Habilita daemon | Sí | No | PROVEN-OFFLINE (mecanismo), UNKNOWN (live) |

---

## Open questions (require live evidence):

1. What is the actual misc_rw capacity on the EX520?
2. Can the binary execute on the real EX520?
3. Is Telnet/SSH already available?
4. What is the viable file transfer mechanism?
5. Does the binary survive reboot in misc_rw?
6. What are the actual CPU/RSS measurements?
7. Can the controller reliably reconnect after reboot?
8. Is UART recovery available?

---

## Decision:

**OFFLINE-READY**

The architecture is sound based on offline analysis. All components are implemented and tested in simulation. The binary is correctly built. The controller handles failure cases. The evidence is honestly classified.

**Cannot become PROVEN until live validation on physical EX520 hardware.**

---

## Next:

When physical access to EX520 is available:
1. Execute Phase 12F.0 (Live Access Gate)
2. Complete Phase 12F validation
3. Based on results, trigger appropriate future loop (12G-12Q)
