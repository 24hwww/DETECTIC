# Detectic — AGENTS.md

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

### VERIFIED LIVE API OPERATION

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
gl/go                 = PROVEN-LIVE
DEV2_WIFI_APDEV_ASSOCDEV = PROVEN-LIVE
IPv4 192.168.0.1      = CURRENTLY UNREACHABLE (GTPR endpoints may respond)
Telnet                = UNKNOWN
SSH                   = UNKNOWN
misc_rw access        = PROVEN-LIVE
misc_rw_bak access    = PROVEN-LIVE
arbitrary execution   = PROVEN-LIVE (via DEV2_LIFEMOTE_AGENT /usr/bin/phoenix.sh)
persistence           = PROVEN-LIVE (split Detectic binary in misc_rw + misc_rw_bak)
manual autostart      = PROVEN-LIVE (phoenix downloads and executes bootstart.sh)
watchdog trigger      = PROVEN-LIVE (cold boot: DOWN -> UP -> GTPR so SENT)
cold boot             = PROVEN-LIVE (watchdog -> phoenix -> bootstart -> detectic; done?status=ok&ret=0)
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

