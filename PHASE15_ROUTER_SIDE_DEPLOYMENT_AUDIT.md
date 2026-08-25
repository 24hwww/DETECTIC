# PHASE 15 — Router-Side Deployment Research Loop

## Detectic — TP-Link EX520V

**Date:** 2026-08-24
**Method:** Static firmware analysis + non-intrusive live network probing. No firmware, bootloader, rootfs, U-Boot, or router configuration modified. No reboot.
**Final classification:** **D — EXTERNAL SENSOR REQUIRED**

---

## 1. Deployment Boundary (Stage 15.0)

### 1.1 Live boundary check

| Capability | Evidence | Access method | Risk | Reversible? | Current status |
|------------|----------|---------------|------|-------------|----------------|
| IPv6 link-local | `fe80::3e6a:d2ff:fe5f:abc1 dev enp2s0 lladdr 3c:6a:d2:5f:ab:c1 router STALE` | Host `enp2s0` | None | N/A | PROVEN-LIVE |
| HTTP/80 | `curl` returned `200` | `http://[fe80::...%enp2s0]/` | None | N/A | PROVEN-LIVE |
| HTTPS/443 | `nc` succeeded | `https://[fe80::...%enp2s0]/` | Low if used | N/A | OPEN (not tested in depth) |
| SSH/22 | `nc -z` failed | — | N/A | N/A | CLOSED/UNREACHABLE |
| Telnet/23 | `nc -z` failed | — | N/A | N/A | CLOSED/UNREACHABLE |

### 1.2 Firmware boundary

| Item | Evidence | Status |
|------|----------|--------|
| misc_rw availability | `rcS` mounts `ubi2:misc_rw` at `/var/run/misc/misc_rw` | PROVEN-STATIC |
| misc_rw persistence | UBI volume on SPI NAND; `rcS` recreates if missing | PROVEN-STATIC |
| rootfs mount state | `CONFIG_TARGET_ROOTFS_SQUASHFS=y`, `inittab` uses `rcS` | PROVEN-STATIC (read-only) |
| /var persistence | `ramfs /var ramfs defaults` in `fstab`; RAM-backed | PROVEN-STATIC (non-persistent) |
| Writable persistent locations | `/var/run/misc/misc_rw`, `/var/run/misc/misc_rw_bak` only | PROVEN-STATIC |
| Available interpreters | `/usr/bin/lua5.1`, BusyBox shell | PROVEN-STATIC |
| Child-process launchers | `cos`, `httpd`, `nrd`, `cloud_client` use `util_exec_system`/`popen` internally | PROVEN-STATIC (vendor-internal) |
| Available dynamic libs | musl libc, `libcmm.so`, `libubus.so` | PROVEN-STATIC |
| Shell available on stock | `dropbearmulti`/`dropbear` compiled, `telnetd` likely BusyBox applet | COMPILED, BUT NOT REACHABLE |
| Vendor utilities | `do_backup.sh`, `do_upgrade.sh`, `phoenix.sh`, `diagTool`, `fw_printenv` | PROVEN-STATIC |
| Daemon lifecycle | `cos` manages fixed list of rootfs daemons | PROVEN-STATIC |
| Watchdog | `checkAndRestartDetectProcess` in `cos` is WAN-detect only | PROVEN-STATIC |
| Service control | `cos` data-model dispatch for vendor OIDs only | PROVEN-STATIC |
| Temporary execution | `/var/tmp`, `/tmp` are RAM-backed | PROVEN-STATIC |
| Config-driven execution | `so`/`ACT_SAVE_CFG` persist data-model blob only | PROVEN-STATIC |

---

## 2. Candidate Execution Surfaces (Stage 15.1)

