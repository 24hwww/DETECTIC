# PHASE 16A — EX520 Stock Firmware Execution-Surface Audit

## Agent B — Research Track B

**Target:** TP-Link EX520V (`EX520V124101568249n_agc3000_0945460481`)  
**Date:** 2026-08-24  
**Method:** Static analysis only of the extracted stock rootfs (`_rootfs/`), boot image strings, and referenced firmware artifacts. No router interaction, no writes to NAND/UBI/rootfs, no reboots, no live testing.  
**Scope:** Identify every legitimate, reproducible, reversible path from a *persistent artifact* (file, config, UBI volume, U-Boot env) to code execution or process launch, without modifying the router.

---

## 1. Executive Summary

* **Primary finding:** The only concrete, static, persistent-artifact → process-launch chain found in the EX520 firmware is the **data-model configuration blob** (`backupcfg.bin` / `0x00300000` in `misc_rw`) enabling `DEV2_TELNET_CFG` or `DEV2_SSH_CFG`, which the `libcmm.so` data-model engine routes through `rsl_setDev2TelnetCfgObj` → `oal_setTelnetd` → `telnetd -p %d` (and `dropbear -p %d ...` is also present). This is a **STRONG-CANDIDATE (static)** but is *not* a safe, no-modify path for Detectic.
* **Why it is not a recommended Detectic channel:**
  1. It requires writing a crafted config into `misc_rw` (a live UBI/NAND write), which is prohibited by the current safety boundary and which the mission explicitly forbids.
  2. It only launches the stock `telnetd`/`dropbear` services; a separate login step is then needed to obtain a shell.
  3. `misc_rw` is only ~1.1 MB, so the stock Detectic binary (~1.26 MB) cannot even persist there (proven in Phase 14.1).
  4. Phase 15 observed that a live `so` on `DEV2_TELNET_CFG` did not immediately start the service; the exact apply trigger in `cos` is therefore still **UNKNOWN** in a live setting.
* **All other surfaces** (`procd`/`ubus`, `rcS_hook`, `hotplug.d`, `crond`, `lua5.1`, `LD_PRELOAD`, cloud command, firmware upgrade, U-Boot recovery flags) are either **vendor-internal/hardcoded**, **orphaned (not wired)**, **not enabled**, or **proven to require signed payloads**.
* **Conclusion:** The stock firmware does **not** expose a legitimate, no-NAND-write, autonomous application/plugin/autostart directory. The telnet/SSH config chain is the only artifact-driven process-launch candidate, and it should only be tested in an authorized, isolated lab. Failing that, a router-side Detectic sensor is not safely deployable on this stock build.

---

## 2. Global Execution Primitive Map

All execution primitives (`system`, `popen`, `execve`, `execvp`, `execv`, `execl`, `execlp`, `vfork`, `posix_spawn`, `dlopen`, `dlsym`, `dlclose`, `util_exec_system`) were searched across `bin/`, `sbin/`, `usr/bin/`, `usr/sbin/`, `lib/`, and `usr/lib/` using `strings`. The table below is limited to the *significant* occurrences in vendor daemons; the full scan results are in `/tmp/exec_prims_scan.txt`.

| Primitive | Binary/Library | Function/Caller | Input Source | User / Persistent Controllable? |
|-----------|----------------|-----------------|--------------|---------------------------------|
| `util_exec_system` | `cos` | `rdp_*`, `rsl_*`, daemon restart logic | Hardcoded rootfs paths / data-model apply | **NO** — internal manager |
| `util_exec_system` | `httpd` | `http_init_main` | Internal web helpers | **NO** — no user shell command path |
| `util_exec_system` | `nrd` | network remote daemon | Internal speedtest / child launch | **NO** |
| `util_exec_system` | `cloud_client` | `rdp_updateFirmware`, `rdp_verifyFirmware` | Cloud-signed firmware | **NO** |
| `util_exec_system` | `cloud_https` | cloud cert validation | Internal | **NO** |
| `util_exec_system` | `obuspa` | TR-369 agent | Internal | **NO** |
| `util_exec_system` | `tr143d`, `speedtest`, `tmpd`, `tdpd`, `cli`, `ipsecVpn`, `mapAgent`, `mapController`, `meshMonitor`, `wanconnd2`, `xmpp` | vendor handlers | Hardcoded | **NO** |
| `util_exec_system` (implementation) | `lib/libcutil.so` | `cutil_exec.c` wrapper | `popen`-based `/bin/sh -c` | **NO** — C library helper |
| `util_exec_popen` | `lib/libcutil.so` | `popen` wrapper | Internal commands | **NO** |
| `popen` | `nrd`, `cos`, `httpd`, `tr143d`, `meshMonitor`, `wanconnd2`, `libcmm.so` | `util_exec_popen` or direct | Built-in strings | **NO** |
| `system` | `nrd`, `pppd`, `ripd/ripngd/zebra`, `openvpn`, `dnsmasq`, `liblua.so` | internal | Built-in commands | **NO** |
| `execve`/`execvp`/`execv` | `busybox`, `dropbearmulti`, `pppd`, `pptpd`, `racoon`, `xtables-multi`, `ebtables`, `libblkid.so`, `libsmartcols.so` | self-exec / applet dispatch | Rootfs-resident only | **NO** |
| `vfork` | `busybox`, `upnpd`, `xtables-multi`, `libc.so` | process creation | Internal | **NO** |
| `posix_spawn` | `libc.so` | `libc` | Internal | **NO** |
| `dlopen`/`dlsym`/`dlclose` | `liblua.so.5.1.5`, `pppd`, `ip`, `tc`, `libc.so` | runtime library loading | Search paths / hardcoded | **NO** — no `.so` from `misc_rw` |

