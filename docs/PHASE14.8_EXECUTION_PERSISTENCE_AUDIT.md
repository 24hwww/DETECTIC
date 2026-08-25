# PHASE 14.8 — COMPLETE

## Execution-Path / Persistence Boundary Audit

Method: Static analysis of extracted rootfs (`_rootfs/`), firmware image
(`EX520_UP_BOOT_2025-07-31_11.34.16.bin`), and all binaries/libraries.
No live writes, no reboots, no firmware modification, no privilege
escalation. Read-only / static analysis only.

---

## 1. Executive Verdict

### **D — NO LEGITIMATE ROUTER-SIDE DEPLOYMENT PATH FOUND**

No supported persistent execution mechanism can be established from
available evidence. The firmware contains execution primitives
(`util_exec_system`, `system()`, `popen`, `fork`/`execvp`) used
internally by `cos` to manage fixed vendor binaries, but there is no
legitimate, supported mechanism for a user to deploy a custom
persistent binary. The one hook mechanism that exists (`rcsHook` →
`/etc/rcS_hook/`) is **orphaned** — nothing in the boot chain calls
it, and the hook directory is empty. Persistent writable storage
(`misc_rw`) is used exclusively for config data, not executable code.

---

## 2. Deployment Boundary Matrix

| Capability                    | Firmware evidence | Current access | Live proof | Classification |
| ----------------------------- | ----------------- | -------------- | ---------- | -------------- |
| Persistent storage            | YES (UBI misc_rw, misc_rw_bak on NAND) | NO (GTPR cannot write files) | NO | FIRMWARE CAPABILITY ONLY |
| `/misc_rw` (UBI at `/var/run/misc/misc_rw`) | YES (rw mount in rcS) | NO | NO | FIRMWARE CAPABILITY ONLY |
| File write to misc_rw         | YES (libcmm.so `oal_sys_writeAppFlash`) | NO (no GTPR file-write op) | NO | FIRMWARE CAPABILITY ONLY |
| File execution from misc_rw   | NO (no binary reads executable content from misc_rw) | NO | NO | NOT PRESENT IN FIRMWARE |
| Init/autostart                | YES (BusyBox init → rcS → cos) | NO (cannot modify rcS or init) | NO | FIRMWARE CAPABILITY ONLY |
| Custom script (rcS_hook)      | YES (rcsHook binary with `doRcsHookExes`) | NO (rcsHook is orphaned; dir empty) | NO | ORPHANED — NOT WIRED |
| Custom daemon                 | NO (cos starts only fixed rootfs paths) | NO | NO | NOT PRESENT IN FIRMWARE |
| Watchdog restart              | YES (`checkAndRestartDetectProcess`) | NO | NO | WAN-DETECT ONLY — NOT CUSTOM CODE |
| Supported extension mechanism | NO (no plugin dir, no rc.local, no cron) | NO | NO | NOT PRESENT IN FIRMWARE |

---

## 3. Startup Chain

Reconstructed from `/etc/inittab`, `/etc/init.d/rcS`,
`/etc/init.d/rcS.model`, `/etc/config.bba`, kernel bootargs, and
binary string analysis of `cos`, `rcsHook`, `libcmm.so`.