| # | Candidate | Class | Why / Why not |
|---|-----------|-------|---------------|
| A | `cos` data-model lifecycle | **VENDOR-INTERNAL** | `cos` starts only fixed rootfs daemons (`httpd`, `telnetd`, `dropbear`, `nrd`, etc.) from hardcoded paths. No API or dispatch table entry for user-supplied daemons. |
| B | `rcS` / `inittab` boot chain | **NOT-USABLE** | `rcS` is read-only SquashFS. It sources only `rcS.model` and starts `cos`. `inittab` is `::sysinit:/etc/init.d/rcS` plus a serial `getty`. No user hook. |
| C | `/etc/rcS_hook/` + `rcsHook` binary | **ORPHANED** | `rcsHook` has `doRcsHookExes()` and can scan `/etc/rcS_hook/`, but `rcS` does not call it and the directory is empty. |
| D | `procd` + `ubus` | **VENDOR-INTERNAL / LIKELY INACTIVE** | `procd`, `ubus`, `ubusd` are compiled (`CONFIG_PACKAGE_*=y`), but `inittab` points to BusyBox `init` and `rcS` does not start `procd`/`ubusd`. `firmware.sh` is procd-style but not invoked by `rcS`. |
| E | `busybox crond` | **VENDOR-INTERNAL** | BusyBox includes `crond`/`crontab` applets, but `rcS` does not start a cron daemon and no `/etc/crontabs` exists. Could be started from a shell, but no shell is available. |
| F | `lua5.1` interpreter | **VENDOR-INTERNAL** | `/usr/bin/lua5.1` exists, but no documented user-accessible path to feed it a script. Used internally by `obuspa`/`jshn` etc. |
| G | `dropbear` / `telnetd` | **VENDOR-INTERNAL** | `dropbear`/`dropbearmulti` are compiled, `telnetd` is expected by `oal_setTelnetd`. Both require `cos`/init to start them; `so` on `DEV2_TELNET_CFG` does not start the service (Phase 14.6/14.7). |
| H | `cloud_client` / `cloud_https` / `cwmp` | **VENDOR-INTERNAL** | Built for TP-Link cloud / TR-069 firmware and command management. Requires cloud authentication and RSA-signed payloads; not a user deployment channel. |
| I | `do_upgrade.sh` / `firmware.sh` | **VENDOR-INTERNAL** | Firmware upgrade helpers with RSA/MD5 signature checks. Cannot be co-opted for arbitrary user binaries. `firmware.sh` is procd-style but not wired into boot. |
| J | `diagTool` / `tr143d` / `speedtest` / `ookla` | **VENDOR-INTERNAL** | Diagnostic / speed-test daemons; not extensible with user code. |
| K | `/etc/hotplug.d/*` | **VENDOR-INTERNAL** | Fixed vendor scripts triggered by kernel events. No user-writable hook directory or documented insertion point. |
| L | `backupcfg.bin` restore | **NOT-USABLE** | Stores/loads data-model config blob (`0x00300000`) only. Not an executable or application delivery format. |
| M | `init_console.sh` | **NOT-USABLE** | Reads U-Boot `console_tx/rx_control` and writes `/proc/tplink/console_control`. Pure console enablement, not code execution. |

### 2.1 Summary of execution-surface search

Every execution primitive in the rootfs (`util_exec_system`, `popen`, `fork`/`execvp`, `system()`) is used internally by vendor daemons to manage vendor components. None of them is wired to a user-supplied or `misc_rw`-resident executable. No `run-parts`, plugin directory, user application directory, or documented script field was found.

---

## 3. Persistence Candidate Matrix (Stage 15.2)

| Surface | Type | Writable | Persistent across reboot | Persistent across power loss | Intended for | Safe for Detectic? |
|---------|------|----------|--------------------------|------------------------------|--------------|--------------------|
| `/` rootfs | SquashFS | NO | N/A | N/A | Firmware | NO (read-only) |
| `/var` | ramfs | YES | NO | NO | Runtime | NO (non-persistent) |
| `/var/tmp` / `/tmp` | ramfs/tmpfs | YES | NO | NO | Temp files | NO (non-persistent) |
| `/var/log` | ramfs | YES | NO* | NO* | Logs | NO (non-persistent) |
| `/var/run/misc/misc_ro` | UBIFS | NO | YES | YES | Manufacturing data | NO (read-only) |
| `/var/run/misc/misc_rw` | UBIFS | YES | YES | YES | User config blob (`0x00300000`) | **POSSIBLE** (storage only; no app mechanism) |
| `/var/run/misc/misc_rw_bak` | UBIFS | YES | YES | YES | Config backup | **POSSIBLE** (storage only) |
| `/var/run/misc/misc_isp` | UBIFS | NO | YES | YES | ISP data | NO (read-only) |
| `/var/run/runtime_data` | UBIFS | YES (if compiled) | YES | YES | Runtime data (not enabled in this build: `INCLUDE_RUNTIME_DATA_SECTION` not set) | NOT ENABLED |
| `/etc` | SquashFS | NO | YES | YES | Read-only config | NO |

* Note: `logd` may persist logs until power loss; `/var` is ramfs per `fstab`.

### 3.1 Application storage suitability

- `misc_rw` is the **only** persistent, writable, user-accessible storage class.
- The firmware uses it only for the data-model blob `0x00300000` (`mfg_config.bin` copy on first boot).
- There is **no documented or supported mechanism** for the firmware to treat `misc_rw` as an application or executable directory.
- A static `aarch64-musl` binary can live there and execute if placed manually, but placement and execution require a shell or equivalent transfer channel.

