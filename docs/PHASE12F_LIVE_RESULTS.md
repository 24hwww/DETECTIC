# PHASE12F_LIVE_RESULTS

## EXECUTIVE SUMMARY

**Phase 12F Status: OFFLINE-READY — All live validation BLOCKED**

The EX520 router is NOT reachable from the development machine. No live tests have been executed. All results below are based on offline analysis with honest evidence classification.

**No live results exist. No live results are claimed.**

---

## 12F.0 — LIVE ACCESS GATE

**Status: BLOCKED**

The development machine (192.168.0.27/24) cannot reach the EX520. The router is on a different physical network. Physical access or network bridge is required.

---

## 12F.1 — HARD SAFETY GATE

**Status: UNKNOWN (offline analysis only)**

All system information is from previous offline analysis of extracted firmware, not from live `uname`, `ps`, `df` commands.

---

## 12F.2 — PRISTINE BACKUP SAFETY

**Status: PARTIALLY PROVEN-OFFLINE**

- Backup format understood: YES
- Key derivation understood: YES (but 32-bit DeviceInfo value unknown)
- Restore procedure: NEVER TESTED on live router
- Pristine backup from live device: NOT EXPORTED

---

## 12F.3 — UART/RECOVERY GATE

**Status: UNKNOWN**

No UART pins identified. No recovery procedure demonstrated.

---

## 12F.4 — REAL MISC_RW DISCOVERY

**Status: UNKNOWN**

- misc_rw existence: PROVEN-OFFLINE
- misc_rw persistence: PROVEN-OFFLINE
- Actual capacity: UNKNOWN
- Actual free space: UNKNOWN
- Can store binary: UNKNOWN

---

## 12F.5 — SAFE WRITE/PERSISTENCE PROBE

**Status: BLOCKED**

No markers created. No persistence tested.

---

## 12F.6 — REAL ARM64 EXECUTION PROBE

**Status: UNKNOWN**

- Binary architecture: PROVEN-OFFLINE
- Binary static: PROVEN-OFFLINE
- Binary executes on real hardware: UNKNOWN
- Transfer mechanism: UNKNOWN

---

## 12F.7 — REAL TELNET VALIDATION

**Status: UNKNOWN**

- Telnet binary in firmware: PROVEN-OFFLINE
- Telnet enabled on live router: UNKNOWN
- Telnet persists reboot: UNKNOWN
- Telnet credentials: UNKNOWN

---

## 12F.8 — TELNET PERSISTENCE

**Status: BLOCKED**

Depends on 12F.7.

---

## 12F.9 — MANAGEMENT TRANSPORT

**Status: UNKNOWN**

No management transport validated on live hardware.

---

## 12F.10 — DETECTIC ARTIFACT VALIDATION

**Status: PROVEN-OFFLINE (binary properties), UNKNOWN (live execution)**

Binary is correctly built, statically linked, ARM64. But never executed on real hardware.

---

## 12F.11 — DETECTIC DEPLOYMENT

**Status: BLOCKED**

Cannot deploy without:
1. Live access
2. Transfer mechanism
3. Storage verification
4. Execution verification

---

## 12F.12 — REAL PROCESS DISCOVERY

**Status: UNKNOWN**

- `ps` availability: PROVEN-OFFLINE
- Process inspection methods on live hardware: UNKNOWN

---

## 12F.13 — REAL HEALTH MODEL

**Status: UNKNOWN**

Health model designed but never validated on live hardware.

---

## 12F.14 — REAL RESOURCE BASELINE

**Status: UNKNOWN**

No actual CPU/RSS measurements exist.

---

## 12F.15 — REAL OFFLINE QUEUE VALIDATION

**Status: UNKNOWN**

- Default spool in /tmp: PROVEN-FROM-SOURCE
- Spool persistence: VOLATILE (lost on reboot)
- Queue bounded: PROVEN-FROM-SOURCE
- Queue recovery after reboot: NO (data lost)

---

## 12F.16 — REAL REBOOT RECOVERY

**Status: SIMULATED (not tested live)**

---

## 12F.17 — REAL CRASH RECOVERY

**Status: SIMULATED (not tested live)**

---

## 12F.18 — CONTROLLER RESTART RECOVERY

**Status: SIMULATED (not tested live)**

---

## 12F.19 — UPDATE/ROLLBACK LIVE TEST

**Status: BLOCKED**

---

## 12F.20 — NETWORK FAILURE

**Status: SIMULATED (not tested live)**

---

## 12F.21 — STORAGE EXHAUSTION SAFETY

**Status: SIMULATED (not tested live)**

---

## 12F.22 — POWER-LOSS / HARD FAILURE MODEL

**Status: SIMULATED (not tested live)**

---

## 12F.23 — SECURITY LIVE AUDIT

**Status: PARTIALLY PROVEN-OFFLINE**

Security controls designed and tested in simulation. Not validated on live hardware.

---

## 12F.24 — SERVICE CONTINUITY AUDIT

**Status: BLOCKED**

Cannot test without live router access.

---

## 12F.25 — FINAL EVIDENCE CLASSIFICATION

### Complete evidence matrix:

