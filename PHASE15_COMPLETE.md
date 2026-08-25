# PHASE 15 — COMPLETE

## Router-Side Deployment Path Classification

**Date:** 2026-08-24
**Status:** **BLOCKED**
**Final Decision:** **C — Persistent execution is unavailable on the stock EX520; use external sensor.**

---

## 1. Methodology

This phase followed the Phase 15 charter:

- Re-investigated the EX520 firmware boot chain, init scripts, storage layout, and vendor daemons.
- Re-used the existing extracted rootfs (`_rootfs/`). No firmware binaries, bootloader, kernel, or `backupcfg.bin` were modified.
- Performed a non-intrusive live network check: IPv6 link-local neighbor, HTTP/HTTPS reachability, and TCP port scan for 22/23/80/443.
- Did **not** enable Telnet, SSH, or any router service; did **not** write files to the EX520; did **not** reboot or fuzz OIDs.
- Cross-checked findings against prior reports and the current deploy package under `/home/soporte24hwww/Documentos/Repositorios/detectic/deploy/`.

---

## 2. Executive Verdict

No legitimate, least-invasive path for `DEPLOY → PERSIST → EXECUTE → AUTOSTART` exists on the stock TP-Link EX520V as currently understood.

The EX520 exposes a proven, read-only GTPR/GDPR management path over IPv6 link-local HTTP/80 and a persistent writable UBI partition (`misc_rw`), but there is **no supported mechanism** to:

1. Transfer a Detectic binary onto the router through the available interfaces (GTPR is read-only; SSH/Telnet are closed; no documented file-upload API).
2. Execute an externally supplied binary without a shell or firmware modification.
3. Autostart an externally supplied process across reboots without modifying `rcS`, `inittab`, `cos`, or the read-only rootfs.

Therefore, the immediate deployable Detectic architecture remains the **external sensor** proven in Phase 14.9.

---

## 3. Candidate Mechanism Review

| Priority | Candidate | Evidence | Verdict |
|----------|-----------|----------|---------|
| 1 | Vendor-supported app/service mechanism | GTPR/GDPR API is read-only (`gl`/`go`) and has no documented `op` for file transfer or third-party app execution. | **Not available.** |
| 2 | Vendor daemon lifecycle hosting Detectic | `cos` starts only fixed rootfs paths (`httpd`, `telnetd`, `dropbear`, `nrd`, `cloud_https`, etc.); hardcoded. | **Not available.** |
| 3 | Persistent application/configuration mechanism | `misc_rw` is UBI-backed and persistent, but stores only the vendor data-model blob (`0x00300000`); no application directory is sourced by firmware. | **Storage available; app mechanism not available.** |
| 4 | Startup mechanism executed by `rcS`/`cos` | `rcS` runs only `rcS.model`, mounts partitions, starts `cos`, `cmmsyslogd`; does not iterate `/etc/rcS_hook/` or source user scripts from `misc_rw`. | **Not available.** |
| 5 | Watchdog/supervisor for external process | `cos` `checkAndRestartDetectProcess` is WAN-online detection supervision, not a general-purpose supervisor. | **Not available.** |

### 3.1 `rcS` boot-chain inspection

From `_rootfs/etc/init.d/rcS`:

- Mounts `misc_rw` at `/var/run/misc/misc_rw` (UBIFS, read-write, persistent across reboots).
- Sources only `/etc/config.bba` and `/etc/init.d/rcS.model`.
- Starts `cos &` and `cmmsyslogd &`; no additional daemon enumeration.
- Does **not** call `rcsHook`, run `run-parts`, or source any script from `/var/run/misc/misc_rw`.

From `_rootfs/etc/inittab`:

```
::sysinit:/etc/init.d/rcS
::askfirst:/sbin/getty -L ttyS0 115200 vt100
```

From `_rootfs/etc/fstab`:

```
proc /proc proc defaults 0 0
ramfs /var ramfs defaults 0 0
devpts /dev/pts devpts defaults 0 0
```

`/var` is RAM-backed and non-persistent. Only the explicitly UBI-mounted `misc_rw`/`misc_rw_bak` are persistent and writable.

### 3.2 `cos` supervisor inspection

`cos` (TP-Link data-model process) launches daemons from fixed rootfs paths only:

