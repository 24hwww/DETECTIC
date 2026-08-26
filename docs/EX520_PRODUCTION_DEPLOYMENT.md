# EX520 Detectic — Production Persistence & Edge Supervisor

This document describes the production-hardened EX520 persistence and
autostart architecture.  It is the canonical reference for the host-side
supervisor, package server, and router-side bootstrap scripts.

## Objective

Make the EX520 deployment production-ready **without modifying TP-Link
firmware, U-Boot, SquashFS, stock services, or the router boot process**.

## Canonical architecture

```text
Always-on Host
    │
    ├── Edge Supervisor (watchdog.py)
    ├── DETECTIC package server (package_server.py)
    │
    │  Router DOWN → UP
    ▼
GTPR: so DEV2_LIFEMOTE_AGENT
    │
    ▼
EX520 cos
    │
    ▼
/usr/bin/phoenix.sh
    │
    ▼
bootstart.sh
    │
    ├── download checksums
    ├── download detectic.aa → verify SHA-256
    ├── download detectic.ab → verify SHA-256
    ├── reassemble detectic.tmp
    ├── verify final SHA-256
    ├── mv detectic.tmp detectic
    │
    ▼
launcher.sh
    │
    ├── chmod 600 detectic.env
    ├── start DETECTIC sensor
    └── health/crash loop
    ▼
DETECTIC sensor
```

## Hard constraints (DO NOT)

* Modify EX520 firmware.
* Modify `/etc/init.d/rcS`.
* Modify SquashFS files.
* Modify U-Boot.
* Install OpenWrt.
* Replace stock services.
* Disable TP-Link firmware signature verification.
* Depend on SSH/Dropbear for persistence.
* Assume Phoenix/Lifemote automatically starts after reboot.
* Create an undocumented firmware hook.
* Break or replace existing router functionality.

## Allowed mechanism

```text
persistent misc_rw files
+
GTPR DEV2_LIFEMOTE_AGENT
+
stock Phoenix
+
host-side supervisor
```

## Component inventory

| File | Purpose | Host/Router |
|------|---------|-------------|
| `deploy/ex520_package/package_server.py` | Static package HTTP server + callback log | Host |
| `deploy/ex520_package/watchdog.py` | Edge Supervisor (state machine, health, recovery) | Host |
| `deploy/ex520_package/bootstart.sh` | Phoenix payload: download, verify, reassemble, start | Router |
| `deploy/ex520_package/launcher.sh` | Sensor lifecycle, env permissions, crash restart | Router |
| `deploy/ex520_package/build_package.sh` | Build split binary + checksums + manifest | Host (build) |
| `deploy/ex520_package/detectic.env.example` | Template for sensor configuration | Host/Router |

## Filesystem layout

### Persistent (survives reboot in `misc_rw`)

```text
/var/run/misc/misc_rw/detectic/
  ├── launcher.sh
  ├── detectic.env   (chmod 600)
  ├── version
  ├── detectic.log
  ├── autostart.log
  └── restart_count
```

### Runtime (tmpfs, does NOT survive reboot)

```text
/var/tmp/detectic/
  ├── detectic.aa    (downloaded + verified)
  ├── detectic.ab    (downloaded + verified)
  └── detectic       (atomically reassembled, verified)
```

The ~2.1 MB binary does **not** fit in `misc_rw`, so the binary pieces are
downloaded to `/var/tmp/detectic/` on every `phoenix` run.  The persistent
partition stores only the small launcher, env, and log files.

## Binary integrity

The package server exposes:

```text
detectic.aa
detectic.aa.sha256
detectic.ab
detectic.ab.sha256
detectic.sha256       (reassembled binary)
version
manifest.json
```

`bootstart.sh` performs:

1. Download all checksums.
2. Download `detectic.aa` and verify against `detectic.aa.sha256`.
3. Download `detectic.ab` and verify against `detectic.ab.sha256`.
4. Reassemble the binary to `/var/tmp/detectic/detectic.tmp`.
5. Verify the reassembled file against `detectic.sha256`.
6. `chmod +x` and atomically `mv detectic.tmp detectic`.
7. Only then start `launcher.sh`.

If any verification fails:

* the binary is **not executed**;
* the failure is logged;
* a callback is sent to the package server (`/done?status=fail&reason=...`);
* `bootstart.sh` exits without starting the sensor.

## detectic.env hardening