| Mechanism | Evidence | Status | Risk | Recovery |
|-----------|----------|--------|------|----------|
| misc_rw persistence | Code analysis | PROVEN-OFFLINE | LOW | N/A |
| Binary architecture | ELF analysis | PROVEN-OFFLINE | NONE | N/A |
| Binary static linking | ELF analysis | PROVEN-OFFLINE | NONE | N/A |
| CLI arguments | Source code | PROVEN-FROM-SOURCE | NONE | N/A |
| Signal handling | Source code | PROVEN-FROM-SOURCE | NONE | N/A |
| Health model | Source code | PROVEN-FROM-SOURCE | NONE | N/A |
| Spool in /tmp | Source code | PROVEN-FROM-SOURCE | MEDIUM | Data loss on reboot |
| Backup format | Reverse engineering | PROVEN-OFFLINE | LOW | N/A |
| Key derivation | Reverse engineering | PROVEN-OFFLINE | LOW | N/A |
| Telnet enablement | Config analysis | PROVEN-OFFLINE | MEDIUM | Backup restore |
| Controller design | Implementation | PROVEN-OFFLINE | LOW | Rollback |
| Simulator tests | Test suite | SIMULATED | LOW | N/A |
| Live access | Network scan | BLOCKED | HIGH | Physical access |
| misc_rw capacity | — | UNKNOWN | HIGH | N/A |
| Binary execution | — | UNKNOWN | HIGH | N/A |
| Transfer mechanism | — | UNKNOWN | HIGH | N/A |
| Telnet on live router | — | UNKNOWN | HIGH | N/A |
| Telnet persistence | — | UNKNOWN | HIGH | N/A |
| Process discovery | — | UNKNOWN | MEDIUM | N/A |
| Actual RSS/CPU | — | UNKNOWN | MEDIUM | N/A |
| Reboot recovery | — | UNKNOWN | HIGH | N/A |
| Crash recovery | — | UNKNOWN | MEDIUM | N/A |
| UART recovery | — | UNKNOWN | HIGH | Physical |

### Status summary:

- **PROVEN-LIVE**: 0 items
- **PROVEN-OFFLINE**: 10 items
- **PROVEN-FROM-SOURCE**: 7 items
- **SIMULATED**: 5 items
- **UNKNOWN**: 12 items
- **BLOCKED**: 3 items
- **FAILED**: 0 items

---

## 12F.26 — ARCHITECTURE DECISION

### Cannot determine CASE A-G without live evidence.

The following cases remain possible:

- **CASE A** (PRIMARY ARCHITECTURE PROVEN): Requires ALL of:
  - misc_rw persistent: UNKNOWN
  - ARM64 execution works: UNKNOWN
  - Real transfer works: UNKNOWN
  - Management transport works: UNKNOWN
  - Detectic runs correctly: UNKNOWN
  - Controller recovers: UNKNOWN
  - Security gate passes: PARTIALLY

- **CASE B** (Telnet works, transfer poor): UNKNOWN
- **CASE C** (misc_rw persists, execution fails): UNKNOWN
- **CASE D** (Telnet cannot persist): UNKNOWN
- **CASE E** (storage insufficient): UNKNOWN
- **CASE F** (runtime incompatible): UNKNOWN
- **CASE G** (controller unreliable): UNKNOWN

---

## 12F.27 — AUTOMATIC FUTURE LOOPS

Since Phase 12F cannot complete, the following future loops are NOT YET triggered:

- 12G through 12Q are NOT triggered because we don't know which case applies.

When live access is obtained, Phase 12F should be re-executed. Based on results, the appropriate loop will be triggered.

---

## CRITICAL FINDINGS FROM OFFLINE AUDIT

### 1. Spool is VOLATILE

The default spool path is `/tmp/detectic_buffer.jsonl`. This means:
- Offline buffering does NOT survive reboot
- Any unsent events are lost on power cycle
- This is a design limitation, not a bug

**Mitigation options:**
- Move spool to `/var/run/misc/misc_rw/detectic/spool/` (requires config change)
- Accept data loss on reboot (acceptable for MVP)
- Drain spool before planned reboots

### 2. No --daemon flag

Detectic runs in foreground. The external launcher must:
- Start Detectic in background: `nohup detectic sensor &` or similar
- NOT assume `--daemon` flag exists
- NOT assume `--log` flag exists

### 3. No heartbeat mechanism

Detectic has no heartbeat endpoint, file, or structured stdout output. Health must be determined by:
- Process existence (ps)
- VmRSS from /proc/self/status
- Uptime from /proc/self/stat + /proc/uptime
- Successful backend communication (if backend configured)

### 4. pidof/pgrep not used by code

The controller should use `ps` to find Detectic, not `pidof` or `pgrep`.

### 5. SCP not used by code

The Detectic binary uses HTTP (ureq) for all network communication. It does not use SCP, SFTP, or Telnet for file transfer. File transfer must be handled by the external controller.

### 6. Telnet ≠ SCP

Enabling Telnet does NOT enable SCP. File transfer over Telnet requires separate mechanisms (base64 encoding, HTTP upload, or physical media).

---

## NEXT STEPS

1. **Obtain live access to EX520** (physical LAN connection or network bridge)
2. **Execute 12F.0**: confirm router IP and management availability
3. **Execute 12F.1**: collect live baseline (uname, df, mount, ps)
4. **Execute 12F.4**: measure misc_rw actual capacity
5. **Execute 12F.6**: test binary execution
6. **Execute 12F.7**: validate Telnet/SSH availability
7. Based on results, continue to appropriate future loop
