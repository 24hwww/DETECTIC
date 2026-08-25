# PHASE14.6_APPLY_HANDLER_EVIDENCE.md

## Summary

**`dm_postHook` does NOT trigger per-object apply handlers — neither at save time NOR at boot time.**

This is the definitive finding from Phase 14.5 + Phase 14.6 live testing.

---

## Evidence Chain

### Test 1: Lifemote at save time (Phase 14.5 Step 0)

| Action | Result |
|--------|--------|
| `so DEV2_LIFEMOTE_AGENT` enable:1 | success:true |
| `op ACT_SAVE_CFG` | success:true |
| `go DEV2_LIFEMOTE_AGENT` | enable:1, **state:0** |

**Conclusion:** Config persisted, but Lifemote apply handler (`rsl_setDev2LifemoteAgentObj`) did NOT fire.

### Test 2: Lifemote at boot (Phase 14.5 Step 3)

| Action | Result |
|--------|--------|
| Pre-reboot: enable:1, state:0 | Config in flash |
| `op ACT_REBOOT` | success:true |
| Post-reboot: enable:1, state:0 | Config loaded from flash |
| HTTP requests to test server | **ZERO** |

**Conclusion:** Config survived reboot, but `cos` at boot did NOT trigger `dm_postHook` → `rsl_setDev2LifemoteAgentObj` → `phoenix.sh`.

### Test 3: Telnet at save time (Phase 14.6-1)

| Action | Result |
|--------|--------|
| `so DEV2_TELNET_CFG` telnetLocalEnabled:1 | success:true |
| `op ACT_SAVE_CFG` | success:true |
| `go DEV2_TELNET_CFG` | telnetLocalEnabled:1 |
| Port 23 | **CLOSED** |

**Conclusion:** Config persisted, but Telnet apply handler (`oal_setTelnetd`) did NOT fire.

### Test 4: Telnet at boot (Phase 14.6-2)

| Action | Result |
|--------|--------|
| Pre-reboot: telnetLocalEnabled:1 | Config in flash |
| `op ACT_REBOOT` | success:true |
| Post-reboot: telnetLocalEnabled:1 | Config loaded from flash |
| Port 23 | **CLOSED** |

**Conclusion:** Config survived reboot, but `cos` at boot did NOT trigger `dm_postHook` → `rsl_setDev2TelnetCfgObj` → `oal_setTelnetd` → `telnetd`.

---

## Classification Matrix

| Object | Save-time apply | Boot-time apply | Config persists |
|--------|:---:|:---:|:---:|
| DEV2_LIFEMOTE_AGENT | ❌ DISPROVEN | ❌ DISPROVEN | ✅ PROVEN |
| DEV2_TELNET_CFG | ❌ DISPROVEN | ❌ DISPROVEN | ✅ PROVEN |

---

## Implications

### What is PROVEN:
1. **Config persistence via `ACT_SAVE_CFG` → flash: PROVEN-LIVE**
   - Both Lifemote and Telnet configs survive reboot
   - `ACT_SAVE_CFG` successfully writes to `misc_rw/0x00300000`

2. **Config loading at boot: PROVEN-LIVE**
   - `cos` loads config from flash at boot
   - Config is readable via GTPR after reboot

3. **`so` requires `ACT_SAVE_CFG` for persistence: PROVEN-LIVE**
   - `so` alone does NOT persist to flash
   - `so` + `ACT_SAVE_CFG` = persistent config

### What is DISPROVEN:
1. **`dm_postHook` triggers apply handlers at save time: DISPROVEN**
   - `ACT_SAVE_CFG` does NOT trigger `dm_postHook`
   - Or `dm_postHook` fires but apply handlers don't execute

2. **`dm_postHook` triggers apply handlers at boot: DISPROVEN**
   - `cos` at boot does NOT trigger apply handlers
   - Config is loaded but not applied

3. **Phase 14.4 hypothesis "cos init → dm_postHook → apply handlers": DISPROVEN**
   - The static analysis of cos strings was misleading
   - The actual runtime behavior does NOT match the hypothesized sequence

### What remains UNKNOWN:
1. Whether `dm_postHook` fires at all (it might fire but be a no-op)
2. What actually triggers apply handlers in the web UI
3. Whether there's a different code path for web UI saves vs CLI saves
4. Whether `cos` has a different initialization sequence than what strings suggest

---

## Revised Architecture Understanding

### Previous hypothesis (Phase 14.4):
```
cos init → dm_init → dm_getObj → dm_postHook → apply handlers → services start
```

### Actual behavior (Phase 14.5 + 14.6):
```
cos init → dm_init → dm_getObj → [dm_postHook fires but does NOT call apply handlers]
                                         ↓
                                    config loaded into memory
                                         ↓
                                    apply handlers NOT triggered
                                         ↓
                                    services NOT started
```

### What the web UI actually does (unknown):
```
Web UI Save → so (rdp_setObj) → ACT_SAVE_CFG (rdp_saveCfg?) → [???] → services start
```

The `[???]` is the missing mechanism. The web UI must have some way to trigger apply handlers that we haven't identified yet.

---

## Next Investigation Areas

1. **Web UI apply mechanism:**
   - How does the web UI actually start services after Save?
   - Is there a CGI endpoint that triggers apply handlers?
   - Does the web UI use a different code path than `ACT_SAVE_CFG`?

2. **cos message loop:**
   - The `msg_recv` loop might process `EVENT_CONFIG` messages
   - Maybe apply handlers are triggered via messages, not via `dm_postHook`
   - Investigate the socket-based message system

3. **Alternative autostart mechanisms:**
   - If `dm_postHook` doesn't work, find what does
   - Investigate `EVENT_CONFIG` handling
   - Look for other entry points in the cos binary

4. **Binary analysis:**
   - Disassemble `dm_postHook` to understand what it actually does
   - Trace the `cos` initialization sequence more carefully
   - Look for conditional logic that might prevent apply handlers from firing

---

## Cleanup Status

- Lifemote: disabled (enable:0) ✅
- Telnet: disabled (telnetLocalEnabled:0) ✅
- HTTP server: stopped ✅
- Router: stable, all configs at defaults ✅