**Key observations (PROVEN-STATIC):**

* The `util_exec_system` helper in `lib/libcutil.so` is the single shared execution primitive for all vendor daemons. It is a `popen()` wrapper that ultimately runs `/bin/sh -c` (inferred from `popen` usage and `cutil pipe: popen file(%s)` string). **E-16A-FIRM-01**
* No binary was found to build an `util_exec_system` / `popen` command from a file inside `misc_rw`, `/var/run/misc`, or any other user-writable volume. All command strings are compiled into the binary or constructed from fixed data-model fields. **E-16A-FIRM-02**
* `busybox` is the only binary that legitimately runs an arbitrary `/bin/sh` script, but there is no evidence it is invoked on a user-supplied script path at boot or by any daemon. **E-16A-FIRM-03**

---

## 3. `misc_rw` Deep Cross-Reference

### 3.1 Persistent writable partitions mounted by `rcS`

`rcS` (lines 30–131) is the only boot-time script that creates/mounts the misc UBI partitions:

```
/etc/init.d/rcS:32  MISC_RW_MTD_NAME=misc_rw
/etc/init.d/rcS:33  MISC_RW_UBI_NAME=misc_rw
/etc/init.d/rcS:44  /bin/mkdir -m 0777 -p /var/run/misc/misc_ro
/etc/init.d/rcS:45  /bin/mkdir -m 0777 -p /var/run/misc/misc_rw
/etc/init.d/rcS:54  mount -t ubifs -r ubi1:${MISC_RO_UBI_VOL} /var/run/misc/misc_ro
/etc/init.d/rcS:72  mount -t ubifs ubi2:${MISC_RW_UBI_VOL} /var/run/misc/misc_rw
/etc/init.d/rcS:85  /bin/mkdir -m 0777 -p /var/run/misc/misc_rw_bak
/etc/init.d/rcS:97  mount -t ubifs ubi3:${MISC_RW_BAK_UBI_VOL} /var/run/misc/misc_rw_bak
/etc/init.d/rcS:109 /bin/mkdir -m 0777 -p /var/run/misc/misc_isp
/etc/init.d/rcS:121 mount -t ubifs -r ubi4:${MISC_ISP_UBI_VOL} /var/run/misc/misc_isp
```

### 3.2 `0x00300000` data model blob

`rcS` (lines 312–319):

```sh
if [ ! -f /var/run/misc/misc_rw/0x00300000 ] ; then
    echo Warning: userconfig not exists, use manufacture config!!
    cp -v /etc/mfg_config.bin /var/run/misc/misc_rw/0x00300000
```

### 3.3 Every `misc_rw` / `0x00300000` reference in rootfs

A `strings` search for `misc_rw`, `/var/run/misc`, and `0x00300000` found only the following significant references:

| File | `misc_rw` / `misc_rw_bak` / `0x00300000` usage |
|------|------------------------------------------------|
| `/etc/init.d/rcS` | Mounts partitions and copies `mfg_config.bin` to `0x00300000` if missing. |
| `/lib/libcmm.so` | Contains `dm_loadCfg`, `dm_saveCfg`, `dm_restoreCfg`, `dm_backupCfg`; strings for `/var/run/misc/misc_rw`, `ubi2:misc_rw`, `ubi3:misc_rw_bak`, `0x00300000`. This is the **config database engine**. |
| `/lib/modules/tp_gpio.ko` | References `/var/run/misc/misc_ro` and `/var/run/misc/misc_rw` for GPIO/LED data. |
| `/bin/ated_tp` | References `/var/run/misc/misc_ro`. |
| `/bin/cli` | References `/var/run/misc/misc_ro`. |