```
U-Boot / BL2 (SPI NAND preloader)
  │
  │ bootargs = "ubi.mtd=ubi0 console=ttyS0,115200n1 loglevel=8
  │            earlycon=uart8250,mmio32,0x11002000 AC=300"
  │ (no rootflags=ro, no init= override)
  ▼
Linux kernel (UBI volume: kernelA or kernelB)
  │
  │ mounts rootfs from UBI volume (rootfsA or rootfsB)
  │ rootfs type = UBIFS (INCLUDE_UBI_ROOTFS_TYPE_SQUASH NOT set)
  ▼
/sbin/init → /bin/busybox (BusyBox init, NOT procd)
  │
  │ reads /etc/inittab:
  │   ::sysinit:/etc/init.d/rcS
  │   ::askfirst:/sbin/getty -L ttyS0 115200 vt100
  ▼
/etc/init.d/rcS  (10412 bytes, the SOLE startup script)
  │
  ├── mount -a  (fstab: /var=ramfs, /proc, /dev/pts)
  ├── mount sysfs, debugfs
  ├── ubiattach + mount:
  │     misc_ro   → /var/run/misc/misc_ro   (UBIFS, -r read-only)
  │     misc_rw   → /var/run/misc/misc_rw   (UBIFS, read-write)
  │     misc_rw_bak → /var/run/misc/misc_rw_bak (UBIFS, read-write)
  │     misc_isp  → /var/run/misc/misc_isp  (UBIFS, -r read-only)
  ├── udevtrigger &
  ├── mkdir /var/{lock,log,run,tmp,...}
  ├── insmod kernel modules (tp_board, tp_gpio, ivi, etc.)
  ├── source /etc/init.d/rcS.model  (eth up, mknod, mii.ko)
  ├── insmod ipt_STAT.ko, mtkhnat.ko
  ├── check/copy userconfig: /var/run/misc/misc_rw/0x00300000
  ├── cos &                          ← MAIN SUPERVISOR
  ├── cmmsyslogd &
  ├── amixer settings
  ├── cp /etc/cloud/cloud_service.cfg → /tmp/
  ├── sleep 3
  └── sleep 100 && drop_caches &
  ▼
cos  (main process supervisor, 422KB)
  │
  ├── dm_init() — load config from misc_rw (file 0x00300000)
  ├── msg_init / msg_srvInit — IPC server (/var/tmp/apdev_msg_send)
  ├── Main loop: msg_recv → rdp_action → event handlers
  │
  └── Manages daemons from FIXED ROOTFS PATHS only:
        httpd, telnetd, dropbear, cloud_https, cwmp, nrd, tmpd,
        dnsProxy, igmpd, mldProxy, dyndns, noipdns, ntpc, snmpd,
        diagTool, ipsecVpn, muAgentD, qoeStatisticsHandler,
        tr143d, wanconnd2, wlNetlinkTool, obuspa, apsd
        (all launched as /bin/<name> or /usr/sbin/<name>)
```

### Where could a Detectic binary legitimately enter?

**Nowhere.** There is no point in this chain where a user-controlled
file from persistent storage is executed:

- `rcS` runs only fixed commands and sources only `rcS.model`.
- `rcS` does **NOT** call `rcsHook`, does **NOT** iterate
  `/etc/rcS_hook/`, does **NOT** run `run-parts` on any directory.
- `cos` starts daemons from hardcoded rootfs paths only.
- No `/etc/init.d/*` service enumeration (no procd).
- No `rc.local`, no cron, no scheduled tasks.
- `misc_rw` stores config data (file `0x00300000`), not executables.
- No binary references executing files from `/var/run/misc/`.

---

## 4. Access Boundary

### 4.1 What the firmware CAN theoretically do

| Capability | Evidence |
|---|---|
| Execute arbitrary shell commands | `util_exec_system()` in libcmm.so (popen/fork/execvp) |
| Start telnetd | `oal_setTelnetd()` calls `system("telnetd -p %d")` |
| Start dropbear | `cos` has `dropbearRestart` and dropbear management |
| Execute scripts from `/etc/rcS_hook/` | `rcsHook` binary has `doRcsHookExes()` using `util_exec_system` |
| Replace firmware | `cloud_https` / `cwmp` download + RSA-verify + flash write |
| Modify U-Boot env | `fw_setenv` with config at `/etc/fw_env.config` (`/dev/mtd2`) |
| Write config to misc_rw | `libcmm.so` `oal_sys_writeAppFlash` |
| Start/stop/restart any vendor daemon | `cos` process supervision |

### 4.2 What the current GTPR/GDPR credentials can access

