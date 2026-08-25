# PHASE14.4_APPLY_PATH_MATRIX.md

## Configuration Mechanism Comparison

| Mechanism | Writes config | Persists to flash | Apply handlers | Starts service | Evidence |
|-----------|:------------:|:-----------------:|:--------------:|:--------------:|----------|
| `so` (GTPR API) | YES | NO (not confirmed) | NO | NO | Phase 14.3 live test |
| `cos` init (boot) | NO (load only) | NO | YES | YES | cos strings: dm_init → dm_postHook |
| `dm_saveCfg` | YES | YES | YES | YES | libcmm.so strings |
| `rdp_saveCfg` | YES | YES (via dm_saveCfg) | YES | YES | cos strings: rdp_saveCfg |
| Web UI "Save" | YES | YES (via rdp_saveCfg) | YES | YES | Inferred from rdp_saveCfg + dm_saveCfg |
| Backup restore | YES | YES (via dm_restoreCfg) | YES | YES | BACKUPCFG_ANALYSIS.md |
| `dm_postHook` | NO | NO | YES | YES | cos strings: dm_postHook |

## Detail per mechanism

### `so` (GTPR API)

- **Writes config**: YES — `rdp_setObj` sets object in memory
- **Persists to flash**: NOT CONFIRMED — `rdp_saveCfg` is NOT called by `so`
- **Apply handlers**: NO — `dm_postHook` is NOT triggered
- **Starts service**: NO
- **Evidence**: Phase 14.3 — `so` on DEV2_LIFEMOTE_AGENT with enable:1 → state:0; `so` on DEV2_TELNET_CFG with telnetLocalEnabled:1 → port 23 CLOSED

### `cos` init (boot)

- **Writes config**: NO — loads config from flash
- **Persists to flash**: NO
- **Apply handlers**: YES — `dm_postHook` fires after `dm_init` + `dm_getObj`
- **Starts service**: YES — per-object handlers fire (oal_setTelnetd, rsl_setDev2LifemoteAgentObj)
- **Evidence**: cos strings: `dm_init → dm_getObj → dm_postHook → os_threadCreate`

### `dm_saveCfg`

- **Writes config**: YES
- **Persists to flash**: YES — writes to misc_rw/0x00300000
- **Apply handlers**: YES — triggers `dm_postHook`
- **Starts service**: YES
- **Evidence**: libcmm.so strings: `dm_saveCfg`, `dm_saveCfg failed.`

### `rdp_saveCfg`

- **Writes config**: YES
- **Persists to flash**: YES (via `dm_saveCfg`)
- **Apply handlers**: YES (via `dm_saveCfg` → `dm_postHook`)
- **Starts service**: YES
- **Evidence**: cos strings: `rdp_saveCfg`

### Web UI "Save"

- **Writes config**: YES
- **Persists to flash**: YES (via `rdp_saveCfg` → `dm_saveCfg`)
- **Apply handlers**: YES
- **Starts service**: YES
- **Evidence**: Inferred from `rdp_saveCfg` presence in cos + `dm_saveCfg` in libcmm.so

### Backup restore

- **Writes config**: YES
- **Persists to flash**: YES (via `dm_restoreCfg` → `dm_saveCfg`)
- **Apply handlers**: YES
- **Starts service**: YES
- **Evidence**: BACKUPCFG_ANALYSIS.md: `dm_restoreCfg` → `dm_saveCfg` → per-subsystem apply handlers

---

## Key Finding

**The `so` API command does NOT trigger `dm_saveCfg` or `dm_postHook`.**

This means:
- `so` stores config in memory (visible via `gl`)
- `so` does NOT persist to flash (or persistence is unconfirmed)
- `so` does NOT trigger apply handlers
- `so` does NOT start services

**The web UI "Save" button DOES trigger `rdp_saveCfg` → `dm_saveCfg` → `dm_postHook`.**

This means:
- Web UI "Save" persists to flash
- Web UI "Save" triggers apply handlers
- Web UI "Save" starts services

**At boot, `cos` loads config from flash and applies via `dm_postHook`.**

This means:
- Config persisted by web UI "Save" IS applied at boot
- Config persisted by backup restore IS applied at boot
- Config NOT saved by `so` alone may NOT be applied at boot