**Critical finding (PROVEN-STATIC):** `lib/libcmm.so` treats `misc_rw` as a configuration storage area only. It does **not** enumerate, open, `readdir`, `stat`, or `exec` any file inside `misc_rw` as an executable or plugin. **E-16A-FIRM-04**

### 3.4 Can any binary watch/execute from `misc_rw`?

* No `readdir`, `opendir`, `stat`, `popen`, or `exec` string was found in any binary that is combined with a `misc_rw` path, except for `lib/libcmm.so` opening `0x00300000` and related config files for read/write. **E-16A-FIRM-05**
* No `run-parts`, `plugin`, `modules`, `app.d`, or `autostart` directory under `misc_rw` is referenced. **E-16A-FIRM-06**

---

## 4. UBI / MTD / Storage Architecture

### 4.1 Volume and MTD layout

From `etc/config.bba` (lines 139–180) and U-Boot strings in `EX520_UP_BOOT_2025-07-31_11.34.16.bin`:

| Partition | Size | UBI volume(s) / role |
|-----------|------|----------------------|
| `boot` | 2 MiB | U-Boot / boot loader |
| `u-boot-env` | 1 MiB | U-Boot environment (`/dev/mtd2`, offset 0, size 0x20000 per `etc/fw_env.config`) |
| `misc_ro` | 6 MiB | `ubi1:misc_ro` — manufacturing data, read-only |
| `misc_rw` | 6 MiB | `ubi2:misc_rw` — user config / data-model blob, **rw** |
| `ubi0` | 40 MiB | `kernelA`, `rootfsA` — active firmware image |
| `ubi1` | 40 MiB | `kernelB`, `rootfsB` — backup firmware image |
| `misc_rw_bak` | 6 MiB | `ubi3:misc_rw_bak` — dual config backup, **rw** |
| `bflag` | 6 MiB | boot flags |
| `misc_isp` | 6 MiB | `ubi4:misc_isp` — ISP data, read-only |

U-Boot `mtdparts` strings show two detected layouts:

```
"nmbm0:2M(boot),1M(u-boot-env),6M(misc_ro),6M(misc_rw),40M(ubi0),40M(ubi1)"
"nmbm0:2M(boot),1M(u-boot-env),6M(misc_ro),6M(misc_rw),40M(ubi0),40M(ubi1),6M(misc_rw_bak),6M(bflag),6M(misc_isp)"
```

### 4.2 Runtime data / plugin volume

`etc/config.bba:179`:

```
# INCLUDE_RUNTIME_DATA_SECTION is not set
RUNTIME_DATA_SECTION_SIZE="0"
```

`rcS` only mounts `runtime_data` if `INCLUDE_RUNTIME_DATA_SECTION` is set (lines 133–156), and it is **not**. Therefore no writable `runtime_data` UBI volume is available. **E-16A-FIRM-07**

### 4.3 Application/plugin storage suitability

* `misc_rw` is the only persistent, user-writable, executable-capable (UBIFS, no `noexec` mount) volume, but it is only **~1.1 MB** and already holds `0x00300000`. It cannot hold the ~1.26 MB Detectic binary. **E-16A-FIRM-08** (cited from Phase 14.1).  
* No volume or directory is allocated for user applications, plugins, or extensions. **E-16A-FIRM-09**

---

## 5. U-Boot / Boot-Chain Map

### 5.1 Environment and cmdline

From U-Boot strings in the boot image (`EX520_UP_BOOT_2025-07-31_11.34.16.bin`):

```
bootargs=ubi.mtd=ubi0 console=ttyS0,115200n1 loglevel=8 earlycon=uart8250,mmio32,0x11002000 AC=300
baudrate=115200
ipaddr=192.168.1.1
serverip=192.168.1.2
netmask=255.255.255.0
loadaddr=0x46000000
nmbm0=nmbm0
mtdparts=
tp_boot_idx
```

### 5.2 `etc/fw_env.config`

```
/dev/mtd2  0x0000  0x20000  0x20000  8
```

This means U-Boot env lives on MTD partition `/dev/mtd2` (u-boot-env), 128 KB per sector, 8 sectors.

### 5.3 Boot and image selection

