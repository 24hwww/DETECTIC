# PHASE14.4_EVIDENCE_INDEX.md

## Evidence Table

| ID | Source | Location | Type | Observation | Conclusion | Confidence |
|----|--------|----------|------|-------------|------------|------------|
| E01 | filesystem | `_rootfs/bin/cos` | binary | ELF 64-bit ARM aarch64, dynamically linked, 423 KB | cos is manufacturer binary | HIGH |
| E02 | readelf | `_rootfs/bin/cos` | binary analysis | Dependencies: libcmm.so, libtrk.so, libxml.so, libcutil.so, libcJSON.so, libos.so, libonig.so.5, libgdpr.so | cos depends on config management libraries | HIGH |
| E03 | strings | `_rootfs/bin/cos` | binary strings | `dm_init`, `dm_getObj`, `dm_postHook`, `os_threadCreate` in sequence | dm_postHook is called during cos initialization | HIGH |
| E04 | strings | `_rootfs/bin/cos` | binary strings | `rdp_saveCfg` present | cos can save config via rdp_saveCfg | HIGH |
| E05 | strings | `_rootfs/bin/cos` | binary strings | `EVENT_CONFIG`, `EVENT_DETECT`, `EVENT_ADDRESS`, `EVENT_MESH`, `EVENT_LINK`, `EVENT_TIMER` | cos has event-driven architecture | HIGH |
| E06 | strings | `_rootfs/bin/cos` | binary strings | `msg_init`, `msg_srvInit`, `socket`, `bind`, `select`, `msg_recv` | cos uses socket-based message loop | HIGH |
| E07 | strings | `_rootfs/bin/cos` | binary strings | `util_exec_system`, `util_exec_findProc` | cos can execute system commands | HIGH |
| E08 | strings | `_rootfs/bin/cos` | binary strings | `obuspa &` | cos starts obuspa daemon | HIGH |
| E09 | strings | `_rootfs/bin/cos` | binary strings | `User Config has been changed.` | cos detects config changes | HIGH |
| E10 | strings | `_rootfs/lib/libcmm.so` | binary strings | `dm_saveCfg`, `dm_saveCfg failed.` | dm_saveCfg exists in libcmm.so | HIGH |
| E11 | strings | `_rootfs/lib/libcmm.so` | binary strings | `dm_loadCfg`, `dm_saveCfg`, `dm_backupCfg`, `dm_restoreCfg`, `dm_cleanupCfg` | Complete config lifecycle in libcmm.so | HIGH |
| E12 | strings | `_rootfs/lib/libcmm.so` | binary strings | `dm_postHook`, `do dm_postHook faied` | dm_postHook exists and can fail | HIGH |
| E13 | strings | `_rootfs/lib/libcmm.so` | binary strings | `rsl_setDev2TelnetCfgObj`, `oal_setTelnetd`, `telnetd -p %d &` | Telnet apply handler chain | HIGH |
| E14 | strings | `_rootfs/lib/libcmm.so` | binary strings | `rsl_setDev2LifemoteAgentObj`, `phoenix`, `/usr/bin/phoenix.sh` | Lifemote apply handler chain | HIGH |
| E15 | strings | `_rootfs/lib/libcmm.so` | binary strings | `rsl_setDev2AppCfgObj` | AppCfg (Lifemote parent) has set handler | HIGH |
| E16 | live test | Phase 14.3 | GTPR API | `so` on DEV2_LIFEMOTE_AGENT with enable:1 → state:0, no HTTP request | so does NOT trigger apply handlers | HIGH |
| E17 | live test | Phase 14.3 | GTPR API | `so` on DEV2_TELNET_CFG with telnetLocalEnabled:1 → port 23 CLOSED | so does NOT trigger apply handlers | HIGH |
| E18 | live test | Phase 14.3 | GTPR API | Config readable via `gl` after `so` | Config is stored in memory | HIGH |
| E19 | source | `_rootfs/etc/init.d/rcS` | boot script | `cos &` started after partition mount | cos starts at boot | HIGH |
| E20 | source | `_rootfs/etc/init.d/rcS` | boot script | `cp /etc/mfg_config.bin /var/run/misc/misc_rw/0x00300000` (if not exists) | Factory config copied on first boot | HIGH |
| E21 | static analysis | `_rootfs/etc/init.d/rcS` | boot script | No `source`/`.` commands targeting misc_rw | rcS does not execute scripts from writable storage | HIGH |
| E22 | strings | `_rootfs/bin/cos` | binary strings | `dm_postHook` called after `dm_init` + `dm_getObj` | dm_postHook fires at boot during cos init | HIGH |
| E23 | strings | `_rootfs/bin/cos` | binary strings | `cos_init` → `main` | cos has standard main entry point | MEDIUM |
| E24 | strings | `_rootfs/bin/cos` | binary strings | `RSL init error!` | cos reports init errors | LOW |

## Classification Summary

| Classification | Count | Key Items |
|---------------|-------|-----------|
| PROVEN-FROM-SOURCE | 18 | cos binary, dm_saveCfg, dm_postHook, apply handlers, boot sequence |
| PROVEN-LIVE | 3 | so does not trigger handlers, config stored in memory |
| INFERENCE | 2 | Web UI Save triggers rdp_saveCfg, cos init applies config |
| UNKNOWN | 1 | Whether cos calls rdp_saveCfg at boot (no direct string evidence) |

## Critical Unknown

**Does `cos` call `rdp_saveCfg` during initialization?**

Evidence:
- `rdp_saveCfg` exists in cos binary
- `dm_postHook` IS called during init (after dm_init + dm_getObj)
- But `rdp_saveCfg` is NOT in the init sequence strings

This unknown determines whether:
- Config changed at runtime (via web UI) persists across reboot
- Config changed via `so` persists across reboot

**Classification: UNKNOWN — requires live test or disassembly**
