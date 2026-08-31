# Detectic — AGENTS.md

> **⚠️ SAFETY PROTOCOLS — MANDATORY READ:** Before ANY operation on the EX520
> router, read and follow `docs/EX520_SAFETY_PROTOCOLS.md`.  That document
> defines the risk classification matrix (GREEN/YELLOW/ORANGE/RED), router
> reboot protocol, service preservation rules, and forbidden actions.
> **Never reboot the router, modify router configuration, or trigger Phoenix
> without classifying the operation and obtaining authorization per the
> protocol.**  Violations can disrupt DHCP, DNS, WiFi, and internet for all
> LAN devices.

> **Project status:** Production implementation — see `docs/EX520_PRODUCTION_DEPLOYMENT.md`,
> `docs/EX520_DEPLOYMENT.md`, `docs/EX520_OPERATIONS.md`, and `docs/EX520_TEST_PLAN.md`
> for the canonical deployment and test plan.

> **Project status:** Hardware research / sensor bring-up
> **Current target:** TP-Link EX520V
> **Primary objective:** Turn an inexpensive consumer Wi-Fi router into a Detectic sensing node capable of observing Wi-Fi activity, performing lightweight local processing, and securely sending aggregated observations to a remote Detectic backend.

---

# 1. What is Detectic?

Detectic is a **Wi-Fi-based presence, activity, and environmental sensing platform**.

The central idea is to use an ordinary consumer Wi-Fi router or access point as a low-cost sensing device.

Instead of requiring specialized sensing hardware, Detectic attempts to reuse the wireless hardware already present in routers to observe available Wi-Fi information and derive higher-level information such as:

* device presence
* device recurrence
* first seen / last seen
* approximate presence duration
* time-of-day patterns
* recurring activity
* new or unusual devices
* movement patterns
* occupancy estimates
* correlations between multiple sensors
* behavioral anomalies

The long-term goal is to make Detectic deployable on inexpensive hardware.

Conceptually:

```text
                    Wi-Fi Environment
                           │
                           ▼
                  ┌──────────────────┐
                  │ Consumer Router  │
                  │                  │
                  │ Detectic Sensor  │
                  └────────┬─────────┘
                           │
                    Local processing
                           │
                    Aggregated events
                           │
                         HTTPS
                           │
                           ▼
                  ┌──────────────────┐
                  │ Detectic Backend │
                  │                  │
                  │ API              │
                  │ Database         │
                  │ Analytics        │
                  │ Pattern Engine   │
                  └──────────────────┘
```

---

# 2. Core Philosophy

Detectic follows a simple principle:

> **Process the data as close to the source as reasonably possible, but keep the router lightweight.**

The router is a **sensor**, not the main computing platform.

The router should perform:

* Wi-Fi observation
* event extraction
* filtering
* normalization
* lightweight aggregation
* deduplication
* pseudonymization
* buffering
* secure transmission

The backend should perform:

* long-term storage
* historical analysis
* cross-sensor correlation
* behavioral analysis
* anomaly detection
* dashboards
* machine learning
* advanced inference
* configuration management

The router should NOT become:

* a database server
* a large application server
* a Node.js runtime
* a Python environment
* a Docker host
* an LLM inference server

unless a particular hardware platform is later proven capable of supporting such workloads.

---

# 3. Important Conceptual Distinction

Detectic is **not initially a person-identification system**.

Do not assume:

```text
MAC address = person
```

That assumption is incorrect.

A person can have:

```text
smartphone
laptop
tablet
smartwatch
TV
other devices
```

A single device can also be shared by multiple people.

Additionally, modern operating systems may use MAC address randomization.

Therefore the correct conceptual model is:

```text
Wi-Fi device
     │
     ▼
Observations
     │
     ▼
Temporal behavior
     │
     ▼
Presence / activity inference
     │
     ▼
Higher-level interpretation
```

Human identity, if ever implemented, must be a separate and explicitly configured layer.

---

# 4. Current Hardware Target

The current experimental hardware is:

**TP-Link EX520V**

Known firmware identifier:

```text
EX520V124101568249n_agc3000_0945460481
```

A backup/configuration artifact has been obtained:

```text
EX520V124101568249n_agc3000_0945460481_backupcfg.bin
```

This file is an important research artifact.

Do not modify the original file.

Always create a working copy before performing analysis.

---

# 5. Current Project Stage

Detectic is currently in:

```text
HARDWARE RESEARCH
        ↓
FIRMWARE ANALYSIS
        ↓
SYSTEM ACCESS
        ↓
WI-FI CAPABILITY DISCOVERY
```

The project is **not yet** at the advanced AI/analytics stage.

The immediate objective is much more fundamental:

> Determine whether the TP-Link EX520V can reliably execute Detectic code and expose useful Wi-Fi observations.

Do not skip the hardware bring-up phase.

---

# 6. Immediate Goal

The first successful Detectic prototype should achieve:

```text
TP-Link EX520V
       │
       ▼
Linux shell
       │
       ▼
Wi-Fi observation
       │
       ▼
Detectic sensor process
       │
       ▼
Normalized event
       │
       ▼
Local aggregation
       │
       ▼
HTTPS
       │
       ▼
Detectic backend
```

The first milestone is NOT:

> "Detectic recognizes a person."

The first milestone is:

> "Detectic runs on a consumer router, observes useful Wi-Fi information, converts observations into structured events, aggregates them locally, and securely transmits them to a remote server."

---

# 7. Research Before Modification

The agent must follow this rule:

> **Never modify the router blindly.**

Before changing firmware, partitions, bootloader configuration, or system files, first understand the device.

The recommended investigation sequence is:

```text
1. Identify exact hardware
2. Identify firmware
3. Preserve backups
4. Analyze backup
5. Identify CPU architecture
6. Identify operating system
7. Identify BusyBox/system utilities
8. Identify services
9. Obtain shell access
10. Map filesystem
11. Map partitions
12. Map Wi-Fi interfaces
13. Identify available Wi-Fi tools
14. Determine what observations are available
15. Prototype the sensor
```

Only then should implementation begin.

---

# 8. Backup Safety

The original backup:

```text
EX520V124101568249n_agc3000_0945460481_backupcfg.bin
```

must remain untouched.

Create a working copy:

```bash
cp EX520V124101568249n_agc3000_0945460481_backupcfg.bin \
   detectic-router-backup.bin
```

Never overwrite the original research artifact.

If firmware modifications become necessary, maintain:

```text
original
working copy
modified copy
recovery copy
```

whenever possible.

---

# 9. Initial Firmware Analysis

Use tools such as:

```bash
file detectic-router-backup.bin
```

```bash
binwalk detectic-router-backup.bin
```

```bash
strings detectic-router-backup.bin
```

Search for:

```text
ssh
dropbear
telnet
root
busybox
uboot
mtd
console
debug
serial
init
rcS
```

For example:

```bash
strings detectic-router-backup.bin | \
grep -Ei 'ssh|dropbear|telnet|root|busybox|debug|console|mtd|uboot|init'
```

The purpose is to determine:

* whether the backup contains a filesystem
* whether it contains configuration data
* whether it contains credentials or references to credentials
* whether SSH/Dropbear exists
* whether Telnet exists
* whether debugging facilities exist
* whether bootloader information exists
* whether BusyBox exists
* whether Linux filesystem structures are present

Do not assume that a string found in the binary means the corresponding service is actually enabled.

---

# 10. Obtaining Shell Access

The primary technical objective is:

> Obtain a shell with sufficient privileges to inspect the router.

Preferred order:

### 10.1 Existing SSH

Check whether SSH is already available.

### 10.2 Existing debug/admin interface

Some embedded firmware exposes hidden or maintenance interfaces.

### 10.3 Telnet

If available, it may provide an initial shell.

### 10.4 Serial/UART

If remote shell access is unavailable, investigate the physical board for a serial console.

Typical UART pins may be:

```text
GND
TX
RX
VCC
```

Do not connect VCC unless the electrical characteristics are confirmed.

Use an appropriate USB-to-TTL/serial adapter.

### 10.5 Firmware modification

Only consider modifying firmware after understanding the image structure and having a recovery strategy.

Do not jump directly to firmware modification.

---

# 10.A. CANONICAL EX520 MANAGEMENT ACCESS — IMPORTANT

### The EX520 MUST NOT be assumed to be reachable through IPv4 192.168.0.1

The live TP-Link EX520 used by Detectic has been experimentally verified to expose its management API through **IPv6 link-local HTTP**, while IPv4 management at `192.168.0.1` may be filtered/unreachable.

Therefore, an agent MUST NOT conclude that the EX520 is inaccessible merely because:

```
192.168.0.1:80
192.168.0.1:443
192.168.0.1:22
192.168.0.1:23
```

are unreachable.

The canonical management path is currently:

```
Host
  |
  | enp2s0
  v
IPv6 link-local EX520
  |
  | HTTP/80
  v
TP-Link GTPR/GDPR API
  |
  +--> /cgi/getGDPRParm
  +--> /cgi_gdpr?9
  +--> /cgi_gdpr
  +--> gl/go
  +--> vendor OIDs
```

### VERIFIED LIVE EX520 ADDRESS

Current verified link-local address:

```
fe80::3e6a:d2ff:fe5f:abc1%enp2s0
```

EX520 MAC:

```
3c:6a:d2:5f:ab:c1
```

The IPv6 address MUST be treated as potentially dynamic.

Agents MUST verify the current IPv6 neighbor entry before connecting:

```bash
ip -6 neigh show
```

Look for the EX520 MAC:

```
3c:6a:d2:5f:ab:c1
```

Expected form:

```
fe80::3e6a:d2ff:fe5f:abc1 dev enp2s0 lladdr 3c:6a:d2:5f:ab:c1 ...
```

The interface scope `%enp2s0` is required when using the link-local address from the host.

### VERIFIED TRANSPORT

Protocol:

```
HTTP
```

Port:

```
80
```

Address:

```
http://[fe80::3e6a:d2ff:fe5f:abc1%enp2s0]/
```

The exact URL construction must correctly preserve the IPv6 zone/interface identifier.

### VERIFIED AUTHENTICATION

The existing Detectic client performs the TP-Link GTPR/GDPR authentication flow.

Known credentials are stored in the project's approved secret/configuration mechanism.

Agents MUST NOT print passwords, private keys, tokens, session cookies, RSA private keys, or other secrets in reports.

The authentication flow includes:

```
/cgi/getGDPRParm
    |
    v
session/token information
    |
    v
/cgi_gdpr?9
    |
    v
authenticated session
    |
    v
/cgi_gdpr
    |
    v
gl/go operations
```

The protocol uses the vendor's established mechanisms including:

* TokenID
* RSA signing
* AES-128-CBC encrypted responses
* session/JSESSIONID handling

Agents MUST reuse the existing implementation rather than reimplementing or guessing the protocol.

### VERIFIED CLIENT

The canonical client is:

```
python/detectic_client.py
```

Agents MUST inspect and reuse this client before creating another EX520 management client.

The Rust implementation also contains the GTPR client:

```
GtprClient
```

Agents should prefer the existing implementations and protocol handling.

### VERIFIED CLIENT CONSTRUCTOR (Python)

The canonical client constructor is:

```python
from detectic_client import GtprClient, Dialect

client = GtprClient(
    "http://[fe80::3e6a:d2ff:fe5f:abc1%enp2s0]",  # base_url (IPv6 link-local + scope)
    "user",                                        # user
    os.environ["DETECTIC_PASSWORD"],               # password — NEVER hardcode
    Dialect.GDPR_JSON,                             # dialect (default)
)
```

Signature (from `python/detectic_client.py`):

```python
class GtprClient:
    def __init__(self, base_url, user, password, dialect=Dialect.GDPR_JSON):
```

Key points:

* `base_url` must preserve the IPv6 zone/interface (`%enp2s0`); the client
  strips a trailing `/` and appends `cgi/getGDPRParm`, `cgi_gdpr?9`, etc.
* `user` is `user` (not `admin`) for the verified read/write path.
* `password` MUST come from the environment (`DETECTIC_PASSWORD`), never from
  source code or reports.
* `dialect` defaults to `Dialect.GDPR_JSON` (hex RSA signature); use
  `Dialect.GDPR_TEXT` only for the alternate login framing.
* After construction, call `client.connect()` to perform the GTPR handshake
  (getGDPRParm → login → TokenID), then use `gl`/`go`/`so` operations.

### VERIFIED RUST CLIENT

The Rust crate exposes the same client as `detectic::transport::GtprClient`:

```rust
use detectic::transport::{GtprClient, Dialect};

let mut client = GtprClient::with_dialect(
    "http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]",
    "user",
    &std::env::var("DETECTIC_PASSWORD")?,
    Dialect::GdprJson,
);
client.connect()?;
```

The `%` in the URL is percent-encoded (`%25`) because the Rust CLI takes the
URL from the environment. The CLI equivalent is:

```bash
DETECTIC_PASSWORD="$DETECTIC_PASSWORD" ./target/release/detectic \
  --url "http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]" \
  query DEV2_WIFI_APDEV_ASSOCDEV
```

---

# 10.B. EX520 DEPLOYMENT PATHS — COMPLETE MAP

This section documents every proven path to get Detectic code running on the
EX520, from read-only management to full resident autostart. Each path is
classified by what it proves (DEPLOY / PERSIST / EXECUTE / AUTOSTART) and its
current evidence status.

## Path 1 — External client (host-based, PROVEN-LIVE)

The host runs `python/detectic_client.py` (or the Rust binary) and talks to
the EX520 over the IPv6 GTPR path. Nothing is installed on the router.

```
Host (192.168.0.27)
  |  GtprClient("http://[fe80::3e6a:d2ff:fe5f:abc1%enp2s0]", "user", $DETECTIC_PASSWORD)
  v
EX520 GTPR/GDPR API (HTTP/80, IPv6 link-local)
```

* DEPLOY: not applicable (no router code)
* PERSIST: not applicable
* EXECUTE: no (read-only management)
* AUTOSTART: no
* Status: **PROVEN-LIVE** (read-only queries, `DEV2_WIFI_APDEV_ASSOCDEV`)

## Path 2 — Lifemote/Phoenix remote execution (PROVEN-LIVE)

The stock firmware's `DEV2_LIFEMOTE_AGENT` object, when set via `so`, makes
`cos` spawn `/usr/bin/phoenix.sh <URL>`, which downloads and executes the URL
as `root`:

```
GTPR so DEV2_LIFEMOTE_AGENT {enable:1, URL:...}
  -> cos -> rsl_setDev2LifemoteAgentObj
  -> /usr/bin/phoenix.sh <URL>
  -> curl <URL> > /tmp/lifemote_cpe_daemon.sh
  -> sh /tmp/lifemote_cpe_daemon.sh   (arbitrary commands as root)
```

* DEPLOY: **PROVEN-LIVE**
* PERSIST: **PROVEN-LIVE** (`enable:1` + URL survive reboot in misc_rw data model)
* EXECUTE: **PROVEN-LIVE**
* AUTOSTART: **MANUAL ONLY** — `rsl_set` is triggered by a GTPR `so`, NOT at boot
* Status: **PROVEN-LIVE** (PHASE 16–21)

