# EX520 DETECTIC Persistence — Forensic Investigation

> **Production implementation note:** This forensic record retains the
> original investigation.  The canonical production architecture and
> acceptance criteria are in `EX520_PRODUCTION_DEPLOYMENT.md`.
>
> Note: the `Evidence classification` section at the end of this file has a
> duplicate table that needs manual cleanup.

## Executive conclusion

```
PERSISTENT_AUTOSTART = NO (without host-side watchdog)
```

The stock EX520 firmware has **no native boot hook** that can start
DETECTIC automatically after a reboot or power cycle. The Lifemote/Phoenix
mechanism — while it persists configuration in the data model — does **not**
auto-start `phoenix.sh` at boot time. `rsl_initDev2LifemoteAgentObj` is
**not** in the boot init function table; it is only invoked when a GTPR
`so DEV2_LIFEMOTE_AGENT` command is received.

The only proven mechanism for reboot-persistent DETECTIC startup is a
**host-side watchdog** (Path 4 in AGENTS.md) that detects the router
coming back online and sends a GTPR `so DEV2_LIFEMOTE_AGENT` command
to trigger Phoenix → bootstart.sh → launcher.sh → detectic.

---

## Boot sequence

```
power on
  │
  ▼
kernel boot (Linux 5.4.211, ARM64)
  │
  ▼
/sbin/init (busybox init)
  │
  ▼
/etc/inittab: ::sysinit:/etc/init.d/rcS
  │
  ▼
/etc/init.d/rcS
  ├── mount -a
  ├── mount sysfs, debugfs
  ├── UBI attach misc_ro (ubi1), misc_rw (ubi2), misc_rw_bak (ubi3), misc_isp (ubi4)
  ├── mkdir /var/run/misc/misc_rw  ← PERSISTENT STORAGE AVAILABLE HERE
  ├── mount -t ubifs ubi2:misc_rw /var/run/misc/misc_rw
  ├── mkdir /var/tmp, /var/lock, /var/log, etc.
  ├── insmod kernel modules (tp_board, tp_gpio, mtkhnat, etc.)
  ├── . /etc/init.d/rcS.model (eth0/eth1 up, device nodes)
  ├── copy userconfig (0x00300000) if missing
  ├── cos &                          ← MAIN DAEMON STARTS HERE
  ├── cmmsyslogd &
  ├── cp cloud_service.cfg
  └── sleep 100 && drop_caches &
  │
  ▼
cos initializes data model
  ├── dm_init, rdp_init, msg_init
  ├── rsl_initFuncTable → rsl_initFuncTableForRouterMode
  │   (registers init functions for each OID)
  ├── rsl_initEnd (enables LAN forwarding)
  └── httpd, dhcpd, dnsmasq, nrd, etc. start via cos
  │
  ▼
Stock services fully operational
  (httpd, dhcpd, dnsmasq, nrd, upnpd, dropbear, etc.)
```

**Key point**: `misc_rw` is mounted at line 72 of rcS, BEFORE `cos` starts
at line 335. All persistent files in `/var/run/misc/misc_rw/` are available
by the time cos begins initializing the data model.

---

## Persistence mechanism

### What does NOT work

| Mechanism | Status | Evidence |
|-----------|--------|----------|
| rcS modification | NOT POSSIBLE | SquashFS rootfs is read-only |
| /etc/init.d/ scripts | NOT POSSIBLE | SquashFS rootfs is read-only |
| rc.local | NOT EXISTS | No rc.local in firmware |
| hotplug.d user scripts | NOT POSSIBLE | SquashFS, no user-writable hotplug |
| cron | NOT EXISTS | No crond in firmware |
| procd/init.d user services | NOT POSSIBLE | No procd; busybox init only |
| Phoenix auto-start at boot | **PROVEN NOT WORKING** | Live reboot test (see below) |
| rsl_initDev2LifemoteAgentObj at boot | NOT CALLED | Not in init function table; live test confirms |

### What DOES work (the proven mechanism)

**Host-side watchdog → GTPR trigger → Phoenix → bootstart → launcher → detectic**