---

## 4. Minimal Diagnostic Payload (Stage 15.3)

### 4.1 Purpose

Prove `DEPLOY → EXECUTE` only. The payload must:

1. Start.
2. Record timestamp, PID, PPID, runtime info.
3. Create one harmless heartbeat artifact.
4. Remain alive for a controlled period.
5. Exit cleanly.

It must **not** modify network, firewall, Wi-Fi, or router configuration.

### 4.2 Proposed payload

```sh
#!/bin/sh
# /var/run/misc/misc_rw/detectic/.probe/payload.sh
MARKER="/var/run/misc/misc_rw/detectic/.probe/payload_$(date +%s)_$$"
mkdir -p /var/run/misc/misc_rw/detectic/.probe
{
  echo "DETECTIC_PAYLOAD_START"
  echo "timestamp=$(date -Iseconds)"
  echo "pid=$$"
  echo "ppid=$(cat /proc/$$/ppid 2>/dev/null || echo unknown)"
  echo "uid=$(id -u)"
  echo "args=$0 $*"
  echo "hostname=$(hostname)"
  echo "uptime=$(cat /proc/uptime)"
} > "$MARKER"

# Heartbeat loop for 60 seconds
for _ in $(seq 1 60); do
  sleep 1
  date -Iseconds >> "$MARKER.heartbeat"
done

exit 0
```

### 4.3 Status

**Not deployed.** No safe file-transfer/execution channel was identified in this phase.

---

## 5. Execution Test (Stage 15.4)

**Result:** **EXECUTION NOT PROVEN**

No legitimate execution surface was identified. The payload was not transferred or invoked.

| Evidence ID | What was captured | Result |
|-------------|-------------------|--------|
| E-15.4-01 | Deployment location | Not identified — `misc_rw` lacks a supported execution trigger. |
| E-15.4-02 | Invocation mechanism | Not identified — `rcS`, `cos`, `procd`, `ubus`, `crond` are not configured to launch user binaries from `misc_rw`. |
| E-15.4-03 | PID | N/A (not tested) |
| E-15.4-04 | Process lifetime | N/A (not tested) |
| E-15.4-05 | Heartbeat | N/A (not tested) |
| E-15.4-06 | Router health | N/A (not tested) |

---

## 6. Persistence Test (Stage 15.5)

**Result:** **PERSISTENCE NOT PROVEN**

Because `EXECUTE` is unproven, persistence of an application payload cannot be tested. The `misc_rw` UBI volume is persistent by design, but placing a file there legitimately is currently not possible.

| Evidence ID | What was captured | Result |
|-------------|-------------------|--------|
| E-15.5-01 | File present after placement | N/A (not tested) |
| E-15.5-02 | File survives lifecycle event | N/A (not tested) |
| E-15.5-03 | Payload remains executable | N/A (not tested) |
| E-15.5-04 | Router remains healthy | N/A (not tested) |

---

## 7. Autostart Test (Stage 15.6)

**Result:** **AUTOSTART NOT PROVEN**

No startup hook that can be legitimately controlled by a user was found.

- `rcS` does not source `/var/run/misc/misc_rw`.
- `rcS` does not call `rcsHook`.
- `rcS` does not start `procd`/`ubusd`.
- `inittab` has no user hook.
- `cos` does not register user daemons.

Because `DEPLOY`, `EXECUTE`, and `PERSIST` are unproven, autostart was not tested.

| Evidence ID | What was captured | Result |
|-------------|-------------------|--------|
| E-15.6-01 | Startup trigger | Not found (no user-writable or user-configurable autostart path). |
| E-15.6-02 | Detectic PID | N/A |
| E-15.6-03 | Startup timestamp | N/A |
| E-15.6-04 | Router service health | N/A |
| E-15.6-05 | RF observation success | N/A |

---

## 8. Failure Isolation (Stage 15.7)

### 8.1 Design principle

If a router-side Detectic process is ever started, it must be a leaf process:

```
EX520 boot
├── rcS → cos → vendor services (WAN/LAN/WLAN/DHCP/DNS/NAT)
└── Detectic (started independently, after vendor services)
    └── failure must not affect cos or network services
```

### 8.2 Required isolation properties

| Failure | Expected EX520 behavior |
|---------|------------------------|
| Detectic exits | Router continues; no restart of vendor services. |
| Detectic crashes | `cos` and network services continue; Wi-Fi remains up. |
| Detectic storage missing | Router continues; Detectic cannot start but does not block boot. |
| Detectic cannot reach backend | Events buffer; router networking unchanged. |
| Detectic cannot reach GTPR | Detectic marks sensor offline; router management UI unaffected. |