| Capability | Status |
|---|---|
| Read-only GTPR API (gl/go) | PROVEN-LIVE |
| `DEV2_WIFI_APDEV_ASSOCDEV` observation | PROVEN-LIVE |
| `so` (set object) — modifies config in memory | PROVEN-LIVE |
| `ACT_SAVE_CFG` — persists config to flash | PROVEN-LIVE |
| `DEV2_SYS_CFG` read | DENIED (errorcode 9003) |
| Start telnetd via `so` + `ACT_SAVE_CFG` | DISPROVEN (port 23 stays closed) |
| Start telnetd via `oal_setTelnetd` (OID 0xbd30) | NOT REACHABLE (separate dispatch entry, no known GTPR trigger) |
| Write arbitrary files to misc_rw | NO (no GTPR file-write operation) |
| Execute arbitrary commands | NO (no GTPR shell/exec operation) |
| Modify rootfs | NO (no GTPR filesystem operation) |
| Modify U-Boot env | NO (no GTPR boot-env operation) |
| Upload/replace firmware | NO (cloud upgrade requires TP-Link cloud auth + RSA signature) |
| Call `rcsHook` | NO (not wired into any accessible path) |

### 4.3 What has actually been PROVEN LIVE

```
EX520 DISCOVERY              = PROVEN-LIVE
IPv6 LINK-LOCAL              = PROVEN-LIVE
HTTP/80                      = PROVEN-LIVE
GTPR/GDPR                    = PROVEN-LIVE
AUTHENTICATION               = PROVEN-LIVE
gl/go (read)                 = PROVEN-LIVE
DEV2_WIFI_APDEV_ASSOCDEV     = PROVEN-LIVE
so (set config in memory)    = PROVEN-LIVE
ACT_SAVE_CFG (persist config) = PROVEN-LIVE
Telnet start via so          = DISPROVEN-LIVE
Arbitrary execution          = UNPROVEN
File write to persistent storage = UNPROVEN
rcsHook invocation           = UNPROVEN (orphaned in firmware)
```

---

## 5. Unknowns

Only the remaining high-value unknowns:

1. **Is the rootfs mounted read-only or read-write?**
   Kernel bootargs have no `rootflags=ro`. UBIFS default mount mode
   depends on kernel config. Cannot determine statically without
   live `mount` output. **Moot for deployment**: even if rw,
   modifying rootfs is firmware modification (outside safety
   boundary), and rcS does not execute user content from it.

2. **Is `oal_setTelnetd` (OID 0xbd30) reachable via any GTPR
   operation?**
   Phase 14.7 showed it is in a separate dispatch entry from the
   Telnet config SET handler. No known GTPR operation triggers it.
   If reachable, it would provide a telnet shell (and thus arbitrary
   execution), but this would be **service enablement via an
   undocumented dispatch path**, not a legitimate extension
   mechanism. Investigating it further would require either
   brute-forcing OIDs (stopped per Phase 14.7) or live
   experimentation with mutation operations (outside read-only
   safety boundary).

3. **Could the TP-Link cloud push a command that reaches
   `util_exec_system`?**
   `cloud_https` and `cloud_client` have `util_exec_system`, but the
   cloud protocol requires TP-Link cloud authentication and is
   designed for firmware upgrade, not arbitrary command execution.
   Not investigable without TP-Link cloud credentials.

---

## 6. Detectic Recommendation

### **External sensor deployment**

```
EX520 (RF observation)
       │
       │ DEV2_WIFI_APDEV_ASSOCDEV
       │ via GTPR/GDPR IPv6 link-local API
       │ (PROVEN-LIVE)
       ▼
Detectic external host
  (polls EX520, normalizes events,
   aggregates locally, buffers)
       │
       │ HTTPS
       ▼
Detectic backend
  (storage, analytics, pattern engine)
```

**Why external, not router-side:**

1. **No legitimate execution path exists.** The firmware has no
   supported mechanism for user-deployed persistent binaries. The
   `rcS_hook` mechanism is orphaned. `misc_rw` is config-only. `cos`
   launches only fixed vendor daemons.

2. **The proven-live capability is RF observation via GTPR.**
   `DEV2_WIFI_APDEV_ASSOCDEV` returns associated Wi-Fi devices. This
   is the sensor data Detectic needs. It is already accessible
   read-only via the proven IPv6/GTPR path.

3. **Router-side execution would require firmware modification or
   undocumented dispatch exploitation.** Both are outside the
   read-only safety boundary and the project's "research before
   modification" principle.