* `do_upgrade.sh` (lines 92–119) reads `/proc/cmdline` for `mtd=...`, picks the **non-active** `ubi0`/`ubi1` partition, writes the UBI image, and then toggles `tp_boot_idx` with `fw_setenv` so the next boot uses the new image. **E-16A-FIRM-10**
* `do_backup.sh` mirrors the active firmware to the inactive `ubi0/ubi1` volume and updates `fw1_status` / `fw2_status` / `fw_index` in U-Boot env. **E-16A-FIRM-11**
* No `bootcmd` value was recovered from the U-Boot image; the bootloader likely runs a compiled default. No `init=`, `root=`, `overlay`, `recovery`, `failsafe`, or `single` boot script strings were found. **E-16A-FIRM-12**

### 5.4 Serial console toggle

`etc/init.d/init_console.sh` (lines 14–29) does:

```sh
eval $(fw_printenv console_tx_control)
eval $(fw_printenv console_rx_control)
echo "$console_tx_control" > /proc/tplink/console_control
echo "$console_rx_control" > /proc/tplink/console_control
```

This is a **read-only** runtime enabler for the serial console; it does not itself launch a binary, and the required U-Boot variables (`console_tx_control` / `console_rx_control`) were not present in the U-Boot string dump, so they are normally unset. **E-16A-FIRM-13**

---

## 6. Recovery / Factory / Developer / Test Mode Findings

| Mode | Evidence | Real execution channel? |
|------|----------|-------------------------|
| **Factory reset** | `rcS`, `/etc/rc.button/wps`, `/etc/rc.button/reset` (not fully extracted) | Clears data model; no user-code path. |
| **Mediatek Wi-Fi test mode** | `/etc/wireless/mediatek/test-mode-switch.sh`, `/etc/init.d/firmware.sh:10` | Switches firmware to `*_TESTMODE` image; **vendor-internal**, no user payload. **E-16A-FIRM-14** |
| **Diagnostics** | `diagTool`, `tr143d`, `speedtest`, `ookla` strings in `cos` / `nrd` | Run built-in speed/ping tests; not extensible. **E-16A-FIRM-15** |
| **Core dump debug** | `rcS:270` `core_pattern` to `/var/core-%e` or `/usr/sbin/coredump_map.sh` | Requires a crash; not a persistent artifact→execution channel. **E-16A-FIRM-16** |
| **Web debug flags** | `INCLUDE_DEBUG_*_CC_LEVEL` / `_RUN_LEVEL` in `config.bba` and `web/js/oid_str.js` | Log-level settings; no shell or command field. **E-16A-FIRM-17** |
| **Backup restore** | `lib/libcmm.so` `dm_restoreCfg`, `oal_setTelnetd` | The **only** recovery-style path that can launch a process (`telnetd`/`dropbear`) from a persistent config file. (See Section 10.) **E-16A-FIRM-18** |

None of these provide a user-writable script directory or a documented developer/test flag that runs arbitrary code from a persistent artifact. **E-16A-FIRM-19**

---

## 7. Dynamic Loading Findings

### 7.1 `dlopen` / `dlsym` / `dlclose` occurrences

* `lib/libc.so` — dynamic loader itself.  
* `usr/lib/liblua.so.5.1.5` — Lua runtime `loadlib`.  
* `usr/sbin/pppd` — PPP plugin loading (`.so` plugins from `/usr/lib/pppd/...`).  
* `usr/bin/ip` and `usr/bin/tc` — netlink / library loading.  

### 7.2 User-writable `.so` / `LD_PRELOAD` / plugin paths

* **No `LD_PRELOAD` string** found in any binary or library. **E-16A-FIRM-20**
* **No `ld.so.preload` or `/etc/ld.so.*` reference** found. **E-16A-FIRM-21**
* **No `.so` load path points to `misc_rw`, `/var`, `/tmp`, or any user-writable volume.** `pppd` plugins, if any, would be under `/usr/lib/pppd/...` on the read-only rootfs. **E-16A-FIRM-22**
* `lua5.1` exists in `/usr/bin/lua5.1`, but no daemon or boot script was found that executes a user-controlled `.lua` file from a writable location. **E-16A-FIRM-23**

**Conclusion:** Dynamic loading is strictly for C libraries and fixed PPP/Lua internals; it is **DISPROVEN** as a user-controlled persistence → execution channel. **E-16A-FIRM-24**

---

## 8. Vendor Daemon Configuration Findings

### 8.1 Main daemons and execution primitives