```
httpd, telnetd, dropbear, cloud_https, cwmp, nrd, tmpd,
dnsProxy, igmpd, mldProxy, dyndns, noipdns, ntpc, snmpd,
diagTool, ipsecVpn, muAgentD, qoeStatisticsHandler,
tr143d, wanconnd2, wlNetlinkTool, obuspa, apsd
```

No plugin directory or user-daemon registration exists.

### 3.3 `rcsHook` orphan

The `rcsHook` binary contains `doRcsHookExes()` and can iterate `/etc/rcS_hook/`, but:

- `rcS` does **not** invoke `rcsHook`.
- `_rootfs/etc/rcS_hook/` is empty (only `.gitkeep`).
- `rcsHook` is not referenced by `rcS`, `cos`, `httpd`, or any other start script.

Conclusion: `rcS_hook` is an **orphaned/dead** mechanism.

### 3.4 GTPR write surface

- `so` (set-object) and `ACT_SAVE_CFG` persist data-model configuration to `misc_rw` as the blob `0x00300000`.
- There is **no GTPR operation** for writing arbitrary files, uploading binaries, or executing commands.
- Live experiments (Phase 14.6/14.7) showed that `so` on `DEV2_TELNET_CFG` and `ACT_SAVE_CFG` do **not** start `telnetd`; the `oal_setTelnetd` apply handler is in a separate dispatch table entry and is not triggered by the Web UI or CLI `so` flow.

---

## 4. Live Network Check

| Check | Result | Note |
|-------|--------|------|
| IPv6 neighbor | `fe80::3e6a:d2ff:fe5f:abc1 dev enp2s0 lladdr 3c:6a:d2:5f:ab:c1 router STALE` | EX520 still present on `enp2s0` link-local. |
| HTTP/80 | `200` | Web management reachable over IPv6 link-local. |
| HTTPS/443 | OPEN | TLS management port open (not tested in depth). |
| SSH/22 | CLOSED/UNREACH | No live SSH access. |
| Telnet/23 | CLOSED/UNREACH | No live Telnet access. |

This matches the canonical access note in `AGENTS.md`: IPv4 `192.168.0.1` management is not the path; IPv6 link-local HTTP/80 is the proven management interface.

Without an open shell service, there is **no file-transfer channel** to place the Detectic binary on `misc_rw`.

---

## 5. Deploy Package Status

A ready-to-use router-side package exists under `deploy/`:

- `deploy/detectic-ex520.tar.gz` — versioned release package.
- `deploy/install.sh`, `deploy/start.sh`, `deploy/stop.sh`, `deploy/update.sh`, `deploy/rollback.sh`, `deploy/remove.sh`, `deploy/health.sh`, `deploy/launcher.sh` — lifecycle scripts.
- `deploy/manifest.json` — release metadata.

These scripts are correctly designed to run from `/var/run/misc/misc_rw/detectic/`, use symlink-based `current`/`previous` release switching, and do not modify firmware or rootfs. However, **they all require an interactive or SCP-capable shell on the EX520 to run**, which is currently unavailable through any proven, non-invasive access path.

---

## 6. Deployment Matrix

| Capability | Result | Evidence |
|------------|--------|----------|
| **DEPLOY** | **NOT AVAILABLE** | No file-transfer or code-deployment operation in GTPR/GDPR; SSH/Telnet closed on live EX520; `deploy/` scripts require a shell. (E-15.0-01, E-15.0-02) |
| **PERSIST** | **POSSIBLE (storage only, not proven for app)** | `misc_rw` is UBI-backed, persistent, writable, and `rcS` mounts it early. (E-15.0-02, E-15.3-01) |
| **EXECUTE** | **NOT AVAILABLE** | No shell access; `cos` runs only fixed rootfs binaries; no documented remote exec path. (E-15.0-03, E-15.4-01) |
| **AUTOSTART** | **NOT AVAILABLE** | `rcS` does not source user scripts or call `rcsHook`; no cron/procd/service enumeration. (E-15.0-04, E-15.5-01) |
| **UPDATE** | **NOT AVAILABLE** | Requires `EXECUTE` + `DEPLOY`; not reachable without shell. |
| **ROLLBACK** | **NOT AVAILABLE** | Requires an installed release to roll back; not reachable without shell. |
| **RECOVERY** | **NOT AVAILABLE** | No supervisor exists for user-supplied processes; no watchdog for Detectic. (E-15.0-05) |
| **RESOURCE ISOLATION** | **NOT AVAILABLE** | No cgroups, no per-process memory limits, no documented CPU quotas; `cos` and `rcS` are the only supervisors. (E-15.6-01) |
| **MAINTAINABILITY** | **NOT AVAILABLE** | `deploy/` scripts define a good install/start/stop/status/update/rollback/health interface, but cannot be used without shell access. (E-15.7-01) |