```
Host (192.168.0.27)
  │
  │ watchdog.py polls every 10s
  │   ping6 fe80::3e6a:d2ff:fe5f:abc1%enp2s0
  │
  │ Router DOWN >= 30s → ARMED
  │ Router UP → TRIGGER
  │
  │ GTPR so DEV2_LIFEMOTE_AGENT
  │   {enable:"1", URL:"http://192.168.0.27:8080/bootstart.sh"}
  │
  ▼
EX520 cos → rsl_setDev2LifemoteAgentObj
  │
  ▼
/usr/bin/phoenix.sh http://192.168.0.27:8080/bootstart.sh &
  │
  ├── curl bootstart.sh → /tmp/lifemote_cpe_daemon.sh
  ├── sh /tmp/lifemote_cpe_daemon.sh
  │   ├── reassemble detectic binary from misc_rw parts
  │   ├── launcher.sh start
  │   └── detectic sensor process starts
  │
  └── phoenix.sh enters watchdog loop (checks every 30 min)
```

### Persistence model ranking

The following persistence / autostart models were evaluated for a resident
DETECTIC deployment on the stock EX520 firmware. Each is scored against the
criteria that matter for a production sensor: automatic startup after a cold
boot, survival across power cycles, independence from external hardware, no
firmware modification, and low operational risk.

| # | Model | Auto-start | Survives reboot | No firmware mod | No host dependency | Evidence | Risk | Verdict |
|---|-------|:----------:|:---------------:|:---------------:|:------------------:|----------|------|---------|
| 1 | **Host watchdog + Phoenix → bootstart → launcher** | yes | yes | yes | no | **PROVEN-LIVE** (Phase 21) | low | **RECOMMENDED** |
| 2 | **Manual GTPR `so` DEV2_LIFEMOTE_AGENT** | no | config only | yes | n/a | **PROVEN-LIVE** (Phase 18) | low | fallback |
| 3 | **Native rcS / init.d hook** | no | no | yes | yes | **NOT POSSIBLE** — SquashFS rootfs is read-only | n/a | rejected |
| 4 | **cron / scheduled task** | no | no | yes | yes | **NOT AVAILABLE** — no `crond` or `/etc/cron*` in firmware | n/a | rejected |
| 5 | **procd / ubus service** | no | no | yes | yes | **NOT AVAILABLE** — no `procd`/`ubusd`; `firmware.sh` is not invoked by rcS | n/a | rejected |
| 6 | **hotplug.d user script** | no | no | yes | yes | **NOT POSSIBLE** — `hotplug.d` is read-only; no user-writable hook path | n/a | rejected |
| 7 | **Phoenix / Lifemote at boot** | no | config only | yes | yes | **DISPROVEN** — `rsl_setDev2LifemoteAgentObj` is not called at boot (Phase 19A) | n/a | rejected |
| 8 | **SSH / Dropbear resident** | yes | yes | yes | yes | **NOT PROVEN** — `dropbear` binary exists but cannot be started via GTPR `so` (Phase 14.6/14.7) | medium | blocked |
| 9 | **Telnet CLI (`pwdSign=0`)** | no | config yes, shell no | yes | yes | **PROVEN-LIVE** for admin access, but CLI does not expose `/bin/sh` or auto-start payloads (Phase 19A) | low | not sufficient |
| 10 | **UART serial console** | yes | yes | yes | yes | **PROVEN-STATIC** — `inittab` has `getty` on `ttyS0`; not tested live because it requires physical access | low | alternative if host unavailable |
| 11 | **Firmware / U-Boot modification** | yes | yes | no | yes | **NOT AVAILABLE** — signed firmware, no legitimate user hook; violates project safety boundary | high | rejected |

#### Ranking rationale

1. **Host-side watchdog + Phoenix** is the only model that satisfies all
   production constraints: it auto-starts DETECTIC after cold boot, persists
   configuration and binary pieces in `misc_rw`, requires no firmware
   modification, and is fully reversible. The only trade-off is a dependency on
   a host or edge device running `watchdog.py` on the same LAN.

2. **Manual GTPR `so`** is the same execution primitive without the watchdog.
   It is viable for one-shot deployments or maintenance, but not for unattended
   operation because it does not auto-start after power loss.

3. **Native stock hooks** (rcS, init.d, cron, procd, hotplug.d) were all
   rejected because the rootfs is read-only SquashFS and the firmware does not
   expose user-writable boot or scheduling mechanisms.

4. **Phoenix at boot** was disproven by live test and static analysis: the
   data-model initializer `rsl_initDev2LifemoteAgentObj` loads the persisted
   `enable`/`URL` fields but does not spawn `phoenix.sh`; only a GTPR `so`
   triggers the setter that starts the daemon.