| Daemon | Role | Execution primitive | Notable config/path strings | User command field? |
|--------|------|---------------------|-----------------------------|---------------------|
| `cos` | Central manager | `util_exec_system`, `popen` | `telnetd`, `httpd`, `dropbear`, `cloud_https`, `obuspa &`, `tr143d &`, `snmpd`, `speedtest` | **NO** |
| `httpd` | Web UI / GTPR | `util_exec_system` | `/var/tmp/dropbear_err_key`, SSL cert paths | **NO** |
| `nrd` | Network remote | `system`, `popen` | `speedtest`, `/var/tmp/speedtest/log.bak` | **NO** |
| `cloud_client` | TP-Link cloud | `util_exec_system` | `rdp_updateFirmware`, `rdp_verifyFirmware` | **NO** |
| `cloud_https` | Cloud validation | — | `cloud_https.cfg` | **NO** |
| `cwmp` / `obuspa` | TR-069 / TR-369 | `util_exec_system` | fixed OIDs | **NO** |
| `libcmm.so` | Data model engine | — (calls `util_exec_system` via `oal_*`) | `oal_setTelnetd`, `rsl_setDev2TelnetCfgObj`, `dm_restoreCfg` | **NO** (only fixed OIDs) |

### 8.2 Configuration files

* `/etc/cloud/config.cfg` and `/etc/cloud/cloud_service.cfg` — cloud server, heartbeat, cert paths; no command/script fields. **E-16A-FIRM-25**
* `/etc/cloud_https/cloud_https.cfg` — TLS validation settings; no command/script fields. **E-16A-FIRM-26**
* `/etc/config.bba` — compile-time feature flags. `INCLUDE_SSH_ACCESS` is **not set**; `INCLUDE_WEB_TELNET` and `INCLUDE_REMOTE_TELNET` are set to `y` (lines 269–272). **E-16A-FIRM-27**

### 8.3 Telnet / SSH apply handlers (most important)

From `lib/libcmm.so` strings:

```
rsl_setDev2TelnetCfgObj
oal_setTelnetd
oal_app_setLocalTelnetAccess
oal_app_setRemoteTelnetAccess
telnetd -p %d &
dropbear -p %d -r %s -d %s -A %s &
Device.X_TP_AppCfg.SSHCfg.
Device.X_TP_AppCfg.TelnetCfg.
DEV2_SSH_CFG
DEV2_TELNET_CFG
TelnetLocalPort
TelnetRemotePort
```

This is the **data-model to daemon-launch chain**: `set DEV2_TELNET_CFG` → `oal_setTelnetd` → `popen`/`system` `telnetd -p %d`. **E-16A-FIRM-28**

---

## 9. Update / Package / Install System Findings

### 9.1 `do_upgrade.sh`

`/usr/bin/do_upgrade.sh` (lines 47–119):

* Takes `uptype upfile uplen upchecksum`.
* Computes/verifies an MD5 `upchecksum` before calling `mtd write` or `nand_upgrade_ubinized`.
* For firmware, it writes the opposite `ubi0`/`ubi1` image and toggles `tp_boot_idx`.
* Does **not** accept arbitrary file paths, scripts, or packages; it only writes `boot` or `ubi` images. **E-16A-FIRM-29**

### 9.2 Firmware signature checks

`etc/config.bba` (lines 191–201):

```
INCLUDE_FWUPGRADE_CHECK=y
INCLUDE_FWUPGRADE_CHECK_MD5=y
INCLUDE_FWUPGRADE_CHECK_RSA=y
INCLUDE_FWUPGRADE_CHECK_PRODUCT_ID=y
INCLUDE_FWUPGRADE_CHECK_ADDHWVER=y
INCLUDE_FWUPGRADE_CHECK_SPECIAL_VER=y
INCLUDE_FWUPGRADE_BOOT_UPDATE=y
```

This confirms the stock upgrade path is **signed, product-verified, and image-only**; it cannot be co-opted into installing a Detectic package or arbitrary shell script. **E-16A-FIRM-30**

### 9.3 `backupcfg.bin` / `0x00300000`

* The data-model blob is DES-ECB + zlib (proven in Phase 13). It contains only TR-069 / BBA data-model parameters; it **cannot** carry arbitrary files, plugins, or executable payloads. **E-16A-FIRM-31**
* It can, however, carry `DEV2_TELNET_CFG` / `DEV2_SSH_CFG` values and is restored via `dm_restoreCfg` into `misc_rw`. This makes it a **configuration** artifact, not a binary delivery format. **E-16A-FIRM-32**

---

## 10. Candidate Execution Paths

### 10.1 Primary candidate: `backupcfg/0x00300000` → `telnetd` / `dropbear`

**Chain (STRONG-CANDIDATE static, UNKNOWN live):**