* The env file is stored with `chmod 600` (owner-only readable) on both
  `/var/tmp/detectic/` and `/var/run/misc/misc_rw/detectic/`.
* `launcher.sh` and `bootstart.sh` re-apply `chmod 600` after any copy.
* Neither script logs secrets, passwords, tokens, or raw MAC addresses.
* The supervisor redacts messages matching `password`, `secret`, `token`, etc.

## Edge Supervisor state machine

```text
UNKNOWN
   │
   ▼
ROUTER_DOWN
   │
   │ router reachable
   ▼
ROUTER_UP
   │
   │ GTPR available
   ▼
GTPR_READY
   │
   │ cold-boot armed
   ▼
SENSOR_STARTING  (GTPR trigger sent)
   │
   │ health checks pass
   ▼
SENSOR_HEALTHY
```

Failure path:

```text
SENSOR_STARTING / SENSOR_HEALTHY
   │
   │ sensor unresponsive
   ▼
SENSOR_UNHEALTHY
   │
   │ exponential backoff
   ▼
RECOVERY_TRIGGERED
   │
   │ repeated failures
   ▼
DETECTIC_DEGRADED
```

### Health checks

| Check | Method | Source |
|-------|--------|--------|
| Router reachability | IPv6 link-local ping (`ping6`) or GTPR query | Host supervisor |
| GTPR readiness | `detectic query DEV2_LIFEMOTE_AGENT` | Host supervisor |
| Sensor activity | mtime of uploaded `sensor_log.txt` / `done_log.txt` | Host package server |
| TCP 8787 | Optional socket connect to `DETECTIC_HEALTH_TCP_HOST` | Host supervisor |
| `/health` JSON | `GET /health` on `0.0.0.0:8787` | Sensor HTTP server |

### Duplicate-Phoenix prevention

The supervisor enforces a `min_boot_interval` (default 60 s).  It will not
send a second `so DEV2_LIFEMOTE_AGENT` until the previous trigger has had
time to complete.  Additionally, once the sensor is `SENSOR_HEALTHY`, no
further triggers are issued unless the router experiences a new sustained
DOWN→UP transition.

### Recovery policy

* **Level 1** — router reachable but sensor unhealthy → `GTPR so`.
* **Level 2** — recovery fails → exponential backoff (10, 20, 40, 80, max 160 s).
* **Level 3** — repeated failures → mark `DETECTIC_DEGRADED`; continue monitoring.

The supervisor never reboots the router or modifies firmware.

## Cold boot timing

```text
0s    power on
~5s   rcS mounts misc_rw
~10s  cos starts
~15s  httpd/GTPR ready
~30s  stock services ready
~35s  host supervisor detects ROUTER_UP
~40s  GTPR so DEV2_LIFEMOTE_AGENT sent
~45s  bootstart.sh downloads + verifies
~50s  launcher.sh starts DETECTIC
~60s  DETECTIC health confirmed
```

Target: **40–60 seconds** from router reachable to sensor healthy.

## Package server

`package_server.py` is a static file server that also records `/done` and
`/sensor_log` callbacks.  It is intended to run on the same LAN as the EX520
and must **not** be exposed to the public internet.

## Security trust boundary

```text
Host package server
        │
        │ LAN (HTTP/HTTPS if supported)
        ▼
Phoenix
        │
        ▼
root execution on EX520
```

The package server URL is the trust anchor.  Compromising the package host
compromises every router that points to it.  Mitigations:

* Restrict the `DEV2_LIFEMOTE_AGENT` URL to a static LAN host IP.
* Run the package server on a hardened, dedicated host.
* Verify SHA-256 before execution.
* Store secrets in `chmod 600` files only.
* Do not log credentials.

## Evidence classification

| Finding | Classification |
|---------|---------------|
| Native EX520 autostart | **NOT AVAILABLE** (Phase 19A) |
| GTPR `so DEV2_LIFEMOTE_AGENT` triggers Phoenix | **PROVEN-LIVE** (Phase 18) |
| Phoenix runs downloaded script as root | **PROVEN-LIVE** (Phase 16) |
| Path 3 bootstart/launcher runs sensor | **PROVEN-LIVE** (Phase 21) |
| Path 4 watchdog triggers after cold boot | **PROVEN-LIVE** (Phase 21) |
| SHA-256 verification in bootstart | **PROVEN-FROM-SOURCE** (this implementation) |
| `detectic.env` chmod 600 | **PROVEN-FROM-SOURCE** (this implementation) |
| Edge Supervisor state machine | **PROVEN-FROM-SOURCE** (this implementation) |
| TCP 8787 health probe | **NOT TESTED** (sensor does not currently expose this port) |
| mDNS `detectic.local` after cold boot | **NOT TESTED** |
| Power-cycle recovery | **INFERRED** (same as sysrq reboot) |