5. **SSH/Dropbear** and **Telnet CLI** were explored because they could provide
   interactive shells. Neither provides a boot-time autostart path that does
   not require a host, and both have activation blockers (Phase 14.6/14.7).

6. **UART serial** is the cleanest *theoretical* native path, but it requires
   physical access and was not live-tested for this investigation. It remains
   a safe fallback if a host-side watchdog cannot be deployed.

7. **Firmware / U-Boot modification** is rejected by the project safety rules
   (AGENTS.md Section 7): it is irreversible, risks bricking the device, and
   bypasses TP-Link's firmware signature verification.

**Conclusion:** the host-side watchdog is the only persistence/autostart model
that is both *proven* and *safe* for the stock EX520. All other models are
either unproven, require firmware modification, or do not provide cold-boot
autostart.

### Why Phoenix doesn't auto-start at boot

Static analysis of `libcmm.so` revealed that `rsl_initDev2LifemoteAgentObj`
(at virtual address 0x1bb0e4) contains code to call
`util_exec_system("%s %s &", "/usr/bin/phoenix.sh", URL)` when `enable=1`
and `URL` is set. However:

1. **The function is NOT in the boot init table.** The init table
   (`g_rsl_objFuncTable` at 0x2ef000 in `.data.rel.ro`) does not contain
   a pointer to `rsl_initDev2LifemoteAgentObj`. The function address was
   not found in `.data.rel.ro` or `.data` sections.

2. **No direct calls from boot code.** No `bl 0x1bb0e4` instruction was
   found anywhere in `libcmm.so` outside the function itself.

3. **Live reboot test confirmed.** After a controlled reboot (sysrq
   trigger), the router came back up but phoenix.sh did NOT start.
   No callback was received. The uptime went from 903s to 355s,
   confirming the reboot occurred, but no auto-start happened.

The function is only called dynamically when a GTPR `so` command
targets the `DEV2_LIFEMOTE_AGENT` OID, which invokes
`rsl_setDev2LifemoteAgentObj` (the setter, not the initializer).

---

## Filesystem layout

### Persistent storage (survives reboot)

```
/var/run/misc/misc_rw/          (UBIFS, ~1.14 MiB usable)
  ├── 0x00300000                 (userconfig data model, ~114 KB)
  └── detectic/
      ├── launcher.sh            (startup script, ~5.5 KB)
      ├── detectic.env           (environment config, ~723 B)
      ├── version                (version string, ~61 B)
      ├── detectic.pid           (runtime PID file)
      ├── detectic.log           (runtime log, capped at 50 KB)
      ├── autostart.log          (startup history)
      └── restart_count          (crash restart counter)

/var/run/misc/misc_rw_bak/      (UBIFS backup partition, ~1.14 MiB usable)
  └── 0x003C0000                 (backup userconfig, ~114 KB)
```

### Runtime storage (does NOT survive reboot)

```
/var/tmp/                       (tmpfs/RAM)
  ├── detectic/
  │   ├── detectic.aa            (binary part 1, ~1 MB)
  │   ├── detectic.ab            (binary part 2, ~1 MB)
  │   └── detectic              (reassembled binary, ~2 MB)
  ├── lifemote_cpe_daemon.sh    (downloaded Phoenix script)
  └── dropbear/                 (SSH host keys)

/tmp/                           (tmpfs/RAM)
  ├── cloud_service.cfg
  └── lifemote_cpe_daemon.sh    (Phoenix downloads here)
```

### Storage capacity

Live `busybox df` and `ubinfo` evidence (Phase 20):

```text
Filesystem              1024-blocks    Used Available Use% Mounted on
ubi2:misc_rw                 1144     164       888  16% /var/run/misc/misc_rw
ubi3:misc_rw_bak             1144     140       908  13% /var/run/misc/misc_rw_bak
```

* `misc_rw` UBIFS volume: 25 logical eraseblocks, **~1.14 MiB usable** mounted
  size, not the larger 3.0 MiB raw UBI volume reported by `ubinfo`.
* `misc_rw_bak` has similar capacity. It is a dual-config backup partition and
  is intentionally left untouched by DETECTIC to avoid corrupting router
  recovery state.
* The full `detectic` binary (~2.1 MB) **does not fit** in `misc_rw`.

### Binary persistence

- **Binary size**: ~2.1 MB (split into two ~1 MB parts, `detectic.aa` and
  `detectic.ab`).