### 8.3 Status

Not tested because no router-side execution was established. If a future authorized test achieves `EXECUTE`, the first fault-isolation tests should be: kill Detectic, remove `misc_rw/detectic`, and verify `ps | grep -E 'cos|httpd|dnsmasq|nrd'` remains healthy and Wi-Fi clients stay connected.

---

## 9. Maintainability / Update Architecture (Stage 15.8)

### 9.1 Recommended install tree

If `misc_rw` is ever safely writable:

```
/var/run/misc/misc_rw/detectic/
├── bin/
├── config/
├── data/
├── logs/
├── run/
├── state/
├── releases/
│   ├── v0.1.0/
│   ├── v0.2.0/
│   └── v0.3.0/
├── current -> releases/v0.3.0
├── previous -> releases/v0.2.0
└── version
```

### 9.2 Operations

| Operation | Script | Location |
|-----------|--------|----------|
| INSTALL | `install.sh` | `deploy/install.sh` |
| START | `start.sh` | `deploy/start.sh` |
| STOP | `stop.sh` | `deploy/stop.sh` |
| STATUS | `health.sh` / `launcher.sh status` | `deploy/health.sh` / `deploy/launcher.sh` |
| UPDATE | `update.sh` | `deploy/update.sh` |
| ROLLBACK | `rollback.sh` | `deploy/rollback.sh` |
| UNINSTALL | `remove.sh` | `deploy/remove.sh` |
| HEALTH | `health.sh` | `deploy/health.sh` |

### 9.3 Update sequence

```
Upload new release
→ validate SHA256
→ install side-by-side in releases/<version>
→ run health test (`detectic version`, `detectic status`)
→ switch `current` symlink
→ stop old, start new
→ verify heartbeat/logs
→ retain previous version for rollback
```

### 9.4 Current status

The `deploy/` package (`detectic-ex520.tar.gz`) already implements this architecture. It is ready to use **as soon as a safe file-transfer and shell channel is available**.

---

## 10. Real Sensor Integration (Stage 15.9)

### 10.1 Router-side sensor flow (if ever enabled)

```
EX520
├── vendor cos/httpd (unchanged)
└── Detectic (from misc_rw)
     ├── GTPR/GDPR client on 127.0.0.1 or [::1]?
     ├── poll DEV2_WIFI_APDEV_ASSOCDEV
     ├── normalize / pseudonymize
     ├── presence engine
     ├── local durable queue (JSONL or SQLite in misc_rw)
     └── optional HTTPS upload to backend
```

### 10.2 First integration step

Only after `DEPLOY → PERSIST → EXECUTE → AUTOSTART` is proven with the minimal payload.

1. Start `detectic sensor --once` locally.
2. Confirm `DEV2_WIFI_APDEV_ASSOCDEV` parses.
3. Confirm local buffer in `misc_rw`.
4. Add backend upload after local success is stable.

### 10.3 Current status

Not attempted. The external sensor (`python/detectic_sensor.py`) is already proven in Phase 14.9.

---

## 11. Security / Safety Assessment

### 11.1 What was not done

- No firmware modification.
- No bootloader or U-Boot env changes.
- No rootfs writes.
- No `rcS`/`inittab`/`cos` patching.
- No Telnet/SSH enabling attempts.
- No OID brute force.
- No credential extraction.
- No router reboot.
- No write to the live EX520.

### 11.2 What is safe to do later

If a future phase finds a legitimate shell or SCP channel:

- Write only to `/var/run/misc/misc_rw/detectic/`.
- Keep all config, state, and logs under that tree.
- Do not modify `/etc/init.d/`, `/etc/rcS_hook/`, `/etc/hotplug.d/`, or any read-only path.
- Do not start Detectic synchronously in `rcS`.
- Remove by deleting `/var/run/misc/misc_rw/detectic/` and stopping the process.

### 11.3 What is unsafe and must be authorized separately

| Mechanism | Why unsafe |
|-----------|------------|
| Patching `rcS`/`inittab` | Modifies rootfs; not reversible without reflash. |
| Replacing vendor binaries (`cos`, `httpd`) | Breaks firmware signature and router operation. |
| Enabling Telnet/SSH via undocumented OID | Undocumented data-model dispatch; may not start service; security posture change. |
| Brute-forcing OIDs to trigger `oal_setTelnetd` | Out of scope; undocumented exploit path. |
| Using `cloud_client`/`cwmp` for command execution | Requires TP-Link cloud credentials; not a user deployment path. |
| Using `nandwrite`/`ubiupdatevol`/`do_upgrade.sh` for arbitrary code | Raw flash / firmware operations; can brick device. |

