# PHASE12F_PROCESS_MODEL

## 12F.12 REAL PROCESS DISCOVERY — OFFLINE

### Process inspection methods:

| Method | Availability on EX520 | Used by Detectic |
|--------|----------------------|------------------|
| `ps` | PROVEN-OFFLINE (BusyBox) | NO (external only) |
| `pidof` | UNKNOWN | NO |
| `pgrep` | UNKNOWN | NO |
| `/proc/<pid>/exe` | UNKNOWN | NO |
| `/proc/<pid>/status` | PROVEN-FROM-SOURCE | YES (self-reading) |
| `/proc/<pid>/stat` | PROVEN-FROM-SOURCE | YES (self-reading) |
| `/proc/uptime` | PROVEN-FROM-SOURCE | YES |

### How the controller should discover Detectic process:

Based on code analysis, the controller should:

1. Use `ps` (BusyBox) to find Detectic process
2. Parse `ps` output to find PID
3. Use `cat /proc/<PID>/status` to read VmRSS
4. NOT rely on `pidof`, `pgrep`, or `/proc/<PID>/exe`

### Process identity verification:

The controller cannot use `/proc/<PID>/exe` because:
- Detectic code does not use it
- BusyBox `/proc/<PID>/exe` behavior may differ from standard Linux
- symlinks may not work on UBIFS

Alternative: verify by checking `ps` output for the expected command line.

### Process lifetime:

- Detectic runs until SIGTERM/SIGINT or 3 consecutive poll failures
- After reboot: process dies (no autostart)
- After Detectic crash: process dies (external launcher must restart)
- After controller crash: Detectic continues running (independent process)

### Duplicate process prevention:

- Detectic does NOT create a PID file
- Controller must use `ps` to detect existing instances
- Before starting: check `ps | grep detectic`, kill existing if found

### Classification:

| Item | Status |
|------|--------|
| ps available | PROVEN-OFFLINE |
| pidof available | UNKNOWN |
| pgrep available | UNKNOWN |
| /proc/<pid>/exe works | UNKNOWN |
| /proc/<pid>/status works | PROVEN-FROM-SOURCE (self-reading) |
| Process identity via ps | DESIGN (not tested live) |
| Duplicate prevention via ps | DESIGN (not tested live) |
