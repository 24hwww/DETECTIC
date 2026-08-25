# PHASE14.4_PERSISTENCE_PATH.md

## Configuration Persistence Chain

### WRITE

```
Web UI "Save"  OR  GTPR/GDPR API "so"  OR  backup restore
         ↓
    rdp_setObj (in-memory)
         ↓
    rdp_saveCfg → dm_saveCfg (if explicit save)
         ↓
    Write to flash: misc_rw/0x00300000
```

### PERSIST

```
misc_rw/0x00300000
    ↓
UBI volume (ubi2:misc_rw)
    ↓
SPI NAND flash
    ↓
Survives reboot (UBI wear-leveling)
```

### RESTORATION / LOAD (at boot)

```
rcS
    ↓
Mount misc_rw (UBIFS)
    ↓
cos &
    ↓
dm_init
    ↓
Read misc_rw/0x00300000
    ↓
Load into in-memory data model
```

### APPLY (at boot)

```
dm_init (loads config)
    ↓
dm_getObj (enumerates objects)
    ↓
dm_postHook ← APPLIES all per-object handlers
    ↓
For each object with changed values:
    rsl_setDev2*Obj → oal_* → service start/restart
```

### SERVICE START

```
dm_postHook
    ↓
DEV2_TELNET_CFG handler:
    rsl_setDev2TelnetCfgObj
        ↓
    oal_setTelnetd
        ↓
    telnetd -p %d &

DEV2_LIFEMOTE_AGENT handler:
    rsl_setDev2LifemoteAgentObj
        ↓
    phoenix.sh %s &
```

---

## Key Distinction: `so` vs `dm_saveCfg`

| Operation | In-memory | Persist to flash | Apply handlers |
|-----------|-----------|-----------------|----------------|
| `so` (GTPR API) | YES | NO (not confirmed) | NO |
| `dm_saveCfg` | YES | YES | YES (via dm_postHook) |
| Web UI "Save" | YES | YES (via rdp_saveCfg → dm_saveCfg) | YES |
| Backup restore | YES | YES (via dm_restoreCfg → dm_saveCfg) | YES |
| Boot (cos init) | YES (load) | NO (load only) | YES (via dm_postHook) |

---

## Boot-Time Apply Evidence

### From `cos` strings:

```
dm_init → dm_getObj → dm_postHook → os_threadCreate
```

This sequence occurs during `cos` initialization at boot. `dm_postHook` fires after loading the config from flash, which means **per-object apply handlers are triggered at boot**.

### Implication for autostart:

If `DEV2_LIFEMOTE_AGENT.enable=1` is persisted in `misc_rw/0x00300000`, then at boot:
1. `cos` loads config from flash
2. `dm_postHook` iterates objects
3. `rsl_setDev2LifemoteAgentObj` handler fires
4. `phoenix.sh <URL> &` starts
5. `phoenix.sh` downloads script from URL
6. Script executes

**This is the autostart mechanism.**

---

## What `so` Does NOT Do

The `so` command:
1. Calls `rdp_setObj` — sets object in memory
2. Does NOT call `rdp_saveCfg` — does not persist to flash
3. Does NOT trigger `dm_postHook` — does not apply handlers

This is why Phase 14.3 found that `so` stores config but doesn't start services.

---

## What `so` DOES Do

The `so` command:
1. Sets the object in the in-memory data model
2. The in-memory change is visible via `gl` (query)
3. The change persists in the data model object
4. But it is NOT written to flash by `so` alone

**The `so` command may write to flash via a different mechanism (e.g., periodic auto-save, or the data model itself persists in shared memory).**

However, the apply handlers are NOT triggered by `so` alone.

---

## Non-Reboot Apply Assessment

### Classification:

**B — Possible but unproven**

A candidate mechanism exists:
- `rdp_saveCfg` → `dm_saveCfg` → `dm_postHook` → apply handlers

But:
- The web UI "Save" button triggers this path
- The `so` API command does NOT trigger this path
- There is no evidence of an automatic periodic save
- The event system (`EVENT_CONFIG`) could theoretically trigger saves

### What would prove non-reboot apply:

1. Enable Lifemote via `so` (config stored in memory)
2. Trigger `dm_saveCfg` explicitly (e.g., via web UI "Save")
3. Observe `phoenix.sh` starting without reboot

This would prove that `dm_saveCfg` → `dm_postHook` → apply handlers works at runtime.

### What would disprove non-reboot apply:

1. Enable Lifemote via `so`
2. Trigger `dm_saveCfg` explicitly
3. Observe NO change in runtime state

This would mean `dm_postHook` does NOT trigger apply handlers at runtime.

---

## Summary

```
CONFIG WRITE (so)
     ↓
IN-MEMORY (rdp_setObj)
     ↓
PERSISTENCE (rdp_saveCfg → dm_saveCfg → flash) ← MISSING from `so`
     ↓
RESTORATION (dm_init at boot)
     ↓
APPLY (dm_postHook at boot) ← THIS IS THE AUTOSTART MECHANISM
     ↓
SERVICE START (apply handlers)
     ↓
RUNNING
```

The missing transition from `so` is: **PERSISTENCE** (rdp_saveCfg → dm_saveCfg).

At boot, this transition IS performed by `cos` (dm_init loads from flash, dm_postHook applies).

**Therefore: configuration persisted via `so` + explicit save (or web UI) WILL be applied at boot by `cos`.**