### Path 2.A — Runtime re-trigger requires enable 0→1 toggle (PROVEN-LIVE 2026-08-28)

> **VERIFIED LIVE on the real EX520, not assumed.**  This is a critical
> operational finding for redeployment without a cold boot.

When `DEV2_LIFEMOTE_AGENT` already has `enable:1` (the steady state after a
successful deploy), sending another `so` with `enable:1` and a new URL does
**NOT** re-trigger `phoenix.sh`.  The data-model object is updated (the URL
changes, `success:true` is returned), but `cos` does **not** re-invoke
`rsl_setDev2LifemoteAgentObj` for a same-value update, so `phoenix.sh` is never
spawned.  The router never downloads the new payload.

The reliable runtime re-trigger is a **state transition** `enable:0 → enable:1`:

```
Step 1:  so DEV2_LIFEMOTE_AGENT {enable:0, URL:""}          # disable
Step 2:  so DEV2_LIFEMOTE_AGENT {enable:1, URL:<new URL>}   # re-enable
           ↓
         cos detects enable changed 0→1
           ↓
         rsl_setDev2LifemoteAgentObj fires
           ↓
         /usr/bin/phoenix.sh <URL>
           ↓
         downloads + executes payload as root
```

**Evidence (2026-08-28 session):**

| Action | Phoenix fired? | Payload downloaded? |
|--------|---------------|---------------------|
| `so {enable:1, URL:probe.sh}` while already `enable:1` | NO | NO (0 GETs in server log) |
| `so {enable:1, URL:bootstart.sh}` while already `enable:1` | NO | NO |
| `so {enable:0}` then `so {enable:1, URL:bootstart.sh}` | **YES** | **YES** (full download chain observed) |

The toggle re-deploy was confirmed by: `GET /bootstart.sh`, `GET /detectic.aa/.ab/.ac`,
`GET /launcher.sh`, `done?status=ok&version=dev-20260828`, and the sensor binding
`:::8787 LISTEN` — all within seconds of the `enable:0→1` transition.

**Do NOT confuse with cold-boot autostart.**  This toggle mechanism is for
*runtime re-triggering* of the Phoenix chain (redeployment without a reboot).
Cold-boot autostart is a separate claim that must be verified independently
(see Path 4 and the cold-boot verification checklist).  The toggle does NOT
prove that `cos` launches `phoenix.sh` at boot; it only proves that the
`rsl_set` apply handler fires on an `enable` state transition at runtime.

**Implication for the watchdog / Edge Supervisor:**  if the supervisor needs to
re-trigger Phoenix at runtime (sensor unhealthy, new binary version), it must
send `enable:0` first, then `enable:1` with the URL — a single `enable:1` `so`
is a no-op when the object is already enabled.

## Path 3 — bootstart.sh resident bootstrap (PROVEN-LIVE)

`phoenix.sh` is pointed at a host-served `bootstart.sh` that downloads the
split binary (`detectic.aa` + `detectic.ab`) to `/var/tmp/detectic/`,
reassembles it in `/var/tmp/detectic/detectic`, persists only the small
`launcher.sh`, `detectic.env`, and `version` files in `misc_rw`, and starts
the sensor via `launcher.sh`.

```
Host :8080  (package server)
  |  bootstart.sh, detectic.aa, detectic.ab, launcher.sh, detectic.env, version
  v
EX520 phoenix -> bootstart.sh (root)
  -> download pieces to /var/tmp/detectic/
  -> cp launcher.sh / detectic.env / version to misc_rw
  -> cat aa+ab > /var/tmp/detectic/detectic
  -> launcher.sh start
```

* DEPLOY: **PROVEN-LIVE**
* PERSIST: **PROVEN-LIVE** (survives reboot)
* EXECUTE: **PROVEN-LIVE** (`done?status=ok&ret=0` observed)
* AUTOSTART: requires Path 4
* Status: **PROVEN-LIVE** (PHASE 21)

## Path 4 — Cold-boot watchdog autostart and crash recovery (PROVEN-LIVE)

A host-side Edge Supervisor (`deploy/ex520_package/watchdog.py`) monitors the
router with a state machine (UNKNOWN → ROUTER_DOWN → ROUTER_UP → GTPR_READY →
SENSOR_STARTING → SENSOR_HEALTHY).  It uses IPv6 ping and GTPR queries for
reachability, verifies GTPR readiness, avoids duplicate Phoenix triggers with a
`min_boot_interval` guard, and re-triggers with exponential backoff if the
sensor becomes unhealthy.  After a sustained DOWN (>= DOWN_THRESHOLD) it sends
a `so DEV2_LIFEMOTE_AGENT` toggle (enable:0 → enable:1) with the bootstart URL,
re-triggering Path 3 after a cold boot.