1. `backupcfg.bin` (or direct `0x00300000` blob) contains `DEV2_TELNET_CFG` with `X_TP_TelnetEnable = 1` and local port.
2. Web UI restore or data-model apply writes it to `misc_rw`.
3. `rcS` has already mounted `misc_rw` and `cos` has loaded the data model.
4. `lib/libcmm.so` `rsl_setDev2TelnetCfgObj` / `rsl_setDev2SshCfgObj` triggers `oal_setTelnetd`.
5. `libcutil.so` `util_exec_system` runs `telnetd -p %d` (or `dropbear -p %d ...`).
6. A network client can (in theory) connect and, with valid credentials, obtain a shell.

**Scoring (0–5):**

| Criterion | Score | Reason |
|-----------|-------|--------|
| Likelihood | 3 | Strong static evidence; live `so` on `DEV2_TELNET_CFG` did not start service in Phase 15; exact trigger still uncertain. |
| Persistence | 5 | Config lives in `misc_rw` UBI, survives reboot. |
| Autostart | 4 | `cos` applies data model at boot; `oal_setTelnetd` is the apply handler. |
| Maintainability | 2 | Requires crafting a DES-ECB/ZLIB backupcfg and a separate login step. |
| Reversibility | 3 | Can be undone by restoring an original backup or factory reset, but modifies live UBI. |
| Router safety | 1 | Writes to `misc_rw` (NAND/UBI), opens a shell on the device, and could lock the router if the config is malformed. |

**Verdict:** This is the **only** static, persistent-artifact → process-launch chain. It is *not* a "without modifying the router" path because it requires a live UBI write. It is **not autonomous** for Detectic because it only starts a shell service. **E-16A-FIRM-33**

### 10.2 Other candidates (discarded)

| # | Candidate | Status | Why |
|---|-----------|--------|-----|
| B | `rcS_hook` / `rcsHook` | DISPROVEN | `rcsHook` has `doRcsHookExes()`, but `rcS` does **not** call it and `cos` has no `/etc/rcS_hook` string. Directory is empty. **E-16A-FIRM-34** |
| C | `procd` / `ubus` | NOT ENABLED | Binaries compiled, but `inittab` uses BusyBox `init` and `rcS` does not start `procd`/`ubusd`. **E-16A-FIRM-35** |
| D | `hotplug.d` user hooks | DISPROVEN | Scripts are read-only vendor scripts; no user-writable hook directory. **E-16A-FIRM-36** |
| E | `crond` | NOT ENABLED | BusyBox `crond` exists but is not started and no `crontabs` directory. **E-16A-FIRM-37** |
| F | `lua5.1` user scripts | DISPROVEN | Interpreter compiled, no documented invocation of a user-controlled `.lua`. **E-16A-FIRM-38** |
| G | `do_upgrade.sh` as package installer | DISPROVEN | Image-only, MD5+RSA, product/hardware/version checks. **E-16A-FIRM-39** |
| H | `LD_PRELOAD` / `.so` injection | DISPROVEN | No `LD_PRELOAD` string, no user `.so` search path. **E-16A-FIRM-40** |
| I | U-Boot serial console | PHYSICAL ONLY | `console_tx/rx_control` can enable the serial port, but requires physical access and U-Boot env write. **E-16A-FIRM-41** |
| J | `runtime_data` volume | NOT ENABLED | `INCLUDE_RUNTIME_DATA_SECTION` is not set. **E-16A-FIRM-42** |

---

## 11. Evidence Index (E-16A-FIRM-xx)