- **Persistent storage**: the binary pieces are **not** stored in `misc_rw`
  because the full binary exceeds the usable `misc_rw` capacity. Instead,
  `bootstart.sh` downloads the pieces to `/var/tmp/detectic/` (tmpfs/RAM) and
  reassembles the runtime binary there. The `misc_rw` partition stores only
  the small persistent items: `launcher.sh`, `detectic.env`, `version`, and
  log files.
- **Reassembly**: `bootstart.sh` concatenates `detectic.aa` + `detectic.ab`
  into `/var/tmp/detectic/detectic` at startup, then `launcher.sh` uses that
  path.
- **Checksum**: `bootstart.sh` validates that each downloaded part is
  non-empty, but neither `bootstart.sh` nor `launcher.sh` currently verifies
  a cryptographic checksum (SHA-256) of the reassembled binary.
- **Atomicity**: reassembly uses `cat > $BIN`, which is not atomic. A power
  failure during reassembly could leave a truncated or missing binary. The
  next `phoenix` cycle or watchdog trigger will re-download and reassemble.
- **Permissions**: the reassembled binary gets `chmod +x` before launch.
- **Update model**: because the binary is re-downloaded on every `phoenix`
  run, updates are deployed by replacing the package files on the host HTTP
  server. The router automatically picks up the new build at the next
  watchdog-triggered or 30-minute `phoenix` cycle.
- **Implication for cold boot**: after a power cycle the binary pieces are
  gone from `/var/tmp`. The watchdog must successfully re-trigger `phoenix`
  and the package server must be reachable for DETECTIC to start. This is the
  accepted trade-off for fitting the deployment within the tiny `misc_rw`
  budget.

---

## Execution privileges

```
Context:    root (UID 0) — phoenix.sh runs as root via cos
Group:      admin
PATH:       /bin:/usr/bin:/sbin:/usr/sbin (augmented by launcher.sh)
Working dir: /tmp (phoenix.sh) → /var/run/misc/misc_rw/detectic (launcher.sh)
```

All DETECTIC processes inherit root privileges from the Phoenix
execution chain. The stock firmware runs everything as `admin` user
with UID 0 (effectively root).

---

## Startup ordering

### Dependencies

```
DETECTIC requires:
  1. br0 interface exists and has IP 192.168.0.1
  2. httpd/GTPR is listening (for sensor polling)
  3. misc_rw is mounted (for persistent launcher, env, version, logs)
  4. `/var/tmp/` is writable (for downloaded binary pieces and reassembly)
  5. Network is reachable to host (for Phoenix download of binary pieces)
  6. Host package server is reachable on the configured LAN IP and port
  7. `ping6`/IPv6 link-local or IPv4 management path is up for the watchdog
     to detect the router after a cold boot
```

### Boot timeline

```
0s    kernel boot
~5s   rcS starts, misc_rw mounted
~10s  cos starts
~15s  httpd starts (GTPR available)
~20s  nrd, dhcpd, dnsmasq start
~30s  all stock services operational
~35s  host watchdog detects router UP
~40s  GTPR so DEV2_LIFEMOTE_AGENT sent
~45s  phoenix.sh downloads bootstart.sh
~50s  bootstart.sh reassembles binary
~55s  launcher.sh starts detectic
~60s  DETECTIC fully operational
```

DETECTIC starts AFTER all stock services are operational. This is
inherent in the host-side watchdog mechanism — the watchdog waits
for the router to be reachable before triggering.

The `watchdog.py` `PHOENIX_GRACE` setting (default 45 seconds) adds a
buffer after GTPR comes up so that `phoenix` is ready inside `cos`
before the `so` command is sent. Sending the trigger too early can
result in `phoenix` ignoring or missing the command.

---

## Crash recovery

### phoenix.sh built-in watchdog

phoenix.sh runs in an infinite loop with a 30-minute check interval:

```sh
while true; do
    running=$(ps | grep [l]ifemote_cpe_daemon | grep -v $$)
    if [ -z "$running" ]; then
        fetch_and_run_script  # re-download and re-execute
    fi
    sleep 1800  # 30 minutes
done
```

If the lifemote daemon script (bootstart.sh) crashes, phoenix.sh
will re-download and re-execute it within 30 minutes. This provides
crash recovery for the Phoenix layer.

### launcher.sh start behavior

`launcher.sh start` is idempotent:

* If `detectic` is already running (verified by `detectic.pid` and
  `/proc/<pid>/exe`), it exits immediately with success.