4. **The external host can perform all required processing.**
   Normalization, aggregation, deduplication, pseudonymization,
   buffering, and HTTPS transmission to the backend can all run on
   an external host that polls the EX520.

5. **This matches the Detectic philosophy.** The router is a sensor,
   not the computing platform. Processing close to the source is
   achieved by the external host being on the same network segment.

---

## 7. Phase 14.9 Recommendation

### **Phase 14.9 — External Sensor Production Hardening**

Transition from research to production engineering of the proven
external sensor path:

1. **Production-grade GTPR polling client** — robust retry,
   session management, AES/RSA re-authentication, error recovery
   for the `DEV2_WIFI_APDEV_ASSOCDEV` observation loop.

2. **Event normalization & aggregation** — convert raw associated-
   device lists into structured Detectic events (first seen, last
   seen, presence duration, recurrence) on the external host.

3. **Buffering & HTTPS transmission** — local buffer on the
   external host with reliable delivery to the Detectic backend.

4. **Multi-sensor correlation foundation** — design the external
   host architecture to support multiple EX520 sensors reporting
   to the same backend.

5. **Operational monitoring** — sensor health, GTPR session
   status, observation rate, backend delivery confirmation.

Router-side execution investigation is **exhausted** under the
read-only/static analysis safety boundary. The verdict (D) is
conclusive: no legitimate router-side deployment path exists in
the current firmware without firmware modification or undocumented
dispatch exploitation.

---

## Appendix A: Evidence Index

| ID | Description | Classification |
|----|-------------|----------------|
| E-14.8-01 | `/etc/inittab`: single sysinit entry → `/etc/init.d/rcS`, BusyBox init (not procd) | PROVEN-STATIC |
| E-14.8-02 | `/etc/fstab`: `/var` is ramfs (not persistent), `/tmp` → `/var/tmp` | PROVEN-STATIC |
| E-14.8-03 | `rcS` mounts misc_rw as UBIFS read-write at `/var/run/misc/misc_rw` | PROVEN-STATIC |
| E-14.8-04 | `rcS` mounts misc_ro as UBIFS read-only (`-r`) at `/var/run/misc/misc_ro` | PROVEN-STATIC |
| E-14.8-05 | `rcS` starts only `cos &` and `cmmsyslogd &` — no other daemons, no hook iteration | PROVEN-STATIC |
| E-14.8-06 | `rcS` does NOT call `rcsHook`, does NOT iterate `/etc/rcS_hook/` | PROVEN-STATIC |
| E-14.8-07 | `/etc/rcS_hook/` is empty (only `.gitkeep`) | PROVEN-STATIC |
| E-14.8-08 | `rcsHook` binary has `doRcsHookExes()` that opens `/etc/rcS_hook/`, iterates files, executes via `util_exec_system` | PROVEN-STATIC |
| E-14.8-09 | `rcsHook` is NOT referenced by any other binary, script, or web page — ORPHANED | PROVEN-STATIC |
| E-14.8-10 | `libcmm.so` uses misc_rw for config data only (`%s/0x%08lX` file naming, `oal_sys_readCfgFlash`/`oal_sys_writeAppFlash`) | PROVEN-STATIC |
| E-14.8-11 | No binary references executing executable content from `/var/run/misc/misc_rw` | PROVEN-STATIC |
| E-14.8-12 | `cos` manages daemons from fixed rootfs paths only (httpd, telnetd, dropbear, cloud_https, cwmp, nrd, tmpd, etc.) | PROVEN-STATIC |
| E-14.8-13 | `cos` `checkAndRestartDetectProcess` is WAN online-detection supervision, NOT custom code execution | PROVEN-STATIC |
| E-14.8-14 | No `rc.local`, no cron/crontab, no procd service enumeration, no plugin directory | PROVEN-STATIC |
| E-14.8-15 | `/etc/downgrade_exe/` and `/etc/downgrade_xml/` are empty (only `.gitkeep`) | PROVEN-STATIC |
| E-14.8-16 | `firmware.sh` uses `rc.common` + `USE_PROCD=1` but procd is not the init system — orphaned | PROVEN-STATIC |
| E-14.8-17 | Cloud upgrade (`cloud_https`) downloads to `/var/tmp/` (RAM), verifies RSA, writes flash — full firmware replacement, not code execution | PROVEN-STATIC |
| E-14.8-18 | `INCLUDE_CLI_CMD_SH` NOT set — CLI shell command disabled | PROVEN-STATIC |
| E-14.8-19 | `INCLUDE_SEC_ALLOW_APP_ENABLE_SSH` NOT set — SSH app enable disabled | PROVEN-STATIC |
| E-14.8-20 | `INCLUDE_RUNTIME_DATA_SECTION` NOT set — runtime_data partition not used | PROVEN-STATIC |
| E-14.8-21 | Kernel bootargs: `ubi.mtd=ubi0 console=ttyS0,115200n1 loglevel=8` — no `rootflags=ro`, no `init=` override | PROVEN-STATIC |
| E-14.8-22 | `passwd.bak`: admin account with UID 0, shell `/bin/sh`, SHA-256 password hash | PROVEN-STATIC |
| E-14.8-23 | Hotplug.d scripts are all fixed vendor scripts — no user-controlled execution from persistent storage | PROVEN-STATIC |
| E-14.8-24 | `misc_rw` file `0x00300000` is userconfig data (rcS checks/copies it) — not an executable | PROVEN-STATIC |