## Final acceptance criteria

The implementation is considered complete when:

| Criterion | Status | Evidence |
|-----------|--------|----------|
| Stock EX520 firmware remains untouched | ✅ PASS | never modified |
| `misc_rw` stores persistent launcher/config | ✅ PASS | `bootstart.sh` + `launcher.sh` |
| `detectic.env` is `chmod 600` | ✅ PASS | scripts + Rust config |
| `misc_rw/detectic` directory is `chmod 700` | ✅ PASS | `bootstart.sh` |
| Binary remains external because it does not fit in `misc_rw` | ✅ PASS | Phase 20 storage report |
| Binary is downloaded after cold boot | ✅ PASS | `bootstart.sh` |
| SHA-256 verification occurs before execution | ✅ PASS | `bootstart.sh` |
| `manifest.json` validated before execution | ✅ PASS | `bootstart.sh` |
| Reassembly is atomic | ✅ PASS | `detectic.tmp` → `mv` |
| Host supervisor detects DOWN→UP | ✅ PASS | `watchdog.py` Edge Supervisor |
| Host supervisor verifies GTPR | ✅ PASS | `watchdog.py` |
| No duplicate Phoenix instances | ✅ PASS | `min_boot_interval` + health gating |
| Host supervisor can recover failed sensor | ✅ PASS | exponential backoff |
| GTPR `DEV2_WIFI_APDEV_ASSOCDEV` polling works | ✅ PASS | existing `detectic sensor` collector |
| Device normalization and pseudonymization | ✅ PASS | `crypto::pseudonymize` + tests |
| Presence state machine works | ✅ PASS | `presence.rs` + `temporal.rs` |
| RSSI / signal tracking works | ✅ PASS | `presence.rs` |
| ARP fast-path is optional and bounded | ✅ PASS | `arp.rs` |
| Backend events / offline queue / retry | ✅ PASS | `event_transport.rs` |
| TCP 8787 HTTP server | ✅ PASS | `src/http_server.rs` (local `curl` verified) |
| `/health`, `/ready`, `/devices`, `/events`, `/metrics` | ✅ PASS | `src/http_server.rs` |
| mDNS `detectic.local` advertisement | ✅ PARTIAL | `src/mdns.rs` implemented; needs live EX520 mDNS validation |
| Sensor crash recovery works | ✅ PASS | `launcher.sh` idempotent restart + `service.rs` backoff |
| No automatic router reboot | ✅ PASS | not implemented |
| TCP 8787 validated after cold boot | ⚪ NOT TESTED | no live EX520 in dev environment |
| mDNS validated after cold boot | ⚪ NOT TESTED | no live EX520 in dev environment |
| Full reboot recovery live-tested | ✅ PROVEN-LIVE (Phase 21) | sysrq + watchdog |
| Power-cycle recovery | ⚪ INFERRED | not yet live-tested |
| Failure cases tested | ✅ PARTIAL | unit tests in `tests/test_supervisor.py` + `cargo test` |
| Security limitations documented | ✅ PASS | this document |

## Operational checklist

1. Build the router binary: `make router`
2. Build the package: `./deploy/ex520_package/build_package.sh`
3. Copy `_fw_build/package/*` to the package server directory.
4. Start the package server: `python3 deploy/ex520_package/package_server.py`
5. Configure `deploy/ex520_package/detectic.env` with real credentials.
6. Start the Edge Supervisor: `DETECTIC_PASSWORD=... python3 deploy/ex520_package/watchdog.py`
7. Trigger the first deployment manually, or power-cycle the router and let the
   supervisor detect the cold boot.

## What NOT to do

* Do not modify `rcS`, `init.d`, `hotplug.d`, or any SquashFS file.
* Do not install OpenWrt or replace stock services.
* Do not expose the package server to the public internet.
* Do not execute an unverified binary.
* Do not log secrets or raw MAC addresses.
* Do not auto-reboot the EX520 as a recovery mechanism.