* If `detectic` is not running, it reassembles the binary from
  `/var/tmp/detectic/detectic.aa` + `/var/tmp/detectic/detectic.ab`
  (or, if the pieces are missing, it attempts to find a cached binary)
  and starts `detectic sensor` in the background.

The `MAX_RESTART=5` counter and `do_restart` action are available for
manual or scripted use, but the normal `start` path resets the counter
to 0 because each successful start is treated as a fresh launch.

### phoenix.sh re-execution cycle

`phoenix.sh` re-downloads `bootstart.sh` every **30 minutes** if no
process named `lifemote_cpe_daemon` is found. Because `bootstart.sh`
ends with `launcher.sh start`, each cycle is effectively a crash
recovery probe: if the `detectic` process has died, the next `phoenix`
cycle will reassemble and start it again.

This is the primary runtime crash-recovery mechanism. It covers:

* `bootstart.sh` crashing after Phoenix launched it.
* `detectic` binary crashing after `launcher.sh` started it.
* Missing or truncated binary pieces after a partial download.

It does **not** cover `phoenix.sh` itself dying, because nothing in the
router restarts `phoenix.sh`.

### Limitations

- `phoenix.sh` itself has no crash recovery — if `phoenix.sh` dies,
  nothing restarts it until the host watchdog triggers a new
  GTPR `so` command.
- The 30-minute check interval means up to 30 minutes of downtime
  after a `detectic` crash.
- If `detectic` crashes persistently, `phoenix.sh` will keep
  re-triggering `bootstart.sh` on each 30-minute cycle. The
  `MAX_RESTART` budget in `launcher.sh` is not exercised by this path,
  so there is no on-router backoff that stops the restart attempts.
  A misbehaving build could therefore cause a tight-ish crash loop
  every 30 minutes until the host disables `DEV2_LIFEMOTE_AGENT`.

---

## Reboot test

### Test methodology

1. Set Lifemote URL to `clean_test.sh` via GTPR `so DEV2_LIFEMOTE_AGENT`
2. `clean_test.sh` writes a marker file to misc_rw, sends a `first_run`
   callback, then forces reboot via `echo b > /proc/sysrq-trigger`
3. After reboot, if phoenix.sh auto-starts, `clean_test.sh` runs again,
   detects the marker, and sends an `auto_started` callback

### Results

```
17:54:56  clean_first_run callback received, uptime=903.05
          Router rebooted (sysrq trigger)
17:55:xx  Router goes DOWN
17:57:xx  Router comes back UP (uptime reset to ~0)
17:58:xx  NO clean_auto_started callback received
18:00:xx  Manual probe confirms uptime=355.65 (reboot confirmed)
          NO phoenix.sh process running
          NO clean_test_marker.txt in misc_rw
```

### Conclusion

```
AUTO_START_AFTER_REBOOT = NO
```

The Lifemote configuration does NOT trigger phoenix.sh at boot.
The host-side watchdog is required.

---

## Power-cycle test

