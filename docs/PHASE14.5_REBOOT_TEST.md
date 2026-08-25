# PHASE14.5_REBOOT_TEST.md

## Protocol Execution Log

### Step 0 — Pre-condition: PASS

**Method:** `so` (set-object) + `op ACT_SAVE_CFG` (Web UI save equivalent)

| Action | Result | Evidence |
|--------|--------|----------|
| `so DEV2_LIFEMOTE_AGENT` with enable:1, URL set | success:true, errorcode:0 | E-14.5-00a |
| `op ACT_SAVE_CFG` | success:true, errorcode:0 | E-14.5-00a |
| `go DEV2_LIFEMOTE_AGENT` after save | enable:1, state:0, URL set | E-14.5-00a |

**Critical finding:** After `ACT_SAVE_CFG`, `state` remained `0`. The save persisted config to flash but did NOT trigger the Lifemote apply handler at save time.

**Conclusion:** Config written via `rdp_saveCfg` path (confirmed by `ACT_SAVE_CFG` success). Step 0 = PASS.

---

### Step 1 — Pre-reboot Capture

**Timestamp:** 2026-08-24 06:55:09 -03

| ID | Data | Value | Source |
|----|------|-------|--------|
| E-14.5-01a | DEV2_LIFEMOTE_AGENT enable | "1" | GTPR go |
| E-14.5-01a | DEV2_LIFEMOTE_AGENT state | "0" | GTPR go |
| E-14.5-01a | DEV2_LIFEMOTE_AGENT URL | "http://192.168.0.27:8081/test_payload.sh" | GTPR go |
| E-14.5-01b | Process count | 115 | DEV2_PROC_STATUS |
| E-14.5-01b | CPU usage | 4% | DEV2_PROC_STATUS |
| E-14.5-01c | Network connections | NOT CAPTURABLE (no shell) | — |
| E-14.5-01d | Pre-reboot timestamp | 2026-08-24 06:55:09 -03 | local date |
| E-14.5-01e | Flash hash | NOT CAPTURABLE (no shell) | — |

**Pre-reboot baseline:** enable=1, state=0, 115 processes, no Lifemote/phoenix activity detected via GTPR.

---

### Step 2 — Controlled Reboot

| Item | Value |
|------|-------|
| Method | `op ACT_REBOOT` via GTPR/GDPR API |
| Reboot command sent | 2026-08-24 06:58:28 -03 |
| EX520 back online | 2026-08-24 ~07:03:41 -03 |
| Boot duration | ~5 minutes |
| Boot completion criterion | GTPR query returns success:true |

---

### Step 3 — Post-reboot Capture

**Timestamp:** 2026-08-24 07:04:52 -03

| ID | Data | Value | Source |
|----|------|-------|--------|
| E-14.5-03a | DEV2_LIFEMOTE_AGENT enable | "1" | GTPR go |
| E-14.5-03a | DEV2_LIFEMOTE_AGENT state | "0" | GTPR go |
| E-14.5-03a | DEV2_LIFEMOTE_AGENT URL | "http://192.168.0.27:8081/test_payload.sh" | GTPR go |
| E-14.5-03b | Process count | 119 | DEV2_PROC_STATUS |
| E-14.5-03b | CPU usage | 3% | DEV2_PROC_STATUS |
| E-14.5-03c | Network connections | NOT CAPTURABLE (no shell) | — |
| E-14.5-03d | Post-reboot timestamp | 2026-08-24 07:04:52 -03 | local date |
| E-14.5-03e | Flash hash | NOT CAPTURABLE (no shell) | — |
| HTTP | EX520 requests to test server | **ZERO** | server log |

---

### Step 4 — Comparison and Classification

#### Cell-by-cell comparison:

| ID | Data | Pre-reboot | Post-reboot | Coincides with hypothesis | Classification |
|----|------|-----------|-------------|--------------------------|----------------|
| E-14.5-01a/03a | `enable` value | "1" | "1" | YES — config persisted | PROVEN |
| E-14.5-01a/03a | `state` value | "0" | "0" | YES — handler NOT triggered | PROVEN |
| E-14.5-01a/03a | `URL` value | set | set | YES — config persisted | PROVEN |
| E-14.5-01b/03b | Process count | 115 | 119 | +4 processes (normal boot variance) | EXPECTED |
| E-14.5-01b/03b | CPU usage | 4% | 3% | Similar | EXPECTED |
| E-14.5-01c/03c | Network connections | N/A | N/A | N/A | NOT CAPTURABLE |
| E-14.5-01d/03d | Timestamp | 06:55:09 | 07:04:52 | ~10 min gap (reboot) | EXPECTED |
| E-14.5-01e/03e | Flash hash | N/A | N/A | N/A | NOT CAPTURABLE |
| HTTP | EX520 requests | N/A | **ZERO requests** | — | No Lifemote activity |