## Appendix B: Storage Partition Map

| Partition | MTD/UBI | Mount point | Type | Persistent | Writable | Used for |
|-----------|---------|-------------|------|------------|----------|----------|
| kernelA | ubi0 vol 0 | — | raw | YES | NO (boot) | Linux kernel (active) |
| rootfsA | ubi0 vol 1 | / | UBIFS | YES | UNKNOWN | Root filesystem |
| kernelB | ubi0 vol 3 | — | raw | YES | NO (boot) | Linux kernel (backup) |
| rootfsB | ubi0 vol 4 | — | UBIFS | YES | UNKNOWN | Root filesystem (backup) |
| misc_ro | ubi1 | /var/run/misc/misc_ro | UBIFS | YES | NO (-r) | Manufacturing data |
| misc_rw | ubi2 | /var/run/misc/misc_rw | UBIFS | YES | YES | User config (0x00300000) |
| misc_rw_bak | ubi3 | /var/run/misc/misc_rw_bak | UBIFS | YES | YES | Config backup |
| misc_isp | ubi4 | /var/run/misc/misc_isp | UBIFS | YES | NO (-r) | ISP data |
| U-Boot env | mtd2 | — | raw | YES | via fw_setenv | Boot environment |
| /var | — | /var | ramfs | NO | YES | Runtime (RAM) |
| /tmp | — | /var/tmp (symlink) | ramfs | NO | YES | Runtime (RAM) |

## Appendix C: Hook/Extension Directories — Status

| Directory | Contents | Consumer | Status |
|-----------|----------|----------|--------|
| `/etc/rcS_hook/` | empty (`.gitkeep`) | `rcsHook` (orphaned) | DEAD — consumer not called |
| `/etc/downgrade_exe/` | empty (`.gitkeep`) | unknown | DEAD — no consumer found |
| `/etc/downgrade_xml/` | empty (`.gitkeep`) | unknown | DEAD — no consumer found |
| `/etc/hotplug.d/button/` | `00-button` (debug echo) | hotplug-call | ACTIVE — fixed vendor script |
| `/etc/hotplug.d/net/` | `20-wsplcd`, `30-hyd` | hotplug-call | ACTIVE — fixed vendor scripts |
| `/etc/hotplug.d/iface/` | 12 vendor scripts | hotplug-call | ACTIVE — fixed vendor scripts |
| `/etc/hotplug.d/usb/` | `10-usb` (LED update) | hotplug-call | ACTIVE — fixed vendor script |
| `/etc/init.d/` | rcS, rcS.model, firmware.sh, init_console.sh | inittab (rcS only) | PARTIAL — only rcS is called |