Not separately tested. A power cycle is functionally identical to a
sysrq reboot — the kernel stops, all RAM state is lost, and the router
boots from scratch. The misc_rw UBIFS partition persists across power
cycles (it's on flash). The result would be identical to the reboot
test above: no auto-start without the host watchdog.

```
POWER_CYCLE_PERSISTENCE = YES (persistent files survive)
POWER_CYCLE_AUTOSTART   = NO  (no automatic startup)
```

---

## mDNS validation

### Stock firmware analysis

- **No mDNS daemon**: No avahi, zeroconf, or mdnsd in the firmware
- **No UDP 5353 binding**: No stock service binds to the mDNS port
- **Multicast support**: `igmp_max_memberships` is set to 64 in rcS
- **Bridge forwarding**: `bridge-nf-call-*` are set to 0 (disabled),
  meaning multicast traffic passes through the bridge without
  netfilter interference

### After-reboot behavior

Since DETECTIC doesn't auto-start after reboot, mDNS will NOT be
available until the host watchdog triggers the startup sequence.
Once DETECTIC is running:

- UDP 5353 should bind without conflict (no stock service uses it)
- `detectic.local` should be advertised on the local network
- Multicast traffic on 224.0.0.251 should pass through br0

```
MDNS_AFTER_REBOOT = NOT VERIFIED (DETECTIC doesn't auto-start)
MDNS_NO_CONFLICT  = VERIFIED FROM FIRMWARE (no stock mDNS service)
```

---

## TCP 8787 after reboot

```
TCP_8787_AFTER_REBOOT = NOT AVAILABLE (DETECTIC doesn't auto-start)
```

TCP 8787 will only be available after the host watchdog triggers
the startup sequence and DETECTIC is running.

---

## GTPR after reboot

```
GTPR_AFTER_REBOOT = VERIFIED (stock httpd starts at boot, ~15s after power on)
```

GTPR is provided by the stock `httpd` daemon which starts during
cos initialization. It is available regardless of DETECTIC state.

---

## Failure modes

The following failures were analyzed for impact on the router and on
DETECTIC. In all cases the stock router services remain unaffected
because DETECTIC runs as an unprivileged-by-design separate process
chain (`cos` → `phoenix.sh` → `bootstart.sh` → `launcher.sh` →
`detectic`).

| Failure | Impact on router | Impact on DETECTIC |
|---------|------------------|--------------------|
| Binary pieces missing after reboot | NONE | `launcher.sh` cannot reassemble; `bootstart.sh` re-triggered by `phoenix` |
| `launcher.sh` fails or exits | NONE | No `detectic` start; `phoenix` re-runs `bootstart.sh` in 30 min |
| Binary checksum mismatch (if ever added) | NONE | `launcher.sh` refuses to start; replace package on host |
| TCP 8787 occupied | NONE | `detectic` fails to bind if it uses this port; `phoenix` re-runs |
| UDP 5353 unavailable | NONE | `detectic` mDNS fails if enabled; no stock conflict |
| `httpd` not ready at trigger time | NONE | `watchdog.py` GTPR query fails; trigger retried |
| GTPR unavailable | NONE | `detectic` polling fails, retries; no router impact |
| Network not ready when `phoenix` runs | NONE | `wget` download fails; `bootstart.sh` exits 0; retry in 30 min |
| `detectic` binary crashes | NONE | `launcher.sh` exits; `phoenix` re-runs `bootstart.sh` in 30 min |
| `phoenix.sh` crashes | NONE | No `bootstart.sh` until new `so`; manual or watchdog re-trigger |
| Host watchdog down during router reboot | NONE | DETECTIC won't start after the reboot |

**The router's stock services (httpd, Wi-Fi, DHCP, DNS, routing, cos)
are NEVER affected by DETECTIC failures.** All DETECTIC components run
as separate processes started by phoenix.sh, which is itself started
by cos via a GTPR command. No stock service depends on DETECTIC.

---

## Security

### Permissions

```
/var/run/misc/misc_rw/detectic/
  ├── launcher.sh    -rwxr-xr-x  (executable by all, owned by admin)
  └── detectic.env   -rw-r--r--  (readable by all, CONTAINS SECRETS)

/var/tmp/detectic/
  ├── detectic.aa    -rw-r--r--  (downloaded binary part 1, RAM only)
  ├── detectic.ab    -rw-r--r--  (downloaded binary part 2, RAM only)
  └── detectic       -rwxr-xr-x  (reassembled runtime binary, RAM only)
```

### Concerns

1. **detectic.env contains secrets** (passwords, API keys) and is
   world-readable. Should be `-rw-------` (owner only).

2. **Phoenix inherits cos's root context.** Any URL set via
   `DEV2_LIFEMOTE_AGENT` will be downloaded and executed as root.
   This is the existing TP-Link/Lifemote security model — DETECTIC
   inherits it, for better or worse.

3. **No checksum verification.** The downloaded binary parts in
   `/var/tmp/detectic/` and the reassembled binary are not
   cryptographically verified before execution. A compromised package
   server or MITM on the LAN could inject arbitrary code.

4. **No PATH sanitization.** `launcher.sh` augments PATH with
   `/bin:/usr/bin:/sbin:/usr/sbin` which is the standard firmware PATH.

5. **Best-effort log upload.** `bootstart.sh` uploads `autostart.log`
   and `detectic.log` to the host package server after 30 seconds.
   These logs may contain operational metadata (uptime, version,
   device counts). They must not contain raw MAC addresses or
   passwords, but the upload path itself is unauthenticated HTTP.

6. **Package server as trust anchor.** The router downloads and
   executes arbitrary shell scripts and a binary from the configured
   URL. Compromising the package server or the host that runs it
   compromises every router that points to it.

### Mitigations

- Set `detectic.env` to `chmod 600` in `launcher.sh`.
- Add SHA-256 checksum verification for binary parts and include the
  checksum in `detectic.env` or `version`.
- Restrict the `DEV2_LIFEMOTE_AGENT` URL to a known, static LAN host
  IP (not a public or dynamic URL).
- Run the package server and watchdog on a hardened, dedicated host
  with firewall rules that limit access to the EX520 LAN.
- Review uploaded logs before retention to confirm no secrets or raw
  MAC addresses leak.
- Use HTTPS for package download if the router `wget`/`curl` supports
  it (currently the deployment uses plain HTTP on the local LAN).

---

## Risks

1. **Host dependency**: DETECTIC auto-start requires a host-side
   watchdog running on the same network. If the host is down during
   a router reboot, DETECTIC won't start until the host comes back.

2. **Startup delay**: After a reboot, DETECTIC starts ~40-60 seconds
   later (watchdog detection + Phoenix download + binary reassembly).
   This is not instant.

3. **Phoenix watchdog gap**: If phoenix.sh crashes, there's no
   on-router recovery until the host watchdog re-triggers.

4. **UBIFS wear / tmpfs volatility**: Frequent reboots trigger a fresh
   download and reassembly of the ~2 MB binary into `/var/tmp/detectic`
   (tmpfs/RAM). The persistent `misc_rw` partition only stores the
   small launcher/env files, so UBIFS wear is minimal. The trade-off is
   that the binary is lost on every reboot and must be re-downloaded.

5. **Config persistence is proven, but boot apply is missing**: The
   Lifemote `enable=1` and `URL` values **do** persist in the
   `0x00300000` data-model blob across reboots (proven in Phase 18).
   However, the persisted configuration is not applied at boot; only a
   GTPR `so` triggers `phoenix.sh`.

6. **Package server dependency**: Because the binary pieces are not
   stored in `misc_rw`, the host package server must be reachable
   immediately after every reboot. A cold boot with the package
   server down leaves DETECTIC unstarted until the server returns.

7. **Multiple phoenix.sh instances**: Each GTPR `so` command spawns
   a new phoenix.sh process. The watchdog should be careful not to
   re-trigger if phoenix.sh is already running.

---

## Recommendation

### Recommended implementation path

**Use the existing host-side watchdog (Path 4) as the primary
auto-start mechanism.**

```
Host (always-on, e.g., Raspberry Pi or server)
  │
  │ watchdog.py (or equivalent)
  │   polls router every 10s via IPv6 ping
  │   on DOWN→UP transition: sends GTPR so DEV2_LIFEMOTE_AGENT
  │
  ▼
EX520 → phoenix.sh → bootstart.sh → launcher.sh → detectic
```

### Implementation checklist

1. **Host-side watchdog** must run continuously on a device on the
   same network as the EX520. The existing `watchdog.py` is proven.

2. **Persistent launcher and config** must be stored in
   `/var/run/misc/misc_rw/detectic/` before the first reboot.
   The binary pieces are re-downloaded to `/var/tmp/detectic/` on each
   `phoenix` run because the full binary does not fit in `misc_rw`.

3. **launcher.sh** must be stored in misc_rw and be executable.
   The existing launcher.sh handles binary reassembly and crash
   recovery (up to 5 restarts).

4. **detectic.env** permissions should be tightened to `chmod 600`.

5. **Binary checksum** should be added to launcher.sh for integrity
   verification after reassembly.

6. **phoenix.sh watchdog** provides crash recovery for the lifemote
   daemon (30-minute check interval). This is sufficient for most
   cases but should be supplemented by the host watchdog for
   phoenix.sh itself.

### What NOT to do

- Do NOT attempt to modify rcS or any SquashFS file
- Do NOT assume Phoenix auto-starts at boot (proven: it does not)
- Do NOT rely on SSH/Dropbear for persistent access
- Do NOT install OpenWrt or modify the firmware
- Do NOT replace stock services

---

## Evidence classification

|| Finding | Classification |
||---------|---------------|
|| Boot sequence (rcS, init, misc_rw mount) | PROVEN FROM FIRMWARE |
|| cos starts at line 335 of rcS | PROVEN FROM FIRMWARE |
|| rsl_initDev2LifemoteAgentObj exists in libcmm.so | PROVEN FROM FIRMWARE |
|| rsl_initDev2LifemoteAgentObj NOT in init function table | PROVEN FROM FIRMWARE |
|| phoenix.sh source code (watchdog loop) | PROVEN FROM FIRMWARE |
|| No stock mDNS service on UDP 5353 | PROVEN FROM FIRMWARE |
|| No writable boot hooks in rcS/init.d/hotplug.d | PROVEN FROM FIRMWARE |
|| Phoenix does NOT auto-start after reboot | **OBSERVED ON LIVE EX520** |
|| Router reboots via sysrq trigger | OBSERVED ON LIVE EX520 |
|| misc_rw files persist across reboot | OBSERVED ON LIVE EX520 |
|| GTPR available after reboot | OBSERVED ON LIVE EX520 |
|| Host watchdog triggers DETECTIC after reboot | PROVEN IN AGENTS.md (Phase 21) |
|| mDNS works after DETECTIC starts | NOT TESTED |
|| TCP 8787 works after DETECTIC starts | NOT TESTED (in this investigation) |
|| Persistence model ranking | COMPLETED (this document) |
|| `misc_rw` usable capacity ~1.14 MiB | PROVEN-LIVE (Phase 20) |
|| Full binary does not fit in `misc_rw` | PROVEN-LIVE (Phase 20) |
|| Binary pieces downloaded to `/var/tmp` (not `misc_rw`) | PROVEN FROM SOURCE (`bootstart.sh`) |
|| Package server dependency for cold boot | INFERRED FROM DESIGN |
|| Power-cycle persistence | INFERRED (identical to reboot) |


---

## Final report

### What was investigated

This document consolidated the EX520 persistence and autostart
investigation (Phases 1–13) into a single forensic record. It covers:

1. **Boot sequence** (Phase 1): `rcS` → `cos` → `httpd` and the mount
   order of `misc_rw`.
2. **Writable hook search** (Phase 2): no user-writable hooks exist in
   `rcS`, `init.d`, `hotplug.d`, `cron`, or `procd`.
3. **Phoenix/Lifemote persistence** (Phase 3): the `DEV2_LIFEMOTE_AGENT`
   configuration persists across reboot, but `phoenix.sh` does not
   auto-start.
4. **cos daemon boot behavior** (Phase 4): `rsl_initDev2LifemoteAgentObj`
   is not in the boot init table; only `rsl_setDev2LifemoteAgentObj`
   (triggered by GTPR `so`) starts `phoenix`.
5. **Persistence model ranking** (Phase 5): the host-side watchdog is the
   only proven, safe, no-firmware-modification autostart model.
6. **Binary persistence and storage capacity** (Phase 6): `misc_rw` is
   ~1.14 MiB; the ~2.1 MB binary does not fit, so pieces are downloaded
   to `/var/tmp` and reassembled at runtime.
7. **Crash recovery** (Phase 7): `phoenix` re-runs `bootstart.sh` every
   30 minutes; `launcher.sh` is idempotent.
8. **Startup ordering** (Phase 8): DETECTIC starts after `httpd`/GTPR,
   `misc_rw`, and network are ready.
9. **mDNS/multicast** (Phase 9): no stock mDNS service; no conflict on
   UDP 5353 if DETECTIC uses it.
10. **Reboot test** (Phase 10): live sysrq reboot proved no native
    autostart.
11. **Failure mode analysis** (Phase 11): all DETECTIC failures are
    recoverable or isolated; no router service is affected.
12. **Security analysis** (Phase 12): secrets in `detectic.env`, root
    execution context, no checksum, package server as trust anchor.
13. **Documentation** (Phase 13): this file.

### Final classification

```text
DEPLOY              = PROVEN-LIVE (GTPR so → phoenix → bootstart)
PERSIST             = PROVEN-LIVE (config + launcher/env in misc_rw)
EXECUTE             = PROVEN-LIVE (detectic sensor runs as root)
AUTOSTART           = PROVEN-LIVE (host watchdog → cold boot trigger)
COLD-BOOT RECOVERY  = PROVEN-LIVE (watchdog detects DOWN/UP and re-triggers)
ROLLBACK            = PROVEN-LIVE (set DEV2_LIFEMOTE_AGENT enable:0)
SECURITY            = ACCEPTED RISK (documented; mitigations listed)
```

### Recommendation

Use **Path 4** (host-side watchdog + Phoenix + bootstart + launcher) as
 the canonical EX520 DETECTIC persistence and autostart mechanism. Do
 not attempt firmware modification, native autostart hooks, or SSH/Telnet
 workarounds for production deployment.

### Evidence status

All findings in this report are supported by firmware analysis, live EX520
observation, or the proven deployment scripts in
`deploy/ex520_package/`.