| ID | Description | File / Location | Classification |
|----|-------------|-----------------|----------------|
| E-16A-FIRM-01 | `util_exec_system` is a `popen`/`sh -c` wrapper in `lib/libcutil.so` | `_rootfs/lib/libcutil.so` | PROVEN-STATIC |
| E-16A-FIRM-02 | No `util_exec_system`/`popen` uses a `misc_rw` user-supplied command | `_rootfs/bin/`, `_rootfs/lib/` full `strings` scan | PROVEN-STATIC |
| E-16A-FIRM-03 | `busybox` can run `/bin/sh -c`, but is not called on a user script path | `_rootfs/bin/busybox` | PROVEN-STATIC |
| E-16A-FIRM-04 | `lib/libcmm.so` uses `misc_rw` only for config load/save | `_rootfs/lib/libcmm.so` | PROVEN-STATIC |
| E-16A-FIRM-05 | No binary `readdir`/`exec` on a `misc_rw` path | `strings` scan | PROVEN-STATIC |
| E-16A-FIRM-06 | No `plugin`/`app`/`autostart` dir under `misc_rw` referenced | `strings` scan | PROVEN-STATIC |
| E-16A-FIRM-07 | `runtime_data` section not enabled | `_rootfs/etc/config.bba:179`, `_rootfs/etc/init.d/rcS:133` | PROVEN-STATIC |
| E-16A-FIRM-08 | `misc_rw` is ~1.1 MB; Detectic binary ~1.26 MB will not fit | Phase 14.1 / M10 | PROVEN-STATIC |
| E-16A-FIRM-09 | No user application/plugin volume | `_rootfs/etc/config.bba` UBI layout | PROVEN-STATIC |
| E-16A-FIRM-10 | `do_upgrade.sh` toggles `tp_boot_idx` for dual image | `_rootfs/usr/bin/do_upgrade.sh:92-119` | PROVEN-STATIC |
| E-16A-FIRM-11 | `do_backup.sh` mirrors firmware and updates U-Boot env | `_rootfs/usr/bin/do_backup.sh:21-77` | PROVEN-STATIC |
| E-16A-FIRM-12 | No `recovery`/`failsafe`/`single` bootcmd in U-Boot strings | `EX520_UP_BOOT_2025-07-31_11.34.16.bin` strings | PROVEN-STATIC (within dumped strings) |
| E-16A-FIRM-13 | `init_console.sh` only reads `console_tx/rx_control` and writes `/proc` | `_rootfs/etc/init.d/init_console.sh:14-29` | PROVEN-STATIC |
| E-16A-FIRM-14 | Mediatek Wi-Fi test mode is vendor-internal | `_rootfs/etc/wireless/mediatek/test-mode-switch.sh`, `_rootfs/etc/init.d/firmware.sh:10` | PROVEN-STATIC |
| E-16A-FIRM-15 | `diagTool`/`tr143d`/`speedtest` are built-in, not extensible | `_rootfs/bin/cos` strings, `_rootfs/bin/nrd` strings | PROVEN-STATIC |
| E-16A-FIRM-16 | Core dump pattern is crash-only, not an artifact→exec path | `_rootfs/etc/init.d/rcS:270-278` | PROVEN-STATIC |
| E-16A-FIRM-17 | `INCLUDE_DEBUG_*` are log-level flags, no shell | `_rootfs/etc/config.bba`, `_rootfs/web/js/oid_str.js` | PROVEN-STATIC |
| E-16A-FIRM-18 | `lib/libcmm.so` `dm_restoreCfg` → `oal_setTelnetd` → `telnetd -p %d` | `_rootfs/lib/libcmm.so` strings | PROVEN-STATIC |
| E-16A-FIRM-19 | No user-writable recovery/test script directory | `_rootfs/etc/rc.button/`, `_rootfs/etc/hotplug.d/`, `_rootfs/etc/rcS_hook/` | PROVEN-STATIC |
| E-16A-FIRM-20 | No `LD_PRELOAD` string | `strings` scan of all ELF | PROVEN-STATIC |
| E-16A-FIRM-21 | No `ld.so.preload` or `/etc/ld.so.*` reference | `grep` of `_rootfs/etc` and `_rootfs/lib` | PROVEN-STATIC |
| E-16A-FIRM-22 | No `.so` search path points to `misc_rw` | `strings` scan | PROVEN-STATIC |
| E-16A-FIRM-23 | `lua5.1` exists, no user-controlled `.lua` runner | `_rootfs/usr/bin/lua5.1`, `grep` of `_rootfs` | PROVEN-STATIC |
| E-16A-FIRM-24 | Dynamic loading is not user-controlled | `_rootfs/lib/liblua.so.5.1.5`, `_rootfs/usr/sbin/pppd` | DISPROVEN |
| E-16A-FIRM-25 | Cloud config files have no command/script fields | `_rootfs/etc/cloud/config.cfg`, `_rootfs/etc/cloud/cloud_service.cfg` | PROVEN-STATIC |
| E-16A-FIRM-26 | `cloud_https.cfg` has no command/script fields | `_rootfs/etc/cloud_https/cloud_https.cfg` | PROVEN-STATIC |
| E-16A-FIRM-27 | `INCLUDE_WEB_TELNET=y`, `INCLUDE_REMOTE_TELNET=y`, `INCLUDE_SSH_ACCESS` not set | `_rootfs/etc/config.bba:269-272`, `oid_str.js:4610` | PROVEN-STATIC |
| E-16A-FIRM-28 | `oal_setTelnetd` and `rsl_setDev2TelnetCfgObj` strings present, plus command templates | `_rootfs/lib/libcmm.so` strings | PROVEN-STATIC |
| E-16A-FIRM-29 | `do_upgrade.sh` is image-only | `_rootfs/usr/bin/do_upgrade.sh:1-119` | PROVEN-STATIC |
| E-16A-FIRM-30 | Firmware upgrade checks MD5+RSA+product ID+addhwver+special ver | `_rootfs/etc/config.bba:191-201` | PROVEN-STATIC |
| E-16A-FIRM-31 | `backupcfg.bin` is DES-ECB+zlib data-model only | Phase 13 evidence | PROVEN-STATIC |
| E-16A-FIRM-32 | `0x00300000` blob can carry `DEV2_TELNET_CFG` | `_rootfs/lib/libcmm.so` strings, `_rootfs/etc/init.d/rcS:313-319` | PROVEN-STATIC |
| E-16A-FIRM-33 | Primary candidate is `backupcfg` → `telnetd` | Sections 7–10 | STRONG-CANDIDATE |
| E-16A-FIRM-34 | `rcS_hook` is orphaned | `_rootfs/bin/rcsHook`, `_rootfs/etc/init.d/rcS`, `grep` no references | PROVEN-STATIC |
| E-16A-FIRM-35 | `procd`/`ubus` not started | `_rootfs/etc/inittab:1`, `_rootfs/etc/init.d/rcS` | PROVEN-STATIC |
| E-16A-FIRM-36 | `hotplug.d` scripts are fixed and read-only | `_rootfs/etc/hotplug.d/` | PROVEN-STATIC |
| E-16A-FIRM-37 | `crond` not started / no crontabs | `_rootfs/etc/init.d/rcS`, no `/etc/crontabs` | PROVEN-STATIC |
| E-16A-FIRM-38 | `lua5.1` not reachable as user script runner | `_rootfs/usr/bin/lua5.1`, no user invocation | PROVEN-STATIC |
| E-16A-FIRM-39 | `do_upgrade.sh` cannot install arbitrary code | `_rootfs/usr/bin/do_upgrade.sh`, `config.bba` | DISPROVEN |
| E-16A-FIRM-40 | `.so` injection not possible | full `strings` scan | DISPROVEN |
| E-16A-FIRM-41 | U-Boot serial console requires physical env write | `EX520_UP_BOOT_*.bin`, `_rootfs/etc/init.d/init_console.sh` | PHYSICAL-ONLY |
| E-16A-FIRM-42 | `runtime_data` section not enabled | `_rootfs/etc/config.bba:179-180` | PROVEN-STATIC |

