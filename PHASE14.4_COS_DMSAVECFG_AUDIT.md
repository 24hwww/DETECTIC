# PHASE14.4_COS_DMSAVECFG_AUDIT.md

## 1. `cos` Identity

| Property | Value | Evidence |
|----------|-------|----------|
| Location | `/bin/cos` | filesystem |
| Architecture | ELF 64-bit LSB, ARM aarch64 | file command |
| Linking | Dynamically linked | readelf -d |
| Interpreter | `/lib/ld-musl-aarch64.so.1` | file command |
| Stripped | Yes | file command |
| Size | 422,840 bytes (423 KB) | stat |
| Started by | `rcS` (`cos &`) | rcS line ~310 |

### Dependencies (NEEDED):

```
libtrk.so       — TP-Link routing/tracking
libcmm.so       — TP-Link config management (dm_saveCfg, apply handlers)
libxml.so       — XML parsing
libcutil.so     — TP-Link utilities (compression, encryption)
libcJSON.so     — JSON parsing
libos.so        — OS abstraction (threads, time)
libonig.so.5    — Oniguruma regex
libgdpr.so      — GDPR/GTPR API handler
libgcc_s.so.1   — GCC runtime
libc.so         — musl libc
```

### Operation model:

`cos` is a **message-based configuration manager daemon**. It:
1. Initializes the data model from flash
2. Starts a socket-based message server
3. Listens for configuration change events
4. Applies configuration changes via per-object handlers

### Main loop:

```
cos_init
  → dm_init (load config from flash)
  → dm_getObj (enumerate objects)
  → dm_postHook (apply initial config)
  → os_threadCreate (start threads)
  → rdp_init (initialize RDP layer)
  → msg_init → msg_srvInit → socket → bind → select → msg_recv
```

---

## 2. `dm_saveCfg` Identity

| Property | Value | Evidence |
|----------|-------|----------|
| Location | `libcmm.so` | strings grep |
| Callers | `rdp_saveCfg` (in cos), `dm_restoreCfg`, `rsl_sys_restoreDefaultCfg` | strings context |
| Side effects | Write to flash, trigger `dm_postHook` | strings context |
| Error message | `dm_saveCfg failed.` | strings |

### Related functions in `libcmm.so`:

```
dm_loadCfg      — Load config from flash into memory
dm_saveCfg      — Save config from memory to flash + trigger postHook
dm_backupCfg    — Backup config
dm_restoreCfg   — Restore config from backup
dm_cleanupCfg   — Cleanup config
dm_dumpDm       — Dump data model
dm_dumpDmByOid  — Dump by OID
```

---

## 3. `cos → dm_saveCfg` Call Path

### Initialization path (at boot):

```
rcS
  ↓
cos &
  ↓
cos_init
  ↓
dm_init              ← loads config from flash
  ↓
dm_getObj            ← enumerates objects
  ↓
dm_postHook          ← APPLIES initial config (handlers fire)
  ↓
os_threadCreate      ← starts threads
  ↓
rdp_init             ← initializes RDP layer
  ↓
msg_init → msg_srvInit → socket → bind → select → msg_recv
```

### Runtime path (on config change via web UI):

```
Web UI "Save"
  ↓
GTPR/GDPR API
  ↓
rdp_setObj           ← sets object in memory
  ↓
rdp_saveCfg          ← calls dm_saveCfg
  ↓
dm_saveCfg           ← writes to flash + triggers dm_postHook
  ↓
dm_postHook          ← fires per-object apply handlers
  ↓
oal_setTelnetd       ← for DEV2_TELNET_CFG
  ↓
telnetd -p %d &      ← service starts
```

### `so` API path (from Phase 14.3):

```
detectic set OID data
  ↓
GTPR/GDPR API
  ↓
rdp_setObj           ← sets object in memory
  ↓
[NO rdp_saveCfg]     ← handler NOT triggered
  ↓
[NO dm_saveCfg]      ← config NOT persisted to flash
  ↓
[NO dm_postHook]     ← apply handlers NOT fired
```

**This is why Phase 14.3 found that `so` stores config but doesn't trigger handlers.**

---

## 4. `dm_postHook` Mechanism

### Evidence:

From `cos` strings:
```
dm_init
dm_getObj
dm_postHook
os_threadCreate
```

And:
```
do dm_postHook faied    (typo for "failed")
```

### Analysis:

`dm_postHook` is called:
1. During `cos` initialization (after `dm_init` + `dm_getObj`)
2. After `dm_saveCfg` (when config is saved to flash)

It is NOT called:
1. After `rdp_setObj` alone (in-memory write only)
2. After the `so` API command (which only calls `rdp_setObj`)

### What `dm_postHook` does:

It iterates over all data model objects and calls the per-object apply handlers. For example:
- `DEV2_TELNET_CFG` → `rsl_setDev2TelnetCfgObj` → `oal_setTelnetd` → `telnetd -p %d &`
- `DEV2_LIFEMOTE_AGENT` → `rsl_setDev2LifemoteAgentObj` → `phoenix.sh %s &`

---

## 5. Event System

### Constants found in `cos`:

```
EVENT_CONFIG    — configuration change event
EVENT_DETECT    — detection event
EVENT_ADDRESS   — address event
EVENT_MESH      — mesh event
EVENT_LINK      — link event
EVENT_TIMER     — timer event
```

### Message loop:

```
msg_init → msg_srvInit → socket → bind → select → msg_recv
```

`cos` listens on a Unix domain socket for messages. When an `EVENT_CONFIG` message is received, it triggers the configuration apply path.

### Key insight:

The message loop means `cos` can receive config-change notifications at runtime. If a config change is committed (via `dm_saveCfg`), `cos` receives an `EVENT_CONFIG` message and applies the change.

---

## 6. Security/Safety Classification

| Property | Classification |
|----------|---------------|
| `cos` binary | Manufacturer binary, read-only (SquashFS) |
| `dm_saveCfg` | Library function in libcmm.so |
| `dm_postHook` | Apply mechanism in libcmm.so |
| Apply handlers | Per-object callbacks in libcmm.so |
| Event system | Socket-based message passing |
| Non-reboot apply | Via `dm_saveCfg` → `dm_postHook` |

**No security vulnerabilities were sought or exploited.**
**No firmware modification was performed.**
**All analysis was strictly read-only.**