**On-router crash recovery watchdog** (`deploy/ex520_package/detectic_watchdog.sh`):
A lightweight BusyBox shell script that runs entirely on the router.  It monitors
the sensor process + health endpoint (http://127.0.0.1:8787/health) every 30s,
and restarts the sensor via `launcher.sh` if 3 consecutive health checks fail.
This handles crash recovery autonomously — no host involvement needed.

```
Host watchdog.py / Edge Supervisor (poll 10s)
  -> ping6 / GTPR query
  -> state machine: UNKNOWN -> ROUTER_DOWN -> ARMED -> ROUTER_UP -> GTPR_READY
  -> GTPR so DEV2_LIFEMOTE_AGENT toggle (enable:0 -> enable:1)
  -> phoenix -> bootstart.sh (SHA-256 verify, atomic reassembly)
  -> launcher.sh start (sensor)
  -> detectic_watchdog.sh (on-router crash recovery watchdog)
  -> health checks via callbacks / sensor_log

On-router detectic_watchdog.sh (poll 30s)
  -> process check: /proc/*/cmdline scan for detectic*sensor
  -> health check: wget http://127.0.0.1:8787/health
  -> 3 consecutive failures -> launcher.sh stop + start
  -> exponential backoff: 30s -> 60s -> 120s -> 240s -> 300s
  -> max 10 restarts before giving up
```

* DEPLOY: **PROVEN-LIVE**
* PERSIST: **PROVEN-LIVE**
* EXECUTE: **PROVEN-LIVE**
* AUTOSTART (cold boot with host): **PROVEN-LIVE** (Tests 3+4: DOWN → UP → toggle → sensor running)
* CRASH RECOVERY (on-router, no host): **PROVEN-LIVE** (Tests 2+6: kill sensor → watchdog restarts via launcher.sh)
* COLD-BOOT AUTOSTART (no host): **BLOCKED** — no stock firmware mechanism starts misc_rw scripts at boot
* IDEMPOTENCY: **PROVEN-LIVE** (Test 5: duplicate trigger safe)
* HEALTH: **PROVEN-FROM-SOURCE** (supervisor state machine and health checks)
* Status: **PROVEN-LIVE** (2026-08-28: 6 tests passed, see `firmware_forensics/path4_watchdog_coldboot.md`)

### Path 2.A nuance: URL change trigger

After Phoenix completes (state resets to 0), sending `enable:1` with a different
URL DOES trigger Phoenix again.  This is because cos sees `state:0, enable:1` as
a trigger condition, regardless of whether `enable` changed.

| Action | Phoenix fires? | Condition |
|--------|---------------|-----------|
| `so {enable:1, URL:X}` while `enable:1, state:1` | NO | Same enable, phoenix running |
| `so {enable:1, URL:Y}` while `enable:1, state:0` | **YES** | State reset after phoenix completed |
| `so {enable:0}` then `so {enable:1, URL:X}` | **YES** | State transition 0→1 (reliable toggle) |

## Path 5 — Native firmware autostart (NOT AVAILABLE)

All stock-firmware boot hooks were audited and rejected (read-only SquashFS
rootfs, no user hooks in rcS/inittab/cron/procd, no hotplug.d user scripts,
signed firmware only). There is no legitimate native autostart.

* Status: **NATIVE_AUTOSTART = NOT_AVAILABLE** (PHASE 19A)

## Deployment summary matrix

| Path | DEPLOY | PERSIST | EXECUTE | AUTOSTART | Evidence |
|------|--------|---------|---------|-----------|----------|
| 1. External client | n/a | n/a | no | no | PROVEN-LIVE |
| 2. Lifemote/Phoenix | P | P | P | manual only | PROVEN-LIVE |
| 3. bootstart.sh | P | P | P | needs 4 | PROVEN-LIVE |
| 4. Watchdog autostart | P | P | P | P | PROVEN-LIVE |
| 5. Native autostart | – | – | – | – | NOT AVAILABLE |

## Deployment chain (current)

The canonical production chain is Path 4 → Path 3 → Path 2:

```
[host] watchdog.py  --cold boot detected-->  GTPR so DEV2_LIFEMOTE_AGENT
        |                                             |
        v                                             v
[host] package server :8080  <--downloads--  phoenix.sh (root)
        |                                             |
        v                                             v
[router] bootstart.sh -> launcher.sh -> detectic sensor (misc_rw)
        |
        v
[router] GTPR poll (own API, http://192.168.0.1) -> snapshots
        |
        v
[host] backend :8082 -> SQLite (pseudonymized events)
```

Key files:

```
deploy/ex520_package/watchdog.py      # Path 4 trigger (cold-boot autostart)
deploy/ex520_package/package_server.py # serves the deploy package (:8080)
deploy/ex520_package/bootstart.sh     # Path 3 bootstrap (run as root)
deploy/ex520_package/launcher.sh      # start/stop/restart sensor on router
deploy/ex520_package/detectic.env     # router sensor environment (secrets!)
deploy/ex520_package/emaild.py        # SMTP notifications from router
python/detectic_client.py             # Path 1 external client (reference)
src/transport.rs                      # Rust GTPR client (GtprClient)
```

## ⚠️ COLD-BOOT TESTING IMPACT ON ROUTER SERVICES — MANDATORY READ

> **NEVER reboot the EX520 router without explicit user authorization.**
> Router reboots disrupt DHCP, DNS, and internet connectivity for ALL devices
> on the LAN.  This is not a Detectic issue — it is the normal behavior of any
> router during a cold boot.

### What happens during a router reboot

When the EX520 reboots (via `ACT_REBOOT`, power cycle, or `sysrq`):

1. **All LAN devices lose connectivity** (0-60s while the router boots).
2. **DHCP leases are interrupted**: devices with active leases keep their IP
   during the outage, but new devices cannot get an IP until the DHCP server
   restarts (~30-60s after boot).
3. **DNS Relay takes 1-3 minutes to initialize**: after the router comes back
   UP, `dnsmasq` needs time to start and populate its forwarding rules.  During
   this window, `dig @192.168.0.1 <domain>` returns `SERVFAIL` and all devices
   using `192.168.0.1` as DNS resolver lose name resolution.
4. **WiFi clients disconnect**: all WiFi stations lose association and must
   reconnect.  Some devices (especially Android) may not reconnect
   automatically if the SSID was hidden or the signal is weak.
5. **NTP resynchronizes**: the router's clock may drift during the outage,
   which can cause TLS certificate validation failures on the router itself
   (affecting the sensor's backend connectivity).

### Observed timeline (2026-08-28 incident)

During Path 4 cold-boot testing, the router was rebooted **twice in 30 minutes**
(Test 3 at ~21:32 UTC, Test 4 at ~21:43 UTC).  After the second reboot:

- Router came back UP in ~30s (ping responded)
- DHCP server was available within ~60s
- **DNS Relay took ~2-3 minutes to start resolving** (`status: "Disabled"` in
  GTPR even though `enable: "1"`)
- 2 of 5 WiFi devices did not reconnect automatically
- The user reported "no DHCP, no internet" — caused by the DNS Relay startup
  delay, not by a persistent configuration problem

### Mandatory rules for any test that requires a router reboot

1. **Ask the user first.**  Never send `ACT_REBOOT` or `sysrq` without explicit
   confirmation.  State clearly: "This will reboot the router and disconnect
   all devices for 1-3 minutes.  Proceed?"

2. **Warn about the DNS delay.**  After a reboot, DNS may take 1-3 minutes to
   start working.  This is normal.  Do not report it as a bug unless it
   persists for more than 5 minutes.

3. **Wait for full recovery before testing.**  After a reboot, wait at least
   120s before declaring the router "UP" or running any health checks.  The
   sequence is:
   ```
   0s:    reboot sent
   30s:   router responds to ping (kernel up)
   60s:   DHCP server available
   90s:   WiFi APs broadcasting
   120s:  DNS Relay forwarding (dnsmasq fully initialized)
   180s:  all services stable
   ```

4. **Do not reboot more than once every 10 minutes.**  Multiple rapid reboots
   compound the disruption and may cause WiFi clients to enter a backoff state
   where they refuse to reconnect for extended periods.

5. **After testing, verify all services are restored**:
   ```bash
   # DHCP
   curl -sS http://192.168.0.1:8787/devices | python3 -m json.tool
   # DNS
   dig @192.168.0.1 google.com +short
   # Internet
   curl -sS -m 5 -o /dev/null -w "%{http_code}" http://example.com
   # WiFi clients
   curl -sS http://192.168.0.1:8787/devices | python3 -c "
   import json,sys
   for d in json.load(sys.stdin):
       print(f\"{d.get('hostname','?'):20s} active={d.get('active','?')}\")"
   ```

6. **If the user reports "no internet" after a reboot**, check DNS first:
   ```bash
   dig @192.168.0.1 google.com +short
   ```
   If it returns `SERVFAIL`, wait 60s and retry.  If it still fails after 5
   minutes, try re-enabling DNS Relay via GTPR:
   ```bash
   ./target/release/detectic --url "http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]" \
     --user user set DEV2_DNS_RELAY '{"enable":"1","stack":"0,0,0,0,0,0"}'
   ```

7. **Production deployments**: the host-side watchdog (`watchdog.py`) should
   only trigger a reboot of the *router* when the router is already
   unreachable (ROUTER_DOWN state).  It must NOT reboot the router
   proactively or on a schedule.  The watchdog's job is to detect router
   reboots and recover the sensor — not to cause router reboots.

---

## EX520 Safety Protocols — Mandatory Rules Summary

> **Full document:** `docs/EX520_SAFETY_PROTOCOLS.md`
>
> The following is a quick-reference summary.  The full document is canonical
> and must be read before any router operation.

### Risk classification (all operations MUST be classified before execution)

| Level | Authorization | Examples |
|-------|--------------|----------|
| **GREEN** | None needed | GTPR queries, sensor health checks, kill/restart sensor, host-side builds |
| **YELLOW** | Notify user | Phoenix toggle, bootstart re-deploy, firewall rule add, modify Detectic scripts |
| **ORANGE** | User MUST confirm | Router reboot, modify DNS/DHCP/WiFi/WAN/LAN settings |
| **RED** | Forbidden | Firmware flash, misc_rw overwrite, modify stock binaries, factory reset, `iptables -F` |

### Top 10 rules (memorize these)

1. **Never reboot the router without asking the user.**  State: "This will
   reboot the router and disconnect all devices for 1-3 minutes. Proceed?"
2. **Wait 180s after a reboot before declaring the router operational.**  DNS
   Relay takes 1-3 minutes to initialize.
3. **Never reboot more than once every 10 minutes.**  WiFi clients enter
   backoff if rapid reboots.
4. **Never flush firewall rules** (`iptables -F`, `ip6tables -F`).  Only add
   Detectic rules (TCP/8787 on br0).
5. **Never modify stock router services** (cos, httpd, dnsmasq, busybox).
6. **Never serve Phoenix scripts from external URLs.**  Only from
   `http://192.168.0.27:8080/`.
7. **Never deploy unverified binaries.**  SHA-256 checksums must match the
   manifest.
8. **Always set DEV2_LIFEMOTE_AGENT URL back to bootstart.sh after testing.**
   Never leave test scripts (kill_sensor.sh) as the URL.
9. **The watchdog detects reboots, it does NOT cause them.**  The host-side
   watchdog must never proactively reboot the router.
10. **After any test, verify all 9 baseline services** (DHCP, DNS, WAN, WiFi
    2.4/5GHz, Web UI IPv4/IPv6, GTPR, sensor) are operational.

### Service baseline (must remain operational)

```bash
# Quick check — all services
dig @192.168.0.1 google.com +short                    # DNS
curl -sS -m 5 -o /dev/null -w "%{http_code}" http://example.com  # Internet
curl -sS http://192.168.0.1:8787/devices              # DHCP + WiFi
curl -sS http://192.168.0.1:8787/health               # Sensor
curl -sS -o /dev/null -w "%{http_code}" http://192.168.0.1/  # Web UI
```

### Incident response: "no internet"

1. Check DNS first: `dig @192.168.0.1 google.com +short`
2. If SERVFAIL and router was recently rebooted (< 5 min): **wait 180s**, it's
   normal DNS Relay startup delay.
3. If SERVFAIL after 5 min: re-enable DNS Relay via GTPR
   (`set DEV2_DNS_RELAY '{"enable":"1"}'`).
4. If DNS works but no internet: check WAN (`curl http://example.com`).
5. If no DHCP: check `DEV2_DHCPV4_SERVER` enable status.

---

## Cold-boot autostart verification checklist (2026-08-28)

> The toggle trigger (Path 2.A) proves *runtime* re-triggering, NOT cold-boot
> autostart.  Cold-boot autostart must be verified independently with a
> controlled reboot and **no manual `enable:0→1` after the reboot**.

### COLD-BOOT TEST RESULT (2026-08-28): FAILED — native autostart does NOT work

A controlled cold-boot test was performed with the watchdog **NOT running**.
Result:

| # | Check | Result |
|---|-------|--------|
| 1 | EX520 rebooted (ACT_REBOOT) and came back UP | **PASS** (DOWN 3s, UP 30s) |
| 2 | `DEV2_LIFEMOTE_AGENT` persisted (`enable:1`, URL=bootstart.sh) | **PASS** |
| 3 | `phoenix.sh` executed without manual `so` | **FAIL** — 0 GETs in server log |
| 4 | `bootstart.sh` downloaded | **FAIL** |
| 5 | Sensor started | **FAIL** |
| 6 | `192.168.0.1:8787` reachable | **FAIL** |
| 7 | Sensor uptime compatible with fresh boot | N/A |
| 8 | No `enable:0→1` toggle sent after reboot | **PASS** (confirmed) |

**Conclusion: `cos` does NOT launch `phoenix.sh` at boot**, even when
`DEV2_LIFEMOTE_AGENT.enable=1` and `URL` are set and persisted.  This confirms
the Phase 16 static-analysis finding: `rsl_initDev2LifemoteAgentObj` is NOT in
the boot init function table; it is only called when a GTPR `so` command is
received at runtime.

**The "cold boot = PROVEN-LIVE" verdict above refers to the watchdog path
(Path 4), where the host-side Edge Supervisor detects the router coming back
UP and sends the `so` trigger.  Without the watchdog running, there is NO
autostart after a cold boot.**

Service was restored after the test via the `enable:0→1` toggle (Path 2.A),
which re-triggered phoenix and brought the sensor back to healthy
(`uptime=42s, status=healthy, 192.168.0.1:8787 → 200`).

### What WOULD constitute native cold-boot autostart

A controlled cold-boot test must confirm ALL of the following **with the
watchdog NOT running**:

1. EX520 reboots (ACT_REBOOT or power cycle) and comes back UP.
2. `DEV2_LIFEMOTE_AGENT` still has `enable:1` + bootstart URL after reboot
   (persisted in misc_rw data model — no re-configuration needed).
3. `phoenix.sh` executes **without any manual GTPR `so`** after the reboot.
4. `bootstart.sh` is downloaded and executed (observed as `GET /bootstart.sh`
   in the package server log).
5. The Detectic binary is reassembled and `launcher.sh` starts the sensor.
6. `192.168.0.1:8787/health` responds 200 (IPv4) AND
   `[fe80::...]:8787/health` responds 200 (IPv6).
7. The sensor's `uptime` is compatible with a fresh boot (low seconds, not
   thousands).
8. No `enable:0→1` toggle was sent after the reboot — the chain must be
   purely: cold boot → cos init → phoenix → bootstart → launcher → detectic.

If all 8 pass, then: **Detectic is natively installed on the EX520, on stock
firmware without modification, with persistent configuration and autostart
after cold boot.**  The CWMP/ACS line remains a separate experimental track,
not a requirement for Detectic persistence.

**Current status: native cold-boot autostart is NOT available on stock
firmware.  The watchdog (Path 4) is REQUIRED for autostart after cold boot.**

## Agent fast-path: build + deploy in one command (2026-08-27)

Any agent should be able to build and deploy from these two commands, no manual
file plumbing. All credentials come from `deploy/ex520_package/detectic.env`
(secrets) or the environment — never hardcode.

```bash
make package               # build aarch64-musl + split + flat manifest into deploy/ex520_package/
DETECTIC_PASSWORD=... ./deploy/ex520_package/deploy.sh   # reboot -> trigger -> wait -> verify
```

### What `make package` does
1. `make router-docker` — cross-compiles the on-router sensor via
   `messense/rust-musl-cross:aarch64-musl` with `--features wss,tls` (no C deps).
2. `build_package.sh` — copies the binary to `dist/`, splits into 1 MiB parts
   (`detectic.aa/.ab/.ac`), writes SHA-256 per part + full-binary hash, and emits
   a **FLAT** `manifest.json`, all directly into the served dir
   (`deploy/ex520_package/`).

### The manifest contract (critical)
`bootstart.sh` parses a **flat** manifest with flat `"detectic.XX":"<hash>"`
keys and a top-level `"detectic":"<full_hash>"`, matching `version`. It does NOT
support nested `{"files": {...}}`. `build_package.sh` now emits the correct flat
format; if you hand-write a manifest, keep it flat or the boot fails with
`manifest_full_hash_missing`.
- `sha256sum -b` is used (BusyBox `sha256sum` applet is non-functional on the
  EX520; bootstart.sh falls back to `/usr/sbin/openssl dgst -sha256`).

### What `deploy.sh` does (idempotent)
`ensure server -> reboot (ACT_REBOOT) -> wait for real down->up cycle ->
stability grace (cos init) -> GTPR so DEV2_LIFEMOTE_AGENT -> wait for sensor ->
verify stability past the phoenix lifecycle kill`. Success = post-reboot
autonomous verification, NOT merely "deploy finished". Flags:
`--package` (rebuild), `--no-reboot`, `--verify`.

### Gotchas any agent MUST know
- **`ACT_REBOOT` is async.** Granting the `so()` before the router finishes its
  down->up cycle loses the trigger. `deploy.sh` waits for a real down->up
  transition + grace before granting.
- **Two bootscripts:** a stale `boot_detectic.sh` may persist in the EX520
  `misc_rw` data model. It is now a no-op skip (returns immediately), so it can't
  start a competing bootstrap that kills the new sensor. Leave it a no-op.
- **`setsid`/`nohup`/`printf` are NOT reliable on this BusyBox build** (applet
  list stops at `ping6`; `printf` errors "not found" and aborts the launcher).
  `launcher.sh` uses only `echo`/ash builtins and the proven
  `( trap '' 1; exec "$BIN" sensor ... ) &` pattern; the sensor's restart loop
  is the real guard against the phoenix lifecycle kill.
- **On-router sensor uses `DETECTIC_URL=http://192.168.0.1`** (NOT 127.0.0.1,
  which the web server answers with HTTP 406), and the HTTP control plane binds
  `[::]:8787` (dual-stack), reachable from the host at `192.168.0.1:8787`.
- **TLS + WSS features are REQUIRED for upload.** `cargo build --release`
  alone produces a sensor that cannot reach HTTPS/WSS. Always build with
  `--no-default-features --features wss,tls` (via `make router-docker`).

### Key files
```
Makefile                          # build, router-docker, package targets
deploy/ex520_package/build_package.sh   # split + flat manifest + version (bash)
deploy/ex520_package/deploy.sh          # idempotent reboot+trigger+verify (bash)
deploy/ex520_package/run_package_server.sh  # start/stop/status package server
deploy/ex520_package/launcher.sh         # sensor launch + restart loop (ash)
deploy/ex520_package/bootstart.sh        # bootstrap (ash, flat-manifest parser)
deploy/ex520_package/package_server.py   # serves :8080 + /done + /env_line + /version
deploy/ex520_package/trigger_deploy.sh   # bare so() trigger (credentials from env)
src/runtime.rs                           # SIG_IGN-aware signal handlers (survive cos kill)
```



The following operation has been successfully executed against the REAL EX520:

```
gl/go
```

with:

```
DEV2_WIFI_APDEV_ASSOCDEV
```

The response was successfully decrypted/parsed and returned associated Wi-Fi devices.

Observed evidence:

```
HTTP status: 200
JSESSIONID: present
gl/go status: 200
response body: ~6424 bytes
OID: DEV2_WIFI_APDEV_ASSOCDEV
RadioMac: 3C:6A:D2:5F:AB:C1
```

This is **PROVEN-LIVE access**, not mock-router evidence.

### IMPORTANT DISTINCTION

The following states are different:

```
IPv4 192.168.0.1 unreachable
    !=
EX520 unreachable
```

The current proven state is:

```
IPv4 management:
    NOT currently reachable

IPv6 link-local management:
    PROVEN-LIVE

HTTP/80:
    PROVEN-LIVE

GTPR/GDPR:
    PROVEN-LIVE

authentication:
    PROVEN-LIVE

gl/go:
    PROVEN-LIVE

DEV2_WIFI_APDEV_ASSOCDEV:
    PROVEN-LIVE
```

### REQUIRED ACCESS PROCEDURE FOR NEW AGENTS

Before claiming that the EX520 is unreachable:

1. Identify the EX520 MAC:

   ```
   3c:6a:d2:5f:ab:c1
   ```

2. Inspect IPv6 neighbors:

   ```
   ip -6 neigh show
   ```

3. Identify the corresponding IPv6 link-local address.

4. Identify the interface/scope, currently expected to be:

   ```
   enp2s0
   ```

5. Use the existing:

   ```
   python/detectic_client.py
   ```

6. Use the IPv6 link-local management path.

7. Verify authentication.

8. Perform a known safe read-only operation such as:

   ```
   DEV2_WIFI_APDEV_ASSOCDEV
   ```

9. Only after these steps fail may the agent report:

   ```
   EX520 MANAGEMENT ACCESS FAILED
   ```

### DO NOT USE AS PRIMARY DISCOVERY METHOD

Agents MUST NOT begin EX520 access investigation by assuming:

```
http://192.168.0.1
```

is the canonical management endpoint.

IPv4 may be unavailable while the router remains fully reachable through the verified IPv6/GTPR/GDPR path.

### TELNET / SSH

Telnet and SSH are currently:

```
UNKNOWN
```

unless a later live investigation proves otherwise.

Do not enable either service merely to obtain access.

Do not assume that SSH/Telnet is required for Detectic.

The GTPR/GDPR IPv6 management path is already PROVEN-LIVE.

### SECURITY

This access path MUST be used in a read-only manner unless a specific operation has been explicitly authorized.

Agents MUST NOT:

* modify firmware;
* bypass firmware signature verification;
* modify bootloader;
* modify kernel;
* modify read-only rootfs;
* change WAN/LAN/WLAN;
* change DHCP/DNS;
* change NAT/firewall/routing;
* reboot without explicit approval;
* execute undocumented mutation operations;
* expose credentials in logs or reports.

For Detectic deployment, the preferred architecture remains:

```
ORIGINAL FIRMWARE
      |
      v
existing vendor runtime
      |
      v
persistent external launcher
      |
      v
Detectic ARM64 binary
      |
      v
RF processing
```

The GTPR/GDPR API provides the currently proven management/control path for investigation. It does NOT by itself prove arbitrary shell execution, filesystem access, persistence, or autostart.

Those capabilities MUST be independently proven on the live device.

### EVIDENCE STATUS

Current canonical access status:

```
EX520 DISCOVERY       = PROVEN-LIVE
IPv6 LINK-LOCAL       = PROVEN-LIVE
HTTP/80               = PROVEN-LIVE
GTPR/GDPR             = PROVEN-LIVE
AUTHENTICATION        = PROVEN-LIVE
gl                    = PROVEN-LIVE (used by sensor collector; `go` can return 9003 for list OIDs)
go                    = PROVEN-LIVE for scalar OIDs; may return 9003 for list OIDs
DEV2_WIFI_APDEV_ASSOCDEV = PROVEN-LIVE via `gl` (sensor collector returns 6 devices)
IPv4 192.168.0.1      = REACHABLE for sensor HTTP/8787 and routed LAN; IPv4 GTPR/HTTP still unproven
Telnet                = UNKNOWN
SSH                   = UNKNOWN
misc_rw access        = PROVEN-LIVE
misc_rw_bak access    = PROVEN-LIVE
arbitrary execution   = PROVEN-LIVE (via DEV2_LIFEMOTE_AGENT /usr/bin/phoenix.sh)
persistence           = PROVEN-LIVE (split Detectic binary in misc_rw + misc_rw_bak)
manual autostart      = PROVEN-LIVE (phoenix downloads and executes bootstart.sh)
watchdog trigger      = PROVEN-LIVE (cold boot: DOWN -> UP -> GTPR so SENT; manual trigger verified)
cold boot             = PROVEN-LIVE ONLY WITH WATCHDOG (watchdog -> so -> phoenix -> bootstart -> detectic); WITHOUT watchdog, native autostart FAILS (verified 2026-08-28: cos does not launch phoenix.sh at boot)
sensor HTTP/8787      = PROVEN-LIVE (curl /health and /devices from host; IPv4 192.168.0.1:8787 + IPv6 fe80::...:8787 both 200 OK after dual-stack bind fix + firewall open)
sensor GTPR collection = PROVEN-LIVE (sensor populated 6 Wi-Fi devices via DEV2_WIFI_APDEV_ASSOCDEV gl)
Edge Supervisor health = PROVEN-LIVE (watchdog reached SENSOR_HEALTHY via TCP 8787 probe)
SHA-256 verify         = PROVEN-LIVE (OpenSSL dgst -sha256; BusyBox sha256sum applet is non-functional)
email delivery        = PROVEN-LIVE (emaild + Brevo; email_test from router sent and delivered)
startup email         = BUG FIXED (launcher.sh curl line continuation); next clean start will deliver
```

---

# 11. SSH Credentials

Do not assume:

* there is a default SSH password
* the web-admin password is the SSH password
* resetting the router automatically enables SSH
* SSH uses the same authentication mechanism as the web interface

Investigate the actual firmware implementation.

Never brute-force credentials.

---

# 12. First Shell Reconnaissance

Once shell access is obtained, collect system information before installing anything.

Run:

```bash
uname -a
```

```bash
cat /proc/cpuinfo
```

```bash
free
```

```bash
df -h
```

```bash
mount
```

```bash
ps
```

```bash
ip addr
```

```bash
ip route
```

```bash
iw dev
```

Also inspect:

```bash
cat /proc/meminfo
```

```bash
cat /proc/version
```

```bash
ls -la /
```

```bash
ls -la /etc
```

```bash
ls -la /etc/init.d 2>/dev/null
```

Determine:

* CPU architecture
* CPU model
* RAM
* storage
* filesystem type
* Linux version
* BusyBox version
* init system
* available network interfaces
* Wi-Fi interfaces
* writable directories

---

# 13. Identify Existing Tools

Check for:

```bash
which busybox
which iw
which iwinfo
which tcpdump
which hostapd_cli
which ubus
which ip
which ifconfig
```

If BusyBox exists:

```bash
busybox
```

The objective is to reuse existing system functionality whenever possible.

Do not install large packages simply because an equivalent utility already exists.

---

# 14. Wi-Fi Capability Discovery

This is one of the most important stages of Detectic.

We need to determine exactly what the router's Wi-Fi subsystem exposes.

Start with:

```bash
iw dev
```

Then inspect station information where supported:

```bash
iw dev <interface> station dump
```

Do not assume the interface is named `wlan0`.

Possible names include:

```text
wlan0
wlan1
ra0
rai0
ath0
ath1
wifi0
other vendor-specific names
```

Determine the actual interface names first.

---

# 15. Wi-Fi Data We Want

Depending on the chipset, driver, firmware and mode, Detectic may be able to obtain some combination of:

```text
MAC address
RSSI / signal strength
timestamp
interface
channel
association
authentication
disassociation
probe activity
station information
TX bitrate
RX bitrate
packet counters
connection state
```

Not all of these are guaranteed.

The implementation must be based on what the actual router exposes.

Never invent unsupported data.

---

# 16. Important Wi-Fi Observation Limitation

There is an important distinction between:

```text
devices connected to the AP
```

and:

```text
devices detectable over the wireless environment
```

The first Detectic prototype should determine exactly which category the TP-Link hardware can provide.

Do not claim that Detectic can detect arbitrary nearby devices until this capability has actually been demonstrated on the target hardware.

---

# 17. First Sensor Prototype

The first sensor should be extremely small.

Conceptually:

```text
Detectic Sensor
      │
      ├── Wi-Fi provider
      │
      ├── event collector
      │
      ├── normalizer
      │
      ├── aggregator
      │
      ├── pseudonymizer
      │
      └── transport
```

The sensor should have very few dependencies.

---

# 18. Recommended Internal Architecture

Use clear boundaries:

```text
             ┌─────────────────┐
             │ Wi-Fi Provider  │
             └────────┬────────┘
                      │
                      ▼
             ┌─────────────────┐
             │ Collector       │
             └────────┬────────┘
                      │
                      ▼
             ┌─────────────────┐
             │ Normalizer      │
             └────────┬────────┘
                      │
                      ▼
             ┌─────────────────┐
             │ Aggregator      │
             └────────┬────────┘
                      │
                      ▼
             ┌─────────────────┐
             │ Privacy Layer   │
             └────────┬────────┘
                      │
                      ▼
             ┌─────────────────┐
             │ Transport       │
             └─────────────────┘
```

Do not combine all functionality into one monolithic script if it can be avoided.

---

# 19. Wi-Fi Provider Abstraction

The Detectic core should not be tightly coupled to TP-Link.

Define a conceptual interface such as:

```text
WiFiProvider
```

Potential implementations:

```text
TPLinkProvider
LinuxWirelessProvider
OpenWrtProvider
UbiquitiProvider
GenericProvider
```

Conceptually:

```text
                    Detectic Core
                         │
                    WiFiProvider
                         │
          ┌──────────────┼──────────────┐
          │              │              │
       TP-Link        OpenWrt        Generic
```

This is important because the long-term goal is to support multiple inexpensive hardware platforms.

---

# 20. First Event Format

A normalized event may look conceptually like:

```json
{
  "sensor_id": "home-001",
  "device_id": "8e8c4c...",
  "timestamp": 1787310000,
  "event": "seen",
  "rssi": -54,
  "interface": "wlan0"
}
```

The exact schema is expected to evolve.

Do not over-design it prematurely.

The first schema should contain only information actually available from the hardware.

---

# 21. Privacy / Device Identifier

Avoid sending raw MAC addresses to the backend unless there is a strong technical reason.

Prefer:

```text
MAC
  │
  ▼
HMAC-SHA256
  │
  ▼
device_id
```

For example:

```text
AA:BB:CC:11:22:33
        │
        ▼
HMAC-SHA256
        │
        ▼
8e8c4c...
```

The resulting identifier should be:

* stable enough for the sensor's historical analysis
* derived locally
* non-readable as a MAC address
* generated using a secret key

Prefer per-sensor secrets.

The secret must never be hardcoded into public source code.

---

# 22. Local Aggregation

Do NOT send every raw observation immediately.

Bad:

```text
observation
    ↓
HTTP request

observation
    ↓
HTTP request

observation
    ↓
HTTP request
```

Better:

```text
raw observations
       ↓
deduplication
       ↓
time window
       ↓
aggregation
       ↓
batch
       ↓
HTTPS
```

Possible aggregate:

```json
{
  "sensor_id": "home-001",
  "device_id": "8e8c4c...",
  "first_seen": 1787310000,
  "last_seen": 1787310300,
  "observations": 37,
  "avg_rssi": -52,
  "min_rssi": -61,
  "max_rssi": -44
}
```

The aggregation strategy should be configurable.

---

# 23. Resource Constraints

Assume that the router has very limited resources.

Potential constraints include:

* low RAM
* low CPU
* limited flash
* read-only filesystem areas
* limited writable storage
* BusyBox-only environment
* no package manager
* no compiler
* no Python
* no Node.js
* no modern libc
* proprietary Wi-Fi drivers

Therefore:

> **Do not assume a normal Linux server environment.**

The sensor should ideally be:

* small
* static or minimally dependent
* memory efficient
* CPU efficient
* resilient
* easy to replace
* easy to recover

---

# 24. Language Choice

Do not choose a language based solely on popularity.

The choice must be determined by:

* target architecture
* available libc
* binary size
* RAM consumption
* CPU
* static linking support
* cross-compilation
* firmware compatibility
* startup time

Potential implementations:

1. C
2. Rust
3. Go
4. shell/BusyBox for early experiments

Rust is attractive if a sufficiently small compatible binary can be produced.

C may be preferable for maximum compatibility with extremely constrained embedded Linux systems.

Go may be convenient but can produce larger binaries and higher runtime overhead.

The final choice should follow measurements.

---

# 25. No Heavy Dependencies

Avoid introducing dependencies unless they provide clear value.

Do not assume the router can support:

```text
Node.js
npm
Python
pip
Docker
Redis
PostgreSQL
Ollama
LLMs
large ML frameworks
```

The router should be a sensor.

---

# 26. Backend Architecture

The initial backend should remain simple.

Conceptually:

```text
POST /api/v1/events
        │
        ▼
Detectic API
        │
        ▼
PostgreSQL
```

Initial logical data model:

```text
sensors
-------
id
name
secret
created_at

detections
----------
id
sensor_id
device_id
timestamp
event
rssi
```

The database schema can evolve later.

Do not create an unnecessarily complex distributed architecture for the first MVP.

---

# 27. Sensor Authentication

Each sensor should have its own identity.

Conceptually:

```text
sensor
   │
   ├── sensor_id
   └── secret
```

The backend should authenticate incoming sensor data.

Do not use one global secret for every deployed Detectic sensor.

Possible future mechanisms:

* HMAC authentication
* signed requests
* short-lived tokens
* device certificates

The first implementation can remain simple, but the security boundary must exist.

---

# 28. HTTPS Transport

The production transport should use HTTPS.

The sensor should support:

* TLS
* certificate validation
* authentication
* request timeout
* retry
* exponential backoff
* batching

Example:

```text
Sensor
  │
  │ HTTPS
  ▼
Detectic API
```

Do not disable certificate validation just to make development easier without explicitly isolating that behavior to a development mode.

---

# 29. Offline Operation

Detectic must tolerate temporary Internet loss.

Expected behavior:

```text
Internet available
       │
       ▼
send batches

Internet unavailable
       │
       ▼
small local buffer
       │
       ▼
Internet restored
       │
       ▼
send queued batches
```

The buffer must have a strict size limit.

Never allow telemetry to fill the router's filesystem.

---

# 30. Router Startup Integration

Once the sensor works manually, determine the appropriate startup mechanism.

Possible mechanisms:

```text
/etc/init.d/
/etc/rc.local
procd
vendor startup scripts
custom init mechanism
```

Do not install a permanent startup service until the manual sensor process is proven stable.

---

# 31. Logging

Logging must be lightweight.

Development logs may look like:

```text
[Detectic] sensor started
[Detectic] Wi-Fi provider: tp-link
[Detectic] interface: wlan0
[Detectic] observation received
[Detectic] device: 8e8c4c...
[Detectic] RSSI: -54
[Detectic] batch size: 12
[Detectic] upload successful
```

Production logging should be significantly quieter.

Avoid logging raw MAC addresses.

Avoid filling flash storage with logs.

Prefer:

* stdout during development
* syslog when appropriate
* bounded local logs
* counters/metrics where possible

---

# 32. RSSI Interpretation

RSSI is useful but must not be treated as exact distance.

RSSI is affected by:

* walls
* furniture
* people
* antenna orientation
* device orientation
* transmit power
* interference
* frequency
* channel
* hardware
* reflections
* multipath propagation

Therefore:

```text
RSSI
 ↓
signal feature
```

not:

```text
RSSI
 ↓
exact distance
```

Spatial inference requires calibration and multiple observations.

---

# 33. Presence Detection

Once reliable historical data exists, Detectic can derive:

```text
first_seen
last_seen
presence_duration
appearance_count
time_of_day
day_of_week
recurrence
```

Example:

```text
Device A

Monday     08:02
Tuesday    08:05
Wednesday  08:01
Thursday   08:03
Friday     08:00
```

Detectic can eventually infer that the device commonly appears around 08:00.

Do not implement sophisticated behavior models before obtaining real data.

---

# 34. Historical Statistics

Potential statistics include:

```text
device appeared 5 times this week

first observed:
2026-08-17 08:01

last observed:
2026-08-21 08:43

average RSSI:
-52 dBm

typical appearance:
07:50–08:15

days commonly observed:
Monday–Friday
```

These simple statistics are valuable even without machine learning.

---

# 35. Multi-Sensor Architecture

The long-term system should support multiple Detectic sensors.

Example:

```text
             Detectic Sensor A
                    │
                    │
                    ▼
              ┌───────────┐
              │           │
              │  Backend  │
              │           │
              └───────────┘
                    ▲
                    │
                    │
             Detectic Sensor B
```

With multiple sensors, Detectic may eventually infer:

* relative movement
* zones
* room transitions
* approximate location
* occupancy
* spatial patterns

---

# 36. Spatial Inference

Do not claim that a single router can accurately locate a person.

A more realistic model is:

```text
Sensor A → RSSI pattern
Sensor B → RSSI pattern
Sensor C → RSSI pattern
             │
             ▼
      temporal correlation
             │
             ▼
      spatial estimation
```

The system should use empirical calibration.

---

# 37. Machine Learning

Machine learning is a later phase.

Do not start by training a model.

First collect:

```text
high-quality observations
        ↓
historical data
        ↓
labeled/understood patterns
        ↓
features
        ↓
baseline algorithms
        ↓
ML
```

Potential ML applications:

* anomaly detection
* presence prediction
* activity classification
* occupancy estimation
* behavioral pattern recognition
* device classification
* change detection

Simple statistical methods may outperform ML during early stages.

---

# 38. Commercial Applications

Potential markets include:

## Home

* presence detection
* occupancy
* device history
* routines
* unusual device detection
* weekly statistics
* smart-home triggers

Example:

```text
"This device was detected 5 times this week."
```

## Business

* occupancy
* room utilization
* activity patterns
* device recurrence
* spatial analytics

## Retail

* traffic patterns
* recurrence
* dwell-time estimation
* zone activity

## Hospitality

* occupancy estimation
* room/area utilization
* activity patterns

## Industrial

* equipment/device presence
* zone activity
* anomaly detection
* movement patterns

All deployments must comply with applicable privacy and data-protection laws.

---

# 39. Privacy by Design

Privacy must be considered from the beginning.

Prefer:

```text
local preprocessing
        ↓
pseudonymization
        ↓
aggregation
        ↓
minimal telemetry
```

Avoid collecting information that is not necessary for the intended functionality.

Do not build the system around unnecessary raw packet storage.

Do not store raw traffic unless there is a clearly justified research requirement and the deployment explicitly permits it.

---

# 40. Security Principles

Detectic sensors are network-connected devices.

The sensor must:

* minimize exposed services
* use HTTPS
* authenticate with the backend
* protect credentials
* avoid hardcoded secrets
* validate certificates
* limit local storage
* validate updates
* support rollback where possible
* avoid unnecessary shell access
* disable debugging interfaces in production

Never leave development SSH/Telnet access exposed on production devices.

---

# 41. Secure Firmware / Sensor Updates

Future production sensors should support secure updates.

Conceptually:

```text
Detectic Backend
       │
       ▼
New sensor binary
       │
       ▼
Download
       │
       ▼
Signature verification
       │
       ▼
Install
       │
       ▼
Health check
       │
       ▼
Rollback if failed
```

Never implement unattended firmware updates without integrity/authenticity verification.

---

# 42. Hardware Abstraction

The TP-Link EX520V is the first target, not necessarily the final commercial hardware.

Eventually evaluate:

* TP-Link routers
* OpenWrt-compatible routers
* Ubiquiti access points
* older Wi-Fi routers
* inexpensive ARM devices
* inexpensive MIPS devices
* other embedded Linux platforms

Hardware selection criteria:

```text
Wi-Fi capabilities
CPU
RAM
flash
Linux accessibility
driver capabilities
monitoring capabilities
price
availability
stability
power consumption
```

Older hardware may actually be preferable if it is:

* cheap
* well documented
* easy to root
* compatible with OpenWrt
* sufficiently powerful

---

# 43. Development Philosophy

Use:

```text
inspect
  ↓
measure
  ↓
prototype
  ↓
test
  ↓
implement
  ↓
validate
```

Avoid:

```text
modify
  ↓
flash
  ↓
hope
```

Every potentially destructive action should have a recovery path.

---

# 44. Agent Rules

When working on Detectic, an agent should:

1. Inspect before changing.
2. Preserve original firmware/backups.
3. Record hardware discoveries.
4. Avoid assumptions about the router.
5. Prefer reversible changes.
6. Test small changes independently.
7. Measure resource usage.
8. Keep the sensor lightweight.
9. Avoid unnecessary dependencies.
10. Separate hardware acquisition from analytics.
11. Design for multiple hardware platforms.
12. Treat privacy as an architectural requirement.
13. Never assume unsupported Wi-Fi capabilities.
14. Never assume MAC address equals human identity.
15. Never introduce heavy infrastructure without justification.

---

# 45. Things the Agent Must NOT Do Yet

Until the hardware has been fully characterized, do NOT:

* overwrite the bootloader
* blindly flash firmware
* modify critical partitions
* destroy the original backup
* assume default SSH credentials
* brute-force authentication
* install large runtimes
* install Docker
* install Node.js
* install Python
* install PostgreSQL
* install Redis
* install Ollama
* run an LLM on the router
* build a large ML pipeline
* collect unlimited raw packets
* send every Wi-Fi observation individually
* assume RSSI equals distance
* assume MAC equals person
* implement complex multi-sensor positioning
* build a sophisticated dashboard before the sensor works

---

# 46. Development Milestones

## Milestone 0 — Hardware Discovery

Success criteria:

```text
CPU identified
Linux identified
RAM identified
storage identified
Wi-Fi interfaces identified
filesystem identified
startup mechanism identified
```

---

## Milestone 1 — Shell Access

Success criteria:

```text
Reliable shell
Sufficient privileges
System inspection possible
Recovery path documented
```

---

## Milestone 2 — Wi-Fi Observation

Success criteria:

```text
Wi-Fi interface identified
Observation mechanism identified
Device/event information captured
RSSI available if supported
Timestamp available
```

---

## Milestone 3 — Detectic Sensor

Success criteria:

```text
Detectic binary/process runs on router
Wi-Fi events are normalized
Logs are generated
Memory/CPU usage is acceptable
```

---

## Milestone 4 — Local Aggregation

Success criteria:

```text
Duplicate observations reduced
Events grouped by time window
first_seen available
last_seen available
observation_count available
RSSI statistics available
```

---

## Milestone 5 — Secure Transport

Success criteria:

```text
HTTPS communication
Sensor authentication
Batch upload
Retry
Backoff
Offline buffering
```

---

## Milestone 6 — Backend

Success criteria:

```text
API
Sensor registration
Event ingestion
PostgreSQL persistence
Basic historical queries
```

---

## Milestone 7 — Presence Analytics

Success criteria:

```text
presence sessions
frequency
duration
first/last seen
time patterns
new-device detection
```

---

## Milestone 8 — Multi-Sensor

Success criteria:

```text
multiple sensors
cross-sensor correlation
zone inference
movement estimation
```

---

## Milestone 9 — Intelligence

Success criteria:

```text
behavior models
anomaly detection
prediction
advanced classification
```

---

# 47. First MVP Definition

The first Detectic MVP is successful when the following pipeline works reliably:

```text
                 Wi-Fi Environment
                         │
                         ▼
                 ┌───────────────┐
                 │ TP-Link EX520V│
                 │               │
                 │ Detectic      │
                 │ Sensor        │
                 └───────┬───────┘
                         │
                  Wi-Fi observation
                         │
                         ▼
                    Normalization
                         │
                         ▼
                     Aggregation
                         │
                         ▼
                   Pseudonymization
                         │
                         ▼
                       HTTPS
                         │
                         ▼
                 ┌───────────────┐
                 │ Detectic API  │
                 └───────┬───────┘
                         │
                         ▼
                    PostgreSQL
                         │
                         ▼
                  Historical data
```

At this point Detectic has proven its core technical concept.

---

# 48. Exact Current Work Order

The agent should follow this order unless new evidence requires a change:

```text
[01] Identify exact EX520V hardware
       ↓
[02] Record firmware version
       ↓
[03] Preserve backup
       ↓
[04] Analyze backupcfg.bin
       ↓
[05] Identify CPU architecture
       ↓
[06] Identify Linux/BusyBox
       ↓
[07] Identify available access mechanisms
       ↓
[08] Obtain shell
       ↓
[09] Map filesystem
       ↓
[10] Map partitions
       ↓
[11] Map startup system
       ↓
[12] Map Wi-Fi interfaces
       ↓
[13] Map Wi-Fi utilities/drivers
       ↓
[14] Capture first real observation
       ↓
[15] Build minimal Detectic sensor
       ↓
[16] Produce normalized JSON
       ↓
[17] Add aggregation
       ↓
[18] Add pseudonymization
       ↓
[19] Add HTTPS transport
       ↓
[20] Build minimal backend
       ↓
[21] Store historical data
       ↓
[22] Build presence analytics
       ↓
[23] Add multi-sensor support
       ↓
[24] Add advanced intelligence
```

---

# 49. First Concrete Task

The immediate task is **firmware and hardware reconnaissance**.

On the development machine, analyze:

```bash
file EX520V124101568249n_agc3000_0945460481_backupcfg.bin
```

```bash
binwalk EX520V124101568249n_agc3000_0945460481_backupcfg.bin
```

```bash
strings EX520V124101568249n_agc3000_0945460481_backupcfg.bin | \
grep -Ei 'ssh|dropbear|telnet|root|busybox|debug|console|mtd|uboot|init'
```

The results should be documented before any firmware modification is attempted.

---

# 50. Definition of Done for the Current Phase

The current hardware-research phase is complete when we know:

```text
✓ Exact hardware
✓ CPU architecture
✓ RAM capacity
✓ Flash/storage layout
✓ Linux version
✓ BusyBox/system environment
✓ Filesystem structure
✓ Boot/startup mechanism
✓ Wi-Fi interfaces
✓ Wi-Fi driver/stack
✓ Available observation mechanisms
✓ Available RSSI information
✓ Available station/event information
✓ Shell access method
✓ Recovery method
✓ Feasible Detectic binary format
```

Only then should implementation of the permanent router-side Detectic sensor begin.

---

# 51. Long-Term Vision

The long-term Detectic architecture is:

```text
                         DETECTIC CLOUD
                    ┌─────────────────────┐
                    │ API                 │
                    │ PostgreSQL          │
                    │ Analytics           │
                    │ Pattern Engine      │
                    │ ML/AI               │
                    │ Device Management   │
                    └──────────┬──────────┘
                               │
                    ┌──────────┼──────────┐
                    │          │          │
                    ▼          ▼          ▼
                Sensor A   Sensor B   Sensor C
                    │          │          │
                ┌───▼───┐  ┌──▼────┐  ┌──▼────┐
                │Router │  │Router │  │AP     │
                │       │  │       │  │       │
                │Sensor │  │Sensor │  │Sensor │
                └───────┘  └───────┘  └───────┘
```

The ultimate goal is to make Detectic a **software-defined sensing platform** where inexpensive network hardware can become intelligent environmental sensors.

---

# 52. Final Principle

The entire project should evolve from the bottom up:

```text
HARDWARE
   ↓
ACCESS
   ↓
WI-FI CAPABILITY
   ↓
OBSERVATION
   ↓
SENSOR
   ↓
EVENTS
   ↓
AGGREGATION
   ↓
BACKEND
   ↓
HISTORY
   ↓
PATTERNS
   ↓
PRESENCE
   ↓
MULTI-SENSOR
   ↓
INTELLIGENCE
```

**Do not build intelligence on top of data that we have not yet proven we can reliably collect.**

The most important objective right now is:

> **Make an inexpensive consumer router reliably function as a Detectic sensor while preserving router stability, minimizing resource usage, protecting privacy, and keeping the implementation portable to other hardware.**

---

# 53. Event-Driven Architecture (Phases 1–8 / 13)

DETECTIC is moving from snapshot uploads to a canonical, privacy-safe event envelope.

## Canonical event envelope

Every event is an `EventEnvelope` with:

```
event_id        — deterministic per sensor
sequence        — monotonic u64 per sensor
sensor_id       — declared sensor identity
timestamp       — epoch seconds UTC
type            — dot-notation lifecycle (device.connected, device.disconnected, ...)
device_id       — HMAC pseudonym (never raw MAC)
payload         — event-specific JSON
```

Raw MACs, IPs and hostnames never leave the sensor. The `device_id` is `HMAC(sensor_secret, raw_mac)`.

## State machine

Devices transition through:

```
UNKNOWN → CONNECTED ↔ SUSPECTED_ABSENCE → DISCONNECTED → ABSENT
            ↑                                  ↑
            │                                  │
            └────── RF evidence ---------------┘
```

`process_associated` handles GTPR station polls; `process_rf_evidence` will handle non-associated probe evidence if a future hardware path becomes available; `process_networks` handles nearby AP site survey.

## Sessions

Each confirmed connection creates a `ConnectionSession` with:

- `session_id` (deterministic)
- `started_at`, `ended_at`, `duration_seconds`
- `band`, `last_signal`, `last_noise`

`DeviceSummary` tracks first/last seen, current/last session, total connected time and connection count.

## Transport

The sensor uses `ReliableQueue` + `SpoolEventTransport` + `HttpEventTransport`:

- `ReliableQueue` bounds in-memory pending events (4096 events / 4 KiB each by default).
- `HttpEventTransport` POSTS batches to `POST /api/v1/events` with HMAC-SHA256 replay-protected signatures.
- `SpoolEventTransport` writes undelivered events to a bounded JSONL spool (`detectic_events.jsonl`) and drains it before each flush.
- Observation continues when Cloudflare is offline.

## WebSocket (WSS): PROVEN-LIVE for EX520 → Cloudflare (2026-08-26)

A direct WebSocket Secure (WSS) connection from the EX520 to the Cloudflare
Durable Object is **PROVEN-LIVE** and is the preferred transport for real-time
event delivery.

### Implementation

- `src/wss_transport.rs` uses `tungstenite` (pure Rust, no C deps, no async
  runtime) with the `wss` cargo feature.
- The sensor connects to `wss://detectic.24hwww.workers.dev/ws?role=sensor&sensor_id=ex520-001`.
- The Durable Object (`RealtimeHub` in `backend/cf-worker/src/realtime.ts`)
  accepts the WSS connection, receives `event` messages, and:
  1. Updates in-memory device/network state for real-time fan-out
  2. **Persists each event to D1** via `persistEventToD1()` (INSERT OR IGNORE)
  3. Broadcasts to subscribed frontend WebSocket clients
  4. Sends push notifications if configured
- No authentication is required on the WSS endpoint (sensor_id is passed as
  a query parameter).

### Build requirements

The `wss` and `tls` features are REQUIRED for the router build:

```bash
make router
# => cargo build --release --target aarch64-unknown-linux-musl \
#    --no-default-features --features wss,tls
```

Binary size: ~2.38 MB (statically linked, stripped, aarch64-musl).

### Environment configuration

```
DETECTIC_BACKEND_URL=wss://detectic.24hwww.workers.dev/ws
DETECTIC_MDNS=1
DETECTIC_URL=http://192.168.0.1
```

### Verified metrics (2026-08-26)

- `INFO_wss_connected` on every poll cycle
- `events_flush_sent=59_kept=0_dropped=0` (all events delivered, none spooled)
- 405+ events in D1, last received <30s ago
- Event types: `network.detected`, `rf.environment_snapshot`,
  `device.connected`, `device.signal_changed`
- Dashboard: 8 devices, 72 APs, 10 distinct devices

### Previous assessment (superseded)

The earlier assessment that WebSocket was "NO-GO" was based on the assumption
that a WebSocket library would add too much size or C dependencies. The
`tungstenite` crate proved to be pure-Rust, lightweight (~2.38 MB total
binary), and compatible with the musl static build. The synchronous
(non-async) API fits the sensor's single-threaded poll loop.

## Proximity

`ProximityBucket` (MediaTek RCPI-based) and `ProximityConfidence` are included in `device.connected` and `device.signal_changed` payloads. Confidence is `none` until calibrated samples are available.

---

# 54. AP / RF Intelligence Capability Matrix (Phases 1–7 / 10)

This matrix summarizes what the EX520 can realistically provide and the conditions for each capability.

## EX520 near-AP observations (`iwpriv get_site_survey`)

| Field | Source | Confidence | Status | Notes |
|-------|--------|------------|--------|-------|
| BSSID | `get_site_survey` table | **CONFIRMED** | SAFE-TO-IMPLEMENT | Used as `bssid_pseudonym` via HMAC. |
| SSID | `get_site_survey` | **CONFIRMED** | SAFE-TO-IMPLEMENT | May be empty for hidden networks. |
| Channel | `get_site_survey` | **CONFIRMED** | SAFE-TO-IMPLEMENT | Parsed as `u32`. |
| Band | interface name `rai0`/`rax0` | **CONFIRMED** | SAFE-TO-IMPLEMENT | Inferred from `iwpriv` interface. |
| Signal % | `get_site_survey` | **CONFIRMED** | SAFE-TO-IMPLEMENT | Converted to dBm as `%(2) - 100`. |
| Security | `get_site_survey` | **CONFIRMED** | SAFE-TO-IMPLEMENT | e.g. `WPA2PSK/AES`. |
| W-Mode (PHY) | `get_site_survey` | **CONFIRMED** | SAFE-TO-IMPLEMENT | e.g. `11b/g/n/ax`, `11a/n/ac/ax`. |
| ExtCH | `get_site_survey` | **CONFIRMED** | SAFE-TO-IMPLEMENT | `NONE`, `ABOVE`, `BELOW`. |
| Frequency (Hz) | derived from channel + band | **CONFIRMED** | SAFE-TO-IMPLEMENT | Derivable at runtime. |
| OUI / vendor | derived from BSSID | **CONFIRMED** | SAFE-TO-IMPLEMENT | First 3 octets of BSSID. |
| Channel width | NOT EXPOSED | - | NO-GO | Not in `get_site_survey` output. |
| HT/VHT/HE full IEs | NOT EXPOSED | - | NO-GO | No IE parsing from beacons. |
| Transmit power | NOT EXPOSED | - | NO-GO | Not in survey. |
| Per-chain RSSI | NOT EXPOSED | - | NO-GO | Survey only reports aggregate signal %. |
| RCPI / noise per AP | NOT EXPOSED | - | NO-GO | No per-AP noise in survey. |
| Beacon interval | NOT EXPOSED | - | NO-GO | Not in survey. |

## AP temporal / session model

| Capability | Status | Implementation |
|------------|--------|----------------|
| Online/stale/offline states | **CONFIRMED-GO** | `TemporalEngine::process_networks` uses `TrackedNetwork` and configurable `missing_polls_to_disconnect`. |
| AP first/last seen | **CONFIRMED-GO** | Tracked via `TrackedNetwork` first/last seen. |
| AP session counting | **CONFIRMED-GO** | `TemporalEngine` emits `network.detected` and `network.disappeared`; sessions are implicit between these events. |
| RSSI statistics (avg/min/max/variance) | **CONFIRMED-GO** | `RFEnvironmentSnapshot` computes this per poll. |
| Channel change events | **CONFIRMED-GO** | `network.changed` emitted on channel change. |
| Security/W-Mode/ExtCH change events | **CONFIRMED-GO** | `network.changed` emitted on these field changes. |
| RF environment snapshot | **CONFIRMED-GO** | `TemporalEngine::rf_environment_snapshot` emits `rf.environment_snapshot` event. |

## Same-LAN AP / device discovery

| Mechanism | Source | Status | Notes |
|-----------|--------|--------|-------|
| LAN host table (hostname, IP, MAC, type) | `DEV2_HOST_ENTRY` GTPR | **CONFIRMED** | SAFE-TO-IMPLEMENT for known LAN devices. |
| Associated Wi-Fi client details | `DEV2_WIFI_APDEV_ASSOCDEV` | **CONFIRMED** | SAFE-TO-IMPLEMENT already used by collector. |
| DHCP leases LAN | `DEV2_DHCPV4_CLIENT` | **NO-GO** | Returns WAN client, not LAN leases. |
| ARP/IPv6 neighbor read-only | `/proc/net/arp`, `/proc/net/ipv6_neigh` | **CONDITIONAL-GO** | Requires safe read-only file access on EX520. Not yet validated. |
| mDNS/SSDP/LLDP passive | NOT AVAILABLE | **NO-GO** | No tools on stock firmware; no active scanning. |
| OUI correlation AP BSSID ↔ LAN MAC | derived from `DEV2_HOST_ENTRY` + BSSID | **CONDITIONAL-GO** | Only works if AP has a LAN IP/MAC visible to the EX520. |

## EasyMesh / IEEE 1905

| Capability | Standalone | Requires mesh | Requires controller/agent | Status |
|------------|------------|---------------|---------------------------|--------|
| Topology discovery | - | YES | YES | NOT USABLE STANDALONE |
| Neighboring node list | - | YES | controller | NOT USABLE STANDALONE |
| Backhaul metrics | - | YES | controller/agent | NOT USABLE STANDALONE |
| Unassociated STA link metrics | `getUnassocStaLinkMetrics` exists in HAL | YES | controller/agent trigger | **NO-GO standalone** |
| Client steering / roaming | - | YES | controller | NOT USABLE STANDALONE |

**Decision:** EasyMesh unassociated-STA and remote-AP client data is **NO-GO on a standalone EX520**. It becomes **CONDITIONAL-GO** only if the network is configured with one or more EasyMesh/OneMesh peers and the EX520 is the controller. Multi-AP must be a separate, explicit deployment test.

## AP communication / remote information

| Case | Possibility | Evidence | Decision |
|------|-------------|----------|----------|
| RF observation of AP-B from EX520 | **CONFIRMED** | `get_site_survey` | Safe, implemented. |
| LAN communication with AP-B (same LAN) | **CONDITIONAL-GO** | `DEV2_HOST_ENTRY` if AP is on LAN | Implement read-only, no active scanning. |
| AP management API from EX520 | **NO-GO** | No generic AP query path | Do not attempt unless AP exposes known management API and user authorizes. |
| Mesh protocol data from another AP | **NO-GO standalone** | EasyMesh requires mesh role | Classified as mesh-only. |
| Client information from remote AP | **NO-GO standalone** | Requires EasyMesh or AP management | Same as above. |
| Remote RF info from AP-B | **NO-GO** | AP-B would have to observe and expose it; not available | Do not assume. |

## External RF sensor

| Capability | Status | Notes |
|------------|--------|-------|
| USB Wi-Fi monitor adapter | **CONDITIONAL-GO** | External hardware required; no EX520 firmware changes. |
| OpenWrt SBC as probe | **CONDITIONAL-GO** | Recommended if dual-band probe needed. |
| Probe observation ingestion | **CONFIRMED-GO** | `process_rf_evidence` in `temporal.rs` already exists. |
| Multi-sensor fusion | **CONDITIONAL-GO** | Future backend work; not blocked. |

## Implementation risk summary

| Risk | Mitigation |
|------|------------|
| Active scanning other networks | **DO NOT IMPLEMENT.** Only use existing firmware read paths. |
| Credential exposure | Reuse existing GTPR auth; never expose `DEV2_USER_CFG`. |
| MAC privacy | HMAC pseudonymization already in place. |
| Firmware modification | **NO-GO.** No flashing, no signature bypass. |
| Mesh dependency | Document as conditional; do not implement for standalone EX520. |

---

# 55. Retention and Privacy Model (Phases 14–15)

## Retention

| Layer | Storage | Default retention | Notes |
|-------|---------|-------------------|-------|
| Raw sensor events | D1 `events` table | configurable (recommend 30–90 days) | Idempotent by `event_id`; duplicates rejected. |
| AP state | D1 `ap_state` table | persistent | Last known state per `(sensor_id, ap_id)`; only pseudonyms. |
| AP sessions | D1 `ap_sessions` table | persistent | Summarized sessions; no raw MAC. |
| RF snapshots | D1 `rf_environment_snapshots` | configurable (recommend 30–90 days) | Aggregated; top AP list uses pseudonyms. |
| Device state/sessions | D1 `device_state` / `device_sessions` | persistent | Already in schema. |
| Spool (sensor) | `detectic_events.jsonl` | bounded 64 KiB | Circular / capped file on router. |
| Legacy snapshot spool | `detectic_buffer.jsonl` | bounded | Separate from canonical events. |

**Recommendation:** prune `events` and `rf_environment_snapshots` older than the configured retention at the backend (D1 SQL `DELETE` job or scheduled Worker). Keep `ap_state` and `ap_sessions` as long-term history.

## Privacy audit

- **Raw MACs:** never leave the sensor. All identifiers are `HMAC(sensor_secret, mac)`. The backend only receives pseudonyms.
- **SSIDs:** broadcast by APs; stored in `ap_state.ssid` for context. No personal data.
- **Hostname/IP:** only for associated devices via `DEV2_WIFI_APDEV_ASSOCDEV` / `DEV2_HOST_ENTRY`; these are already part of the existing snapshot flow.
- **Credentials:** GTPR auth is the only credential path. `DEV2_USER_CFG` is **not** accessed.
- **Transport:** HTTPS with HMAC-SHA256 + timestamp replay protection. Spool is on the router only.
- **EasyMesh/remote APs:** **NO-GO** unless explicitly configured; no data from other networks is accessed.

---

# 56. Final Deliverable — AP / RF Intelligence (Fase 20)

## 1. Executive Summary

DETECTIC can evolve from a snapshot-oriented sensor into a temporal, event-driven AP and RF intelligence platform. On the stock TP-Link EX520 the realistic gains are:

- **Near-AP discovery** with BSSID, SSID, channel, signal %, security, PHY mode and extension channel via `iwpriv get_site_survey`.
- **AP temporal tracking** (detected, changed, disappeared) with sessions and signal statistics.
- **RF environment snapshots** with AP counts, band/channel distributions, top APs, RSSI min/max/avg/variance.
- **Same-LAN device context** via `DEV2_HOST_ENTRY` and `DEV2_WIFI_APDEV_ASSOCDEV` already in use.
- **External RF sensor** is the only viable path for unassociated Wi-Fi device probe capture.
- **EasyMesh / remote-AP client data is NO-GO on a standalone EX520.**

No firmware modification, no flashing, no signature bypass and no active network scanning were required. All identifiers are pseudonymized with HMAC before leaving the sensor.

## 2. What the EX520 can actually provide

- Associated station list (GTPR/GDPR `DEV2_WIFI_APDEV_ASSOCDEV`) with hostname, IP, MAC, RSSI, noise, band, standard, rates, association time.
- LAN host table (`DEV2_HOST_ENTRY`) for IPv4/IPv6, MAC, hostname, client type.
- Nearby AP site survey (`iwpriv get_site_survey`) with the fields in §54.
- Per-poll RF environment statistics derived from the survey.
- Legacy snapshot upload (preserved) and new canonical event stream.

## 3. What nearby APs can reveal

Identity: BSSID, SSID, OUI/vendor, security, PHY mode.
Radio: band, channel, extension channel, signal % converted to dBm.
Environment: AP density per band, channel crowding, strongest/weakest APs, variance.
Changes: channel hop, band hop, security change, W-Mode change, signal delta >= threshold.

## 4. What AP communication can reveal

A. RF observation of AP-B from EX520: **CONFIRMED** (`get_site_survey`).
B. LAN communication with AP-B: **CONDITIONAL-GO** only if AP-B is a host on the same LAN and appears in `DEV2_HOST_ENTRY`.
C. AP management API from EX520: **NO-GO** (no generic query path).
D. Mesh protocol data from AP-B: **NO-GO standalone** (requires EasyMesh controller/agent).
E. Client information from AP-B: **NO-GO** unless AP-B exposes it through EasyMesh or a known authorized API.
F. Remote RF info from AP-B: **NO-GO** (not inherently carried by the RF signal).

## 5. What same-LAN APs can reveal

- If an AP is also a LAN host: IP, MAC, hostname, client type via `DEV2_HOST_ENTRY`.
- OUI correlation between BSSID (site survey) and LAN MAC can strengthen the hypothesis that an RF AP is a known managed device.
- No active probing, SNMP or mDNS/SSDP is used.

## 6. What EasyMesh / 1905 can reveal

- `libtp1905.so`, `mapController`, `mapAgent` and `nrd` exist on stock firmware.
- They only exchange meaningful data when the EX520 participates in a controller/agent mesh.
- Standalone: **NO-GO** for unassociated-STA metrics, topology, remote client lists and backhaul metrics.
- With a configured EasyMesh/OneMesh network: **CONDITIONAL-GO** but requires a separate validation phase and user authorization.

## 7. What remote AP client information is possible

On a standalone EX520: **NO-GO.**
With EasyMesh controller: **CONDITIONAL-GO** if the protocol carries the data and the remote AP exposes it.
With authorized SNMP/vendor API on AP-B: **CONDITIONAL-GO** only if explicitly configured and credentials are managed by the user.

## 8. What historical AP intelligence can be built

`ap_state` and `ap_sessions` support:
- first_seen / last_seen
- online duration and session count
- availability percentage
- average / min / max / variance RSSI
- channel change count and channel history
- signal trend (improving / degrading over time)
- stability / anomaly baselines (compare RF snapshots)

## 9. What the external RF sensor adds

- Probe Request capture for unassociated devices (RSSI, SSID, band, channel, capabilities, IEs, randomized MAC).
- Same `EventEnvelope`, `ReliableQueue`, `SpoolEventTransport` and cryptographic mechanisms used by the EX520.
- `process_rf_evidence` in `temporal.rs` is already available to ingest probe observations.

## 10. What multiple sensors add

- Per-sensor device_id + RSSI + timestamp.
- Closest-sensor inference by RSSI comparison.
- Zone classification (NEAR sensor-A, FAR from sensor-B).
- Trajectory and handoff between sensors.
- Fusion confidence (not deterministic identity).

## 11. Capability matrix

| Capability | EX520 stock | + LAN data | + EasyMesh | + External RF sensor | + Multi-sensor |
|------------|-------------|------------|------------|----------------------|----------------|
| Associated device tracking | GO | GO | GO | GO | GO |
| Nearby AP detection | GO | GO | GO | GO | GO |
| AP temporal state | GO | GO | GO | GO | GO |
| RF environment snapshot | GO | GO | GO | GO | GO |
| Unassociated device detection | NO-GO | NO-GO | NO-GO | GO | GO |
| Remote AP client data | NO-GO | CONDITIONAL | CONDITIONAL | NO-GO | CONDITIONAL |
| Mesh topology | NO-GO | NO-GO | CONDITIONAL | NO-GO | CONDITIONAL |
| Multi-sensor positioning | NO-GO | NO-GO | NO-GO | NO-GO | CONDITIONAL |

## 12. Evidence matrix

| Claim | Evidence | Source | Status |
|-------|----------|--------|--------|
| `get_site_survey` exposes BSSID/SSID/signal/security/W-Mode/ExtCH | Sample output parsed in `monitor.rs` tests | EX520 live + tests | PROVEN |
| EX520 cannot capture unassociated Probe Requests | `cfg80211` absent, `iwpriv` unassoc stub returns 9003, no `tcpdump` | RF report + system inspection | PROVEN-NOGO |
| `DEV2_HOST_ENTRY` gives LAN hosts | GTPR `getList` fields documented | API findings | PROVEN |
| EasyMesh STA metrics require mesh | HAL function not wired to web API; needs controller/agent protocol | RF report | PROVEN-NOGO standalone |
| HMAC pseudonymization protects identity | `crypto::pseudonymize` used in `service.rs` and `temporal.rs` | Code review | PROVEN |

## 13. Risk matrix

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Active scanning of neighbor networks | Low if code is read-only | High (legal/privacy) | Only use existing `iwpriv get_site_survey`. |
| MAC privacy leak | Low (HMAC in place) | High | Never serialize raw MAC in `EventEnvelope`. |
| Credential leak via GTPR | Low if not used | High | Reuse existing auth; never call `DEV2_USER_CFG`. |
| Firmware modification | Not applicable | Catastrophic | Explicitly forbidden in project rules. |
| Mesh misconfiguration | Low until enabled | Medium | Classify conditional; require explicit test. |
| Storage exhaustion on router | Medium | Medium | Bounded spool (64 KiB) and in-memory queues. |

## 14. Architecture diagram

```text
                    DETECTIC BACKEND
        ┌──────────────────────────────────┐
        │ D1: events, ap_state,            │
        │ ap_sessions, device_state,       │
        │ rf_environment_snapshots         │
        │ Analytics / API / UI             │
        └──────────────┬───────────────────┘
                       │ HTTPS (HMAC + spool)
         ┌─────────────┼─────────────┐
         │             │             │
         ▼             ▼             ▼
      EX520        AP network    RF sensor(s)
         │             │             │
    GTPR / site       EasyMesh     monitor mode
    survey            (optional)   probe requests
         │             │             │
         └─────────────┼─────────────┘
                       ▼
              TemporalEngine
                       │
         ┌─────────────┼─────────────┐
         ▼             ▼             ▼
      APState     DeviceState     RFState
         │             │             │
         └─────────────┴─────────────┘
                       ▼
                  DETECTIC UI
```

## 15. Proposed data model

Implemented / proposed:
- `sensors`, `events`, `sensor_sequences`
- `device_state`, `device_sessions`
- `ap_state`, `ap_sessions`
- `rf_environment_snapshots`
- `snapshots`, `detections` (legacy, preserved)

See `backend/cf-worker/schema.sql`.

## 16. Temporal state machine

```text
AP UNKNOWN → ONLINE (network.detected)
      ↑
      ONLINE ── missing polls ──> OFFLINE (network.disappeared)
      ↑                            │
      └──── re-detected ───────────┘
```

Device state machine remains in `temporal.rs`: `UNKNOWN → CONNECTED ↔ SUSPECTED_ABSENCE → DISCONNECTED → ABSENT`.

## 17. Event model

Canonical `EventEnvelope` with `event_id`, `sequence`, `sensor_id`, `timestamp`, `type`, `device_id`, `payload`.

AP events:
- `network.detected`
- `network.changed`
- `network.disappeared`
- `rf.environment_snapshot`

Device events:
- `device.connected`, `device.disconnected`, `device.signal_changed`, `device.band_changed`, `device.network_changed`, `device.presence_changed`

## 18. Storage / retention model

- Raw `events` and `rf_environment_snapshots`: configurable (recommended 30–90 days).
- `ap_state`, `device_state`, `ap_sessions`, `device_sessions`: persistent state.
- Router spool: bounded 64 KiB JSONL file; circular on overflow.
- Legacy `snapshots`/`detections` preserved for backward compatibility.

## 19. Privacy / security model

- All identifiers are HMAC pseudonyms (`HMAC(sensor_secret, raw_mac)`).
- No raw MACs in `EventEnvelope`, backend or D1 (except legacy tables during transition).
- HTTPS + HMAC-SHA256 request signature + timestamp anti-replay.
- No active scanning, no credential harvesting, no remote network access.
- EasyMesh/remote APs require explicit authorization and validation.

## 20. Implementation roadmap

| Phase | Deliverable | Status |
|-------|-------------|--------|
| P0 — Inventory | Evidence review, no duplication | Done |
| P1 — Near-AP fields | `monitor.rs` extended | Done |
| P2 — AP temporal | `TemporalEngine::process_networks` | Done |
| P3 — RF snapshot | `rf_environment_snapshot` event | Done |
| P4 — AP/RF backend | `ap_state`, `rf_environment_snapshots` schema + side effects | Done |
| P5 — External RF sensor | Probe ingestion / `process_rf_evidence` | Ready, needs hardware |
| P6 — Multi-sensor fusion | Backend correlation | Future |
| P7 — UI / analytics | Dashboard for AP view, RF history | Future |

## 21. Exact implementation tasks completed

- `src/monitor.rs`: `NearbyObservation` fields + parser + tests; parser fixed for live `No Ch ... Siganl(%)` site-survey layout.
- `src/temporal.rs`: `NetworkObs`, `TrackedNetwork`, `process_networks`, `rf_environment_snapshot`, `EventType::RfEnvironmentSnapshot`, `ProbeObservation`, `process_probes`.
- `src/service.rs`: wire `NetworkObs` and `rf.environment_snapshot` to `ReliableQueue`.
- `src/calibrate.rs`: `ProximityConfidence::as_str`.
- `backend/cf-worker/schema.sql`: `ap_state`, `ap_sessions`, `rf_environment_snapshots`.
- `backend/cf-worker/src/index.ts`: `applyApSideEffects`, `applyRfSnapshot`, routing in `handleEventBatch`, `handleNetworks` querying `ap_state` and `rf_environment_snapshots`, incremental min/max/avg/variance via Welford in D1 SQL.
- `AGENTS.md`: event architecture (#53), AP/RF matrix (#54), retention/privacy (#55), final deliverable (#56).
- `investigations/ap_rf_intelligence_report.md`: complete report with live validation of GTPR and `iwpriv get_site_survey`.
- `cargo test --release`: 177/177 passing.
- `npx tsc`: Cloudflare worker compiles.

## 22. Tests

- Rust unit tests: 177 passing.
- Worker TypeScript: `npx tsc -p tsconfig.json --noEmit` clean.
- Live EX520 validation: GTPR/GDPR handshake, `DEV2_WIFI_APDEV_ASSOCDEV`, `DEV2_HOST_ENTRY` and `iwpriv get_site_survey` (via `DEV2_LIFEMOTE_AGENT`, rolled back).

## 23. Rollback strategy

- All code changes are additive.
- Legacy snapshot path (`snapshots`/`detections`) is preserved and still emitted in `service.rs`.
- New canonical events are written to a separate `detectic_events.jsonl` spool; legacy spool is not modified.
- Backend schema uses `CREATE TABLE IF NOT EXISTS`; no destructive migrations.
- To disable canonical events, set `backend_url` to empty in sensor config; sensor falls back to local spool/null transport.

## 24. Remaining unknowns

- Real `iwpriv get_site_survey` output on the EX520 (the parser was validated against a representative sample).
- Exact band/channel mapping for 6 GHz if the EX520 variant supports it (not in current target).
- Performance of `rf_environment_snapshot` with 100+ APs (bounded by `max_tracked_networks = 512`).
- Multi-sensor fusion behavior with overlapping coverage and randomized MACs.

## 24.A. nrd event infrastructure — LIVE VALIDATION (Phase 22)

Full investigation of whether DETECTIC can passively consume Wi-Fi events
from the EX520's internal `nrd` process. See
`docs/EX520_NRD_EVENT_ANALYSIS.md` for the complete report.

### Key findings

1. **nrd uses MediaTek vendor netlink (protocol 21)** for Wi-Fi events
   (probe requests, associations, RSSI, auth failures, beacon reports).
   The socket is `socket(AF_NETLINK, SOCK_RAW, 21)` with `nl_groups=0`
   (unicast). Confirmed via `/proc/net/netlink`: nrd (PID 2743) is the
   sole recipient.

2. **Passive netlink consumption is NOT possible.** A static aarch64-musl
   probe binary was deployed via Lifemote/Phoenix and created its own
   AF_NETLINK socket on protocol 21. Result: **0 events received** in
   30 seconds. The driver unicasts to nrd's PID only.

3. **nrd's libos IPC socket at `/var/tmp/45`** only handles 2 control
   messages (`CMSG_AI_ROAMING_INFO_RECV` and
   `CMSG_EASYMESH_MAP_RELOAD_NRD`). No data query mechanism exists.

4. **iwlist/iwpriv/iwconfig are non-functional** on this firmware. All
   commands return empty output on all interfaces (rai0, rax0, etc.).
   The MediaTek driver does not support standard WEXT ioctls.

5. **/proc/net/wireless** shows all interfaces with link level -256
   (no useful signal data).

6. **GTPR polling of `DEV2_WIFI_APDEV_ASSOCDEV`** remains the only viable
   Wi-Fi data source, providing associated stations with signal strength,
   data rates, association times, and hostnames.

### Verdict

```
PASSIVE_EVENT_CONSUMPTION = NOT_POSSIBLE
NRD_IPC_QUERY             = NOT_POSSIBLE
IWLIST/IWPRIV_POLLING     = NOT_FUNCTIONAL
GTPR_POLLING              = VIABLE (proven, in production)
```

The existing host-based GTPR polling approach (Path 1) is the recommended
architecture. Probe request detection (unassociated devices) requires
either a USB monitor-mode adapter or firmware modification.

## 24.B. DETECTIC persistence — FORENSIC INVESTIGATION (Phase 23)

Full investigation of reboot persistence for DETECTIC on stock EX520.
See `docs/EX520_DETECTIC_PERSISTENCE.md` for the complete report.

### Key findings

1. **Phoenix does NOT auto-start at boot.** Despite `rsl_initDev2LifemoteAgentObj`
   containing code to launch `phoenix.sh` when `enable=1` and `URL` is set,
   the function is NOT in the boot init function table. It is only called
   when a GTPR `so DEV2_LIFEMOTE_AGENT` command is received.

2. **Live reboot test confirmed.** A controlled reboot (sysrq trigger) was
   performed with the Lifemote URL set to a detection script. The router
   rebooted (uptime went from 903s to 355s) but no auto-start callback was
   received. No phoenix.sh process was running after the reboot.

3. **No writable boot hooks exist.** The rootfs is read-only SquashFS.
   No rc.local, no user-writable hotplug.d, no crond, no procd. The only
   writable persistent storage is misc_rw (UBIFS), which is mounted before
   cos starts but has no stock mechanism to execute files from it at boot.

4. **Host-side watchdog is the ONLY proven auto-start mechanism.**
   The existing Path 4 (watchdog.py) remains the correct approach.

5. **misc_rw files persist across reboot.** The detectic binary parts,
   launcher.sh, config, and logs in `/var/run/misc/misc_rw/detectic/`
   survive reboot. Only the reassembled binary in `/var/tmp/detectic/`
   is lost.

6. **No stock mDNS conflict.** No stock service binds UDP 5353. The
   `igmp_max_memberships` is set to 64, and bridge-nf-call is disabled,
   so multicast traffic passes freely.

### Verdict

```
PERSISTENT_AUTOSTART = NO (without host-side watchdog)
NATIVE_BOOT_HOOK     = NONE
PHOENIX_AUTO_START   = NO (proven by live test)
HOST_WATCHDOG        = REQUIRED (proven mechanism)
```

## 25. Explicit NO-GO list

- Modify EX520 firmware or flash OpenWrt.
- Replace stock firmware or disable signature verification.
- Enable monitor mode / packet injection / deauthentication on the EX520.
- Run `tcpdump` or equivalent on stock EX520 (tool not present; not feasible).
- Capture unassociated Probe Requests directly on the EX520.
- Active scan / probe other networks without authorization.
- Query `DEV2_USER_CFG` or expose credentials.
- Access remote APs without user authorization and a known protocol.
- Claim precise positioning from RSSI without calibration.

---

# 55. Production Build & Deploy — Critical Requirements (2026-08-26)

## TLS + WSS features are REQUIRED for backend upload

The `ureq` HTTP client is configured with `default-features = false` in `Cargo.toml`.
Without the `tls` feature, the sensor **cannot make HTTPS requests** to the Cloudflare
Worker backend. Without the `wss` feature, the sensor **cannot establish WebSocket
Secure connections** for real-time event delivery to the Durable Object.

**Correct build command (use `make router`):**
```bash
make router
# => cargo build --release --target aarch64-unknown-linux-musl \
#    --no-default-features --features wss,tls
```

Or with Docker:
```bash
docker run --rm -v "$PWD:/home/rust/src" messense/rust-musl-cross:aarch64-musl \
  cargo build --release --no-default-features --features wss,tls
```

Then copy the binary:
```bash
cp target/aarch64-unknown-linux-musl/release/detectic dist/detectic-aarch64-musl
```

**Wrong (events will not upload):**
```bash
cargo build --release  # missing --features wss,tls
```

## GTPR URL must be 192.168.0.1, NOT 127.0.0.1

The EX520V's web server returns HTTP 406 (Not Acceptable) for GTPR requests
sent to `http://127.0.0.1`. The sensor must use `http://192.168.0.1` (the
router's own LAN IP) for GTPR polling when running on-router.

**Correct env:**
```
DETECTIC_URL=http://192.168.0.1
```

**Wrong (GTPR poll fails with 406):**
```
DETECTIC_URL=http://127.0.0.1
```

## HTTP server must bind [::] for IPv6 dual-stack

The EX520 is managed via IPv6 link-local. The sensor's HTTP control plane
must bind to `[::]:8787` (IPv6 dual-stack) instead of `0.0.0.0:8787` (IPv4
only). This is handled in `src/http_server.rs`.

## launcher.sh must unset stale env vars

The `phoenix.sh` parent process may carry stale `DETECTIC_BACKEND_URL` from
a previous deployment. The launcher.sh must `unset DETECTIC_BACKEND_URL
DETECTIC_UPLOAD_URL DETECTIC_BACKEND_TOKEN` before sourcing the env file
to prevent stale values from overriding the new configuration.

## bootstart.sh must copy env to /var/tmp/detectic/

The `launcher.sh` prefers `/var/tmp/detectic/detectic.env` over
`/var/run/misc/misc_rw/detectic/detectic.env`. The `bootstart.sh` must
copy the downloaded env file to BOTH locations, and remove stale copies
first with `rm -f`.

## mDNS must be disabled on-router

`DETECTIC_MDNS=0` in the env file. The mDNS responder fails on loopback
IPs and is not needed for health monitoring. The HTTP control plane on
port 8787 is sufficient.

## Verified end-to-end flow (2026-08-26)

```
EX520 bootstart.sh
  -> downloads split binary + env + launcher
  -> reassembles detectic binary in /var/tmp/detectic/
  -> launcher.sh starts sensor
  -> sensor polls GTPR at http://192.168.0.1
  -> iwpriv get_site_survey -> 52 nearby observations
  -> temporal engine generates device/RF events
  -> HTTPS POST to https://detectic.24hwww.workers.dev/api/v1/events
  -> 63 events received by Cloudflare Worker in 10 minutes
  -> HTTP control plane on [::]:8787 (firewall blocks external access)
```

Status: **PRODUCTION-LIVE** — sensor polling, generating events, and
uploading to backend via HTTPS with HMAC-SHA256 authentication.

## Firmware Real-Time Forensics (2026-08-27)

Full firmware forensic analysis completed. See
`docs/EX520_FIRMWARE_REALTIME_FORENSICS.md` for the complete report.

### Key Finding: Answer B — Real-time event path EXISTS

The EX520 firmware contains a **real-time Wi-Fi event path** via
**NETLINK_ROUTE** (standard rtnetlink), which was NOT tested in Phase 22.

Phase 22 tested **NETLINK protocol 21** (band steering) and correctly
concluded it doesn't work for passive event consumption. However, the
firmware analysis proves that:

1. `mt_wifi.ko` calls `wireless_send_event()` — the standard kernel
   function for delivering wireless events to user-space via NETLINK_ROUTE
2. `libplatform_api.so` receives these events via
   `driver_wext_event_rtm_newlink()` and `driver_wext_event_wireless()`
3. `apsd` receives disassociation events via `__isDisassociateEvent()`
4. `wlNetlinkTool` receives WPS/WLAN switch events via the same channel

### Event Path Architecture

```
mt_wifi.ko (driver)
  → wireless_send_event()
  → NETLINK_ROUTE (RTM_NEWLINK + IFLA_WIRELESS)
  → User-space listeners (apsd, wlNetlinkTool, libplatform_api)
```

This is SEPARATE from the band steering netlink (protocol 21) that nrd uses.

### Candidate Real-Time Paths

| Path | Confidence | Latency | Status |
|------|-----------|---------|--------|
| A: Custom NETLINK_ROUTE listener | PROBABLE | <100ms | Test binary built, needs live deploy |
| B: Rapid GTPR polling (2-3s) | CONFIRMED | 2-3s | Tested live, works |
| C: libos IPC tap | POSSIBLE | <100ms | Needs RE work |
| D: NETLINK protocol 21 | DISPROVEN | — | Phase 22 + firmware analysis |

### GTPR Event Objects

- `DEV2_WIFI_DE_ASSOC_EVENT` / `DEV2_WIFI_DE_DISASSOC_EVENT`: exist but
  return 0 instances (EasyMesh DE not active)
- `DEV2_WIFI_APDEV_ASSOCDEV`: returns 5 live instances with MAC, hostname,
  RSSI, active status, association time — **pollable at 2s intervals**

### Test Binary

`firmware_forensics/netlink_test/wifi_event_listen.c` — a static C binary
that binds to NETLINK_ROUTE and listens for wireless events. Cross-compiled
for aarch64-musl. Needs to be deployed to the EX520 via phoenix.sh and
tested with a real device connect/disconnect.

### Firmware Artifacts

- `firmware_forensics/firmware_manifest.json`
- `firmware_forensics/firmware_event_candidates.json`
- `firmware_forensics/firmware_netlink_map.json`
- `firmware_forensics/firmware_ipc_map.json`
- `firmware_forensics/firmware_strings_wifi.txt`

## Active-Status Event Generation Fix (2026-08-27)

### Problem

The EX520 GTPR `DEV2_WIFI_APDEV_ASSOCDEV` table keeps **disconnected** devices
in the list with `active="0"` instead of removing them. Detectic's temporal
engine was treating every device in the table as "associated", so
`DeviceDisconnected` was never generated on `active=1 -> active=0` transitions.

### Fix

1. `src/service.rs` now filters `active="0"` devices out of `device_obs` before
   passing them to the temporal engine.
2. `src/temporal.rs` now checks `missing_polls >= missing_threshold` even on the
   `Connected -> SuspectedAbsence` transition, allowing 1-poll disconnect when
   `missing_polls_to_disconnect: 1`.
3. `DetecticService::new` initializes the temporal engine with
   `missing_polls_to_disconnect: 1`.

### Verified Behavior

- Unit tests in `events.rs` and `temporal.rs` cover `active=1->0`, `0->1`,
  `0->0`, and `1->1` transitions.
- Real EX520 test with a Moto G42:
  - `active=1` -> `DeviceConnected` generated
  - user disconnects -> `active=0` -> `DeviceDisconnected` generated
  - user reconnects -> `active=1` -> `DeviceConnected` generated
- Device identity (pseudonym) is preserved across `active=0/1` flips.

### Implementation References

- `src/service.rs` (lines ~500-565): filters `active="0"` from `device_obs`
- `src/temporal.rs` (lines ~540-580): threshold check in `Connected` state
- `src/temporal.rs` (tests): `step2_temporal_active_*`, `step4_identity_preserved_*`

## GTPR `so DEV2_LIFEMOTE_AGENT` Payload (2026-08-27)

### Required Payload

The EX520 rejects `so` calls that omit `stack`/`pstack` or send `enable` as a
numeric. Use this exact shape:

```python
{
    "enable": "1",
    "URL": "http://192.168.0.27:8080/run_probe.sh",
    "stack": "0,0,0,0,0,0",
    "pstack": "0,0,0,0,0,0"
}
```

The `URL` may be `bootstart.sh` (normal sensor deploy) or `run_probe.sh`
(bootstart + lifecycle probe).

### Operational Notes

- The router responded `40` (connection aborted) when the payload was malformed.
- This is **not** a login lockout; the same session could still perform `gl()`
  immediately. The `40` was the firmware's `so()` parser rejecting the request.
- Canonical trigger script: `deploy/ex520_package/trigger_deploy.sh`

## Measured Latencies (2026-08-27, interval=2s)

| Segment | Typical Value |
|---|---|
| T1 -> T3 (GTPR captured -> event envelope) | 600-850 ms |
| T3 -> T4 (event generated -> transport start) | ~1.3 s (event queue flush) |
| T4 -> T5 (send -> Cloudflare received) | ~80 ms |
| T4 -> T6 (send -> ACK round-trip) | ~300 ms |
| **T1 -> T6 (end-to-end)** | **~2.3 s** |

T1 is the `captured_at` timestamp in the GTPR `DEV2_WIFI_APDEV_ASSOCDEV`
response. T3 is the `T3_ENVELOPE` log wall time. T4 is `T4_SEND`. T5 is the
Cloudflare `received_at` field. T6 is the `T6_ACK` wall time.

## Local Host Sensor

For development, validation, and comparison with the on-router sensor, the
release binary can run locally against the EX520 via IPv6 link-local:

```bash
cd /home/soporte24hwww/Documentos/Repositorios/detectic
set -a; source .env; set +a
DETECTIC_INTERVAL=2 DETECTIC_BACKEND_URL=wss://detectic.24hwww.workers.dev/ws \
  DETECTIC_MDNS=0 ./target/release/detectic sensor
```

Health and snapshots are available on `http://127.0.0.1:8787` (`/health`,
`/devices`, `/ready`).

Caution: only one sensor should use `sensor_id=ex520-001` at a time against the
same backend, or events will be attributed twice. For parallel testing, override
`DETECTIC_SENSOR_ID`.

---

# 57. Production Hardening — Security & Robustness (2026-08-28)

This section documents the production-hardening pass. Every behavior below is
covered by an automated test or an explicit configuration check. **Nothing here
is claimed PROVEN-LIVE unless it has actually been demonstrated in the running
environment.** The hardening protects protocol semantics without changing the
event envelope, the pseudonymization algorithm, or the WSS production
architecture.

## 57.1 WSS sensor authentication (PROVEN-BY-TEST)

**Problem:** `/ws` previously accepted `role=sensor&sensor_id=X` with no
credential, so any peer could inject events or read telemetry.

**Protocol (unchanged transport, added credential):**
1. Sensor opens `wss://…/ws?role=sensor&sensor_id=<id>` (the token is NEVER in
   the URL).
2. The Durable Object stores the socket as unauthenticated
   (`sensor_authed:false`) and sends a non-sensitive greeting.
3. Sensor sends `{"type":"hello","protocol":1,"token":"<secret>"}`.
4. The DO validates `token` against `DETECTIC_SENSORS[<sensor_id>]` with
   **constant-time** comparison (`protocol.ts: constantTimeEqual`). The
   `sensor_id` is only ever used as a registry key — it is never trusted on its
   own.
5. On success the DO marks the socket authenticated and replies `hello_ack`
   (then `GET_STATUS`). On any failure it replies `auth_error` and closes.

**Rules enforced:**
- Missing token → reject.
- Invalid token → reject.
- Unknown `sensor_id` → reject.
- Malformed handshake → reject.
- `sensor A` cannot authenticate as `sensor B` (its token must match A's).
- Only authenticated sensor sockets may send `type:"event"`. For those, the
  attribution `sensor_id` is the one bound at handshake time — never the one in
  the message body, so a sensor cannot impersonate another.
- Token/secret is never logged, never placed in the URL, and never echoed.

**Credential wiring:**
- The sensor presents its existing `DETECTIC_SECRET`. This value MUST equal
  `DETECTIC_SENSORS[<sensor_id>]` on the Worker — the same contract already used
  for HTTP HMAC uploads, so **no new sensor env variable is required**.
- Rebuild + redeploy the on-router binary (`make package`) so `WssEventTransport`
  sends the token. Until then, a sensor that sends no token is rejected.

**Tests:** `backend/cf-worker/tests/protocol.test.ts` (Node, `npm run test:unit`):
`# WSS sensor authentication` covers authenticated accepted, missing/invalid
token, unknown sensor, cross-sensor impersonation, and malformed handshake.
Status: **PROVEN-BY-TEST**; live confirmation requires a redeployed sensor plus a
registered secret (see the ignored live test `wss_transport::test_wss_roundtrip`
which now requires `WSS_TEST_TOKEN`).

## 57.2 CORS policy (PROVEN-BY-TEST)

**Problem:** `Access-Control-Allow-Origin` fell back to `*`, letting any web
origin read the API.

**Policy:** `Access-Control-Allow-Origin` is emitted ONLY for:
- an explicit allowed origin from `DETECTIC_ALLOWED_ORIGINS` (comma separated),
  or
- the worker's own origin (same-origin dashboard).

It is never `*`. Absent Origin, disallowed origins, and non-dashboard clients
receive **no ACAO header** (browsers block cross-origin reads). Non-browser
sensors are unaffected (server-to-server, no CORS). OPTIONS preflight reflects
the allowed origin only.

**Tests:** `protocol.test.ts` `# CORS policy` (allowed/self origin reflected;
disallowed and absent Origin → null). Status: **PROVEN-BY-TEST**.

## 57.3 Canonical event ACK contract + duplicate semantics (PROVEN-BY-TEST)

**Problem:** Rust `parse_ack` expected `accepted_ids`, but the Worker returned
`{accepted, duplicates}`; HTTP 200 parsed to an empty accepted set, so the
`ReliableQueue`/spool could grow indefinitely over the HTTP path.

**Contract (canonical, ID-keyed, never positional):**
```
POST /api/v1/events  (batch) -> 202
{
  "accepted_ids":  [<event_id>, ...],   // newly persisted
  "duplicate_ids": [<event_id>, ...],   // already known (events.event_id UNIQUE)
  "rejected_ids":  [<event_id>, ...],   // un-insertable (retained on sensor)
  "accepted": N, "duplicates": M, "rejected": K
}
```
- Rust `HttpEventTransport` resolves **accepted_ids ∪ duplicate_ids** as
  "done" (removed from queue/spool). Rejected IDs are NOT resolved, so they
  stay queued for retry (never silently dropped).
- Duplicates (re-sent events) are resolved by the backend, so the queue does
  **not** retain them forever.

**Tests (Rust):** `parse_ack_body_maps_worker_contract`,
`parse_ack_body_empty_on_malformed_or_legacy`, `queue_removes_both_accepted_and_duplicate`,
`rejected_event_survives_retry`. Status: **PROVEN-BY-TEST**.

### Duplicate semantics
- `events.event_id` is `UNIQUE`. A re-delivered event that already exists is
  reported as a **duplicate** (not a second accepted event) and is resolved.
- Accepted events are selected **by stable event_id** (not array position), via
  `protocol.ts: selectAcceptedEvents`. Duplicates and rejections can never be
  misattributed as accepted regardless of their position in the batch.
- Regression tests (protocol.ts, `# ID-keyed accepted-event selection`): all
  unique, duplicate in first/middle/end, and
  mixed accepted/duplicate/rejected.

## 57.4 Removed dead `pseudoHmac()` (PROVEN-BY-CLEAN)

The weak, non-deterministic djb2-style `pseudoHmac()` in `index.ts` was dead
code and has been **removed**. Repository-wide search finds no remaining
references. The only pseudonymization path is `crypto.subtle`-based HMAC-SHA256
(`pseudonymize()`), which is MCV1 cross-language compatible.

## 57.5 Production-safe 500 responses (PROVEN-BY-TEST)

**Problem:** the global handler returned `error.message`, `path`, and up to 5
stack frames to any caller.

**Now:** the client receives only `{"error":"internal_error","request_id":"<uuid>"}`.
The full stack/message go to server logs (`console.error("REQUEST_ERROR", …)`).
No filesystem paths, function names, stack frames, secrets, or env vars reach
the client.

**Test:** `protocol.test.ts` `# production-safe 500 error body` (no stack, no
path, no secret). Status: **PROVEN-BY-TEST**.

## 57.6 `/devices` privacy (PROVEN-BY-TEST)

**Problem:** the sensor HTTP control plane (`TCP/8787`, reachable on the LAN)
serialized `Device` structs verbatim, exposing raw `mac`, `ip`, `hostname`.

**Now:** `src/http_server.rs` masks every station before serialization:
- raw MAC is replaced by the stable HMAC pseudonym (`crypto::pseudonymize` with
  the per-sensor secret) — raw MACs never leave the sensor.
- hostname/IP and RSSI/standard/active are retained for control-plane/debugging.
- `/devices/<id>` matches by either the raw identity or the pseudonym.
- The on-router dashboard (same-origin `http://…:8787/`) continues to render the
  pseudonym as the device identity.

**Constraint preserved:** the `EventEnvelope` emitted to the backend is
unchanged (it already carried pseudonyms); only the LAN-facing HTTP response was
hardened.

**Tests (Rust):** `masked_device_replaces_raw_mac_and_is_stable`,
`masked_device_varies_with_secret`, `masked_device_keeps_non_mac_fields`,
`mask_stations_preserves_order_and_len`. Status: **PROVEN-BY-TEST**.

## 57.7 Production vs development credentials (PROVEN-BY-TEST)

**Policy:** production must fail closed rather than silently accept the
well-known development credentials.

- `backend/server.py`: sensor registry comes from `DETECTIC_SENSORS` or
  `sensors.json`. If neither exists it **raises**, unless
  `DETECTIC_ALLOW_DEV_FALLBACK=1` (writes/uses `DEV_SENSORS` with a warning).
  The `--master-secret` arg now fails closed unless
  `DETECTIC_MASTER_SECRET`/flag is set.
- `autonomous/collector.py` and `autonomous/event_reporter.py`: without a
  secret they now **raise** unless `AUTONOMOUS_ALLOW_DEV_SECRET=1`. A
  non-hex `AUTONOMOUS_SECRET` is a hard error (no silent dev fallback).
- Secrets are never logged; the dev warnings print only an indicator.

**Tests (Python):** `backend/tests/test_secret_gating.py`,
`autonomous/tests/test_secret_gating.py` (run via
`python3 -m unittest ...`). Status: **PROVEN-BY-TEST**.

## 57.8 Full regression (2026-08-28)

- `cargo test --release` → **228 passed, 0 failed, 1 ignored** (lib) + **5 passed** (bin).
- `npx tsc -p tsconfig.json --noEmit` → clean.
- `node --experimental-strip-types tests/protocol.test.ts` → all pass.
- `python3 -m unittest backend.tests.test_secret_gating autonomous.tests.test_secret_gating` → all pass.
- Router cross-build (`messense/rust-musl-cross:aarch64-musl cargo build --no-default-features --features wss,tls`) → clean.

## 57.9 Remaining risks

- **WSS live validation pending**: the authentication is proven by unit tests.
  Live confirmation requires redeploying the sensor binary (which now sends
  `DETECTIC_SECRET`) and having `DETECTIC_SENSORS["ex520-001"]` equal that
  secret. Until the on-router binary is rebuilt+redeployed, existing deployed
  sensors will be rejected at the WSS handshake (fail-closed by design).
- **Frontend WebSocket** (`role=frontend`) remains unauthenticated (read-only
  dashboards); it cannot inject events because the `event` path is gated to
  authenticated sensor sockets. If push subscriptions must be protected, add a
  frontend token.
- **HTTP event path** (`HttpEventTransport`) now resolves duplicates correctly,
  but the primary production transport is WSS; the HTTP path is a fallback.

## 57.10 Host deployment credentials

All host-side deployment credentials required to run and deploy Detectic
are stored in the repository root `.env` file. This includes, but is not
limited to:

- `CLOUDFLARE_API_TOKEN` / `CLOUDFLARE_API_TOKEN_WORKER` for `wrangler` deploys.
- `DETECTIC_PASSWORD` for the EX520 GTPR/GDPR API.
- `DETECTIC_D1_SYNC_URL` and the sensor secret for Worker authentication.
- SMTP credentials used by the autonomous collector and event reporter.

The `.env` file is never committed to git. Agents MUST NOT print, copy,
or hardcode any of those values.

To deploy the Worker after `AGENTS.md` safety checks:

```bash
set -a
source .env
set +a
cd backend/cf-worker
npx wrangler deploy
```

For on-router sensor environment variables, see
`deploy/ex520_package/detectic.env`.