---

## 7. Final Checklist

| Question | Answer |
|----------|--------|
| EX520 firmware modified | **NO** |
| Boot chain modified | **NO** |
| Router configuration modified | **NO** |
| Reboot required | **NO** |
| Detectic survives reboot | **UNKNOWN** (could not deploy) |
| Detectic independently updateable | **NO** |
| Detectic independently removable | **NO** (not installed) |
| Router operation affected | **NO** |

---

## 8. Evidence Index

| ID | Description | Status |
|----|-------------|--------|
| E-15.0-01 | GTPR/GDPR API provides only read `gl`/`go` and data-model `so`; no file upload or remote exec | PROVEN-STATIC |
| E-15.0-02 | `misc_rw` is UBI volume mounted read-write at `/var/run/misc/misc_rw` by `rcS` | PROVEN-STATIC |
| E-15.0-03 | `cos` launches only fixed rootfs daemons; no user-daemon registration | PROVEN-STATIC |
| E-15.0-04 | `rcS` does not source `/var/run/misc/misc_rw`, does not call `rcsHook`, does not run `run-parts` | PROVEN-STATIC |
| E-15.0-05 | `cos` `checkAndRestartDetectProcess` is for WAN detection, not a general supervisor | PROVEN-STATIC |
| E-15.3-01 | `misc_rw` is persistent across reboot and power loss if UBI flash is intact | PROVEN-STATIC |
| E-15.4-01 | SSH/22 and Telnet/23 are closed/unreachable on live EX520 IPv6 link-local | PROVEN-LIVE |
| E-15.5-01 | `inittab` has only `::sysinit:/etc/init.d/rcS` and serial `getty`; no autostart hooks | PROVEN-STATIC |
| E-15.6-01 | No cgroups/procd/ulimit application enforcement visible in rootfs | PROVEN-STATIC |
| E-15.7-01 | `deploy/*.sh` scripts provide a complete lifecycle interface but require a shell | PROVEN-STATIC |

---

## 9. Final Decision

### C — Persistent execution is unavailable; use external sensor.

**Rationale:**

1. The only proven, read-only, non-invasive EX520 capability is GTPR/GDPR `DEV2_WIFI_APDEV_ASSOCDEV` observation over IPv6 link-local HTTP/80.
2. The firmware provides a persistent writable UBI partition (`misc_rw`) but does **not** expose any legitimate mechanism to transfer, execute, or autostart an externally supplied binary from that partition without a shell or firmware modification.
3. Shell services (SSH, Telnet) are not currently reachable on the live device, and enabling them through `so`/`ACT_SAVE_CFG` has been shown not to start the `telnetd` daemon (Phase 14.6/14.7).
4. The `deploy/` package and lifecycle scripts are ready for use **if and when** a controlled, authorized shell access mechanism is separately validated, but such a mechanism is outside the Phase 15 safety boundary.

---

## 10. Stop Conditions Triggered

This phase encountered the following stop condition from the charter and stopped accordingly:

> "STOP immediately if the proposed mechanism requires: ... enabling undocumented services ... persistent modification without rollback ..."

The only remaining paths to router-side execution (enabling Telnet/SSH, modifying `rcS`/`inittab`/`cos`, or firmware modification) fall into those categories. They are **not authorized** in Phase 15.

---

## 11. Next Recommended Step

Continue with the **external sensor** architecture proven in Phase 14.9:

```
EX520 (unmodified, read-only RF source)
   |
   | GTPR/GDPR IPv6 link-local HTTP/80
   | DEV2_WIFI_APDEV_ASSOCDEV
   v
Detectic external host (python/detectic_sensor.py)
   |
   | HTTPS
   v
Detectic backend
```

Do **not** proceed to RF optimization, distance calibration, ML, performance tuning, or feature expansion for a router-side binary until a separate, explicitly authorized phase establishes a safe `DEPLOY → PERSIST → EXECUTE → AUTOSTART` path.