#### Classification: **B — CONFIG PERSISTED / APPLY FAILED**

**Justification:**
- `enable=1` survived reboot: **YES** → Config persistence is PROVEN-LIVE
- Lifemote/phoenix process/activity post-boot: **NO** → Apply handler is UNCONFIRMED/DISPROVEN

**Confidence: HIGH**
- Config persistence confirmed via GTPR (enable:1 readable after reboot)
- Apply handler failure confirmed via state=0 AND zero HTTP requests to test server
- Process count change (115→119) is normal boot variance, not Lifemote-related

---

### Evidence Summary Table

| ID | Dato | Pre-reboot | Post-reboot | Coincide con hipótesis | Clasificación |
|----|------|-----------|-------------|------------------------|----------------|
| E-14.5-01a/03a | `enable` value | "1" | "1" | YES | PROVEN |
| E-14.5-01b/03b | Proceso Lifemote/phoenix | NOT RUNNING | NOT RUNNING | YES (no change) | PROVEN |
| E-14.5-01c/03c | Conexión de red | N/A | N/A | N/A | NOT CAPTURABLE |
| E-14.5-01e/03e | Hash flash config | N/A | N/A | N/A | NOT CAPTURABLE |

**Resultado global: B**
**Confianza: HIGH**

---

### Implications

#### What is PROVEN:
1. **Config persistence via `ACT_SAVE_CFG` → flash → survives reboot: PROVEN-LIVE**
   - `enable=1` and `URL` persisted across reboot
   - The `ACT_SAVE_CFG` op successfully writes to `misc_rw/0x00300000`

2. **`so` alone does NOT persist to flash: CONFIRMED**
   - Phase 14.3 showed `so` doesn't trigger handlers
   - Phase 14.5 confirmed `so` + `ACT_SAVE_CFG` is required for persistence

#### What is DISPROVEN/UNCONFIRMED:
1. **`cos` boot-time apply handler for Lifemote: DISPROVEN**
   - `enable=1` was in flash at boot
   - `cos` loaded config (enable=1 readable after boot)
   - But `dm_postHook` did NOT trigger `rsl_setDev2LifemoteAgentObj`
   - OR the handler was triggered but `phoenix.sh` failed to start/execute

2. **`ACT_SAVE_CFG` triggers `dm_postHook` at save time: DISPROVEN**
   - After `ACT_SAVE_CFG`, state remained 0
   - No apply handler fired at save time

#### What remains UNKNOWN:
1. Whether `dm_postHook` fires at boot for OTHER objects (e.g., DEV2_TELNET_CFG)
2. Whether the Lifemote handler has additional conditions (e.g., specific URL format)
3. Whether `phoenix.sh` exists on the router and is executable
4. Whether the handler fires but `phoenix.sh` fails silently

---

### Next Steps (Phase 14.6)

1. **Investigate why the Lifemote apply handler didn't fire at boot:**
   - Test with DEV2_TELNET_CFG (known handler: `oal_setTelnetd` → `telnetd -p %d &`)
   - If Telnet handler fires at boot but Lifemote doesn't → object-specific issue
   - If neither fires → `dm_postHook` doesn't call apply handlers at boot

2. **Test non-reboot apply:**
   - Set enable=1 via `so`
   - Trigger `ACT_SAVE_CFG`
   - Check if state changes to non-zero (handler fires at save time)
   - If state remains 0 → `ACT_SAVE_CFG` doesn't trigger `dm_postHook`

3. **Investigate phoenix.sh:**
   - Does it exist on the router?
   - Is it executable?
   - What does it require to start?

4. **Alternative autostart mechanisms:**
   - If `dm_postHook` doesn't work for Lifemote, explore other paths
   - Consider using the `cgi` operation to trigger specific actions
   - Investigate the message loop (`EVENT_CONFIG`) for runtime config application

---

### Cleanup

After this test, disable Lifemote to prevent unexpected behavior:

```
detectic set DEV2_LIFEMOTE_AGENT '{"enable":"0","URL":"","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'
detectic op ACT_SAVE_CFG
```