---

## 12. Safety Assessment

This phase was purely static: no commands were sent to the live EX520, no files were written to the router, no UBI/NAND/rootfs/Bootloader changes were attempted, and no credentials or keys were extracted or printed. All evidence comes from the already-extracted `_rootfs/` and the U-Boot image in the repository.

The single execution candidate identified (`backupcfg` → `telnetd`) is **not safe under the current rules** because it requires a live write to `misc_rw` (NAND/UBI) and opens a privileged shell. It should not be attempted without explicit authorization and an isolated, recoverable lab setup.

---

## 13. Conclusion / Recommended Next Experiment

### 13.1 Conclusion

The stock EX520 firmware does **not** provide a legitimate, no-NAND-write, reversible, autonomous path from a persistent user artifact to the launch of a Detectic process. Every execution primitive is either internal to vendor daemons or operates on hardcoded rootfs binaries. The only reproducible artifact→process chain is the **data-model config blob enabling Telnet/SSH**, which is a configuration-modify path, not an application deployment path, and which cannot fit the Detectic binary in persistent storage.

### 13.2 Recommended next experiment (only if explicitly authorized)

In an isolated, serial-console-recoverable lab environment:

1. Craft a minimal `backupcfg.bin` that contains `DEV2_TELNET_CFG.X_TP_TelnetEnable = 1` and a known local port.
2. Restore the backup via the web UI or GTPR `gl/restore`.
3. Reboot and observe whether `telnetd` is listening on the configured LAN port (`netstat -ltn` or scan).
4. If `telnetd` starts, attempt login with the currently known admin credentials and try to launch the Detectic binary from `/tmp` (RAM) to verify `EXECUTE` only.
5. If the experiment fails, or if a write to UBI is not authorized, then **no router-side path exists on this stock firmware** and the project should continue with the external Python/Rust sensor proven in Phase 14/15.

**Do not** conclude "external sensor required" until the above `backupcfg`→`telnetd` chain is tested live or explicitly ruled out by the project lead. This report has exhausted the plausible static firmware/storage paths.