---

## 12. Evidence Index

| ID | Description | Classification |
|----|-------------|----------------|
| E-15.0-01 | IPv6 link-local EX520 present on `enp2s0` | PROVEN-LIVE |
| E-15.0-02 | HTTP/80 returns 200 on IPv6 link-local | PROVEN-LIVE |
| E-15.0-03 | SSH/22 and Telnet/23 closed/unreachable | PROVEN-LIVE |
| E-15.0-04 | `misc_rw` UBI mounted at `/var/run/misc/misc_rw` by `rcS` | PROVEN-STATIC |
| E-15.0-05 | Rootfs is SquashFS (`CONFIG_TARGET_ROOTFS_SQUASHFS=y`) | PROVEN-STATIC |
| E-15.0-06 | `/var` is `ramfs` (non-persistent) | PROVEN-STATIC |
| E-15.1-01 | `cos` starts only fixed rootfs daemons | PROVEN-STATIC |
| E-15.1-02 | `rcS` does not source `misc_rw` or call `rcsHook` | PROVEN-STATIC |
| E-15.1-03 | `/etc/rcS_hook/` is empty; `rcsHook` not wired into boot | PROVEN-STATIC |
| E-15.1-04 | `procd`/`ubus` compiled but `inittab` uses BusyBox `init` and `rcS` does not start them | PROVEN-STATIC |
| E-15.1-05 | `busybox crond` applet exists but no `crontabs` dir and `rcS` does not start `crond` | PROVEN-STATIC |
| E-15.1-06 | `lua5.1` exists but no user-accessible script invocation | PROVEN-STATIC |
| E-15.1-07 | `dropbear`/`telnetd` compiled but not reachable; `so` on `DEV2_TELNET_CFG` does not start `telnetd` (Phase 14.7) | PROVEN-STATIC + PROVEN-LIVE |
| E-15.1-08 | `cloud_client`/`cloud_https`/`cwmp` require TP-Link cloud auth and RSA signatures | PROVEN-STATIC |
| E-15.1-09 | `do_upgrade.sh` / `firmware.sh` are for signed firmware only | PROVEN-STATIC |
| E-15.1-10 | `hotplug.d` scripts are fixed vendor scripts | PROVEN-STATIC |
| E-15.2-01 | `misc_rw` is the only persistent writable user partition | PROVEN-STATIC |
| E-15.2-02 | `misc_rw` used only for data-model blob `0x00300000` | PROVEN-STATIC |
| E-15.3-01 | Minimal harmless payload designed; not executed | DESIGN ONLY |
| E-15.4-01 | Execution test not performed: no candidate surface | BLOCKED |
| E-15.5-01 | Persistence test not performed: blocked by E-15.4-01 | BLOCKED |
| E-15.6-01 | Autostart test not performed: blocked by E-15.5-01 | BLOCKED |

---

## 13. Final Classification

| Capability | Status |
|------------|--------|
| DEPLOY | **NOT PROVEN** |
| PERSIST | **NOT PROVEN** |
| EXECUTE | **NOT PROVEN** |
| AUTOSTART | **NOT PROVEN** |
| MAINTAINABLE | **NO** (design exists, but cannot execute) |
| REVERSIBLE | **YES in design** (removes by `rm -rf /var/run/misc/misc_rw/detectic/`) |
| ISOLATED | **DESIGNED** (not proven) |
| ROUTER SAFE | **NOT TESTED** (no writes made) |

### Architecture decision

**D — EXTERNAL SENSOR REQUIRED**

The stock TP-Link EX520V firmware does not provide a legitimate, documented, safe `DEPLOY → PERSIST → EXECUTE → AUTOSTART` path for an externally supplied Detectic binary. All execution surfaces are either vendor-internal, orphaned, or would require firmware / rootfs / service modification. The proven, safe path remains the external sensor (`python/detectic_sensor.py`) polling the EX520 over the GTPR/GDPR IPv6 link-local API.

### Condition for reconsideration

If a future phase obtains explicit authorization and discovers a **new, supported, reversible** router-side deployment channel (e.g., a vendor firmware update adding an app directory, or a documented `op`/`cgi` endpoint for uploading and launching a signed/verified extension), this classification should be re-evaluated. Until then, do not proceed to RF optimization, distance calibration, multi-sensor correlation, or backend performance tuning for a router-side binary.
