# Detectic — AGENTS.md

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

## Path 4 — Cold-boot watchdog autostart (PROVEN-LIVE)

A host-side Edge Supervisor (`deploy/ex520_package/watchdog.py`) monitors the
router with a state machine (UNKNOWN → ROUTER_DOWN → ROUTER_UP → GTPR_READY →
SENSOR_STARTING → SENSOR_HEALTHY).  It uses IPv6 ping and GTPR queries for
reachability, verifies GTPR readiness, avoids duplicate Phoenix triggers with a
`min_boot_interval` guard, and re-triggers with exponential backoff if the
sensor becomes unhealthy.  After a sustained DOWN (>= DOWN_THRESHOLD) it sends
a `so DEV2_LIFEMOTE_AGENT` with the bootstart URL, re-triggering Path 3 after
a cold boot.

```
Host watchdog.py / Edge Supervisor (poll 10s)
  -> ping6 / GTPR query
  -> state machine: UNKNOWN -> ROUTER_DOWN -> ARMED -> ROUTER_UP -> GTPR_READY
  -> GTPR so DEV2_LIFEMOTE_AGENT (phoenix)
  -> phoenix -> bootstart.sh (SHA-256 verify, atomic reassembly) -> launcher.sh
  -> detectic sensor
  -> health checks via callbacks / sensor_log
```

* DEPLOY: **PROVEN-LIVE**
* PERSIST: **PROVEN-LIVE**
* EXECUTE: **PROVEN-LIVE**
* AUTOSTART: **PROVEN-LIVE** (cold boot: DOWN → UP → trigger → sensor running)
* HEALTH: **PROVEN-FROM-SOURCE** (supervisor state machine and health checks)
* Status: **PROVEN-LIVE** (PHASE 21 second cold-boot proof)

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
cold boot             = PROVEN-LIVE (watchdog -> phoenix -> bootstart -> detectic; done?status=ok&ret=0)
sensor HTTP/8787      = PROVEN-LIVE (curl /health and /devices from host)
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
