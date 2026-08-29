# EX520 Safety Protocols — Router Service Preservation Rules

> **CANONICAL DOCUMENT.** These protocols are MANDATORY for all agents,
> operators, and automated systems interacting with the TP-Link EX520V router
> in the Detectic project.  Violating these rules can disrupt DHCP, DNS, WiFi,
> and internet connectivity for all devices on the LAN.

**Version:** 1.0
**Date:** 2026-08-28
**Scope:** All testing, deployment, and operation of Detectic on the EX520V

---

## 1. Guiding Principle

> **The router is a production network device serving real users.**
>
> Detectic is a guest process on the router.  It must NEVER degrade, disrupt,
> or disable the router's primary networking functions:
>
> - DHCP server (IP assignment to LAN devices)
> - DNS relay (name resolution for LAN devices)
> - WiFi access points (2.4GHz and 5GHz)
> - WAN connectivity (internet access)
> - Web management UI (http://192.168.0.1)
> - GTPR management API (IPv6 link-local)

If any Detectic operation risks affecting these services, it MUST be:
1. Classified per the risk matrix (section 3)
2. Authorized by the user (for MEDIUM+ risk)
3. Executed with a rollback plan
4. Verified after execution

---

## 2. Router Service Baseline

The following services MUST remain operational at all times.  Any test or
operation that causes these to fail is a **service incident**.

| # | Service | Verification Command | Expected Result |
|---|---------|---------------------|-----------------|
| 1 | DHCP | `curl -sS http://192.168.0.1:8787/devices` | JSON with ≥1 device |
| 2 | DNS | `dig @192.168.0.1 google.com +short` | IP address (not SERVFAIL) |
| 3 | WAN/Internet | `curl -sS -m 5 -o /dev/null -w "%{http_code}" http://example.com` | `200` |
| 4 | WiFi 2.4GHz | `curl -sS http://192.168.0.1:8787/devices` | ≥1 device with `standard: "n"` |
| 5 | WiFi 5GHz | `curl -sS http://192.168.0.1:8787/devices` | ≥1 device with `standard: "ac"` |
| 6 | Web UI (IPv4) | `curl -sS -o /dev/null -w "%{http_code}" http://192.168.0.1/` | `200` |
| 7 | Web UI (IPv6) | `curl -sS -o /dev/null -w "%{http_code}" "http://[fe80::3e6a:d2ff:fe5f:abc1%enp2s0]/"` | `200` |
| 8 | GTPR API | `detectic query DEV2_LIFEMOTE_AGENT` | `success: true` |
| 9 | Sensor (Detectic) | `curl -sS http://192.168.0.1:8787/health` | `status: healthy` |

### Post-reboot service recovery timeline

After a router reboot, services come back in this order.  Do NOT declare
the router "fully operational" until ALL services are verified.

```
  0s:   reboot command sent
 30s:   router responds to ping (kernel up, cos starting)
 60s:   DHCP server available (dnsmasq started)
 90s:   WiFi APs broadcasting (hostapd started)
120s:   DNS Relay forwarding (dnsmasq forwarding rules populated)
180s:   all services stable — safe to run health checks
```

**Minimum wait before health checks: 120s**
**Minimum wait before declaring "fully operational": 180s**

---

## 3. Risk Classification Matrix

Every operation on the router MUST be classified before execution.

### GREEN — Safe (no authorization needed)

Operations that are read-only or affect only the Detectic sensor process,
with no impact on router networking services.

| Operation | Example | Why it's safe |
|-----------|---------|---------------|
| Read GTPR query | `detectic query DEV2_*` | Read-only, no state change |
| Read sensor health | `curl http://192.168.0.1:8787/health` | Read-only |
| Read sensor devices | `curl http://192.168.0.1:8787/devices` | Read-only |
| Kill Detectic sensor | `kill_sensor.sh` or `kill -9 <pid>` | Only affects sensor process |
| Restart Detectic sensor | `launcher.sh restart` | Only affects sensor process |
| On-router watchdog restart | `detectic_watchdog.sh` | Only monitors/restarts sensor |
| Download files from package server | `wget http://192.168.0.27:8080/*` | No router impact |
| Build/compile Detectic | `make`, `cargo build` | Host-only, no router impact |
| Run host-side watchdog | `python3 watchdog.py` | Host-only, monitors router |
| Git operations | `git commit`, `git status` | No router impact |

### YELLOW — Caution (user notification recommended)

Operations that temporarily affect the sensor or use Phoenix, but do NOT
reboot the router or modify router configuration.

| Operation | Example | Risk | Mitigation |
|-----------|---------|------|------------|
| Phoenix toggle (enable:0→1) | `so DEV2_LIFEMOTE_AGENT {enable:0}` then `{enable:1}` | Sensor downtime 30-60s during re-download | Verify sensor recovers |
| bootstart.sh re-deploy | Toggle with bootstart URL | Sensor downtime 30-60s | Verify `done?status=ok` |
| Firewall rule add (8787) | `iptables -I INPUT ...` | Minimal — adds ACCEPT rule | Idempotent, safe |
| Modify launcher.sh | Edit + redeploy | Sensor restarts | Verify health after |
| Modify watchdog.sh | Edit + redeploy | Watchdog restarts | Verify watchdog PID |
| Modify bootstart.sh | Edit + redeploy | Full re-download cycle | Verify `done?status=ok` |
| Kill on-router watchdog | `kill <watchdog_pid>` | No crash recovery until restarted | Restart watchdog after |

### ORANGE — Risky (user authorization REQUIRED)

Operations that reboot the router or modify router networking configuration.

| Operation | Example | Impact | Required Authorization |
|-----------|---------|--------|----------------------|
| Router reboot | `op ACT_REBOOT` | ALL services down 1-3 min | User must confirm: "This will reboot the router and disconnect all devices for 1-3 minutes. Proceed?" |
| Modify DNS Relay | `set DEV2_DNS_RELAY` | DNS resolution may break | User must confirm |
| Modify DHCP server | `set DEV2_DHCPV4_SERVER` | New DHCP leases may fail | User must confirm |
| Modify WiFi settings | `set DEV2_WIFI_*` | WiFi clients disconnect | User must confirm |
| Modify WAN settings | `set DEV2_WANSETTINGS` | Internet may break | User must confirm |
| Modify LAN IP | `set DEV2_ADT_LAN` | All devices lose connectivity | User must confirm |

### RED — Forbidden (NEVER execute without firmware engineering review)

Operations that can permanently damage the router or require factory reset.

| Operation | Why it's forbidden |
|-----------|-------------------|
| Firmware flash / mtd write | Can brick the router |
| Modify bootloader (U-Boot) | Can brick the router |
| Overwrite misc_rw partition | Destroys all router configuration |
| Delete stock system files | Rootfs is read-only; attempting writes can corrupt |
| Modify cos binary or libraries | Can crash the main router daemon |
| Disable stock services (httpd, cos) | Loses all management access |
| Change admin password via GTPR | May lock out management |
| Factory reset | Destroys all configuration |
| Format any partition | Data loss |

---

## 4. Router Reboot Protocol

### 4.1 When reboots are allowed

Router reboots are ONLY allowed in these scenarios:

1. **Cold-boot autostart testing** — with explicit user authorization, max
   once per 10 minutes, and only after verifying all services are currently
   healthy.
2. **Firmware update** — not applicable (stock firmware, no updates planned).
3. **Recovery from a frozen state** — only if the router is already
   unresponsive and a reboot is the only recovery option.

### 4.2 Reboot authorization checklist

Before sending `ACT_REBOOT` or any reboot command:

```
[ ] User has explicitly authorized this reboot
[ ] All services are currently healthy (section 2 baseline)
[ ] At least 10 minutes since the last reboot
[ ] No other tests are running on the router
[ ] User has been warned: "All devices will lose connectivity for 1-3 minutes"
[ ] A recovery plan exists (host watchdog running, or manual toggle ready)
```

### 4.3 Post-reboot verification checklist

After a reboot, wait 180s then verify:

```
[ ] Router responds to ping (192.168.0.1)
[ ] Web UI responds (http://192.168.0.1/ → 200)
[ ] GTPR API responds (detectic query DEV2_LIFEMOTE_AGENT → success)
[ ] DNS resolves (dig @192.168.0.1 google.com +short → IP)
[ ] Internet works (curl http://example.com → 200)
[ ] DHCP serving (curl http://192.168.0.1:8787/devices → ≥1 device)
[ ] WiFi clients connected (≥1 active device in /devices)
[ ] Sensor healthy (curl http://192.168.0.1:8787/health → healthy)
```

If ANY check fails after 5 minutes, escalate to the user.

### 4.4 Reboot frequency limits

| Scenario | Max frequency | Reason |
|----------|--------------|--------|
| Cold-boot testing | 1 per 10 minutes | WiFi clients enter backoff if rapid reboots |
| Production watchdog | Never (detect only) | Watchdog detects reboots, doesn't cause them |
| Recovery testing | 1 per 30 minutes | Allow full service stabilization |

---

## 5. Detectic Sensor Safety Rules

### 5.1 Sensor process operations

| Operation | Allowed? | Conditions |
|-----------|----------|------------|
| Start sensor (`launcher.sh start`) | ✅ Always | Idempotent, safe |
| Stop sensor (`launcher.sh stop`) | ✅ Always | Only affects sensor |
| Restart sensor (`launcher.sh restart`) | ✅ Always | Only affects sensor |
| Kill sensor (`kill -9 <pid>`) | ✅ Always | Only affects sensor |
| Kill ALL detectic processes | ⚠️ Caution | Must spare the watchdog process |
| Kill on-router watchdog | ⚠️ Caution | No crash recovery until restarted |

### 5.2 Sensor deployment safety

When deploying a new version of the Detectic sensor:

1. **Verify the binary on the host first:**
   ```bash
   ./target/aarch64-unknown-linux-musl/release/detectic version
   ```

2. **Verify SHA-256 checksums match the manifest:**
   ```bash
   sha256sum detectic.aa detectic.ab detectic.ac
   ```

3. **Never deploy a binary that fails `version` subcommand** — it may be
   corrupted or incompatible.

4. **Keep the previous version for rollback:**
   The `launcher.sh` script and `bootstart.sh` handle this by re-downloading
   from the package server.  Ensure the package server always has a known-good
   version before deploying a new one.

5. **After deployment, verify:**
   ```
   [ ] Sensor health: curl http://192.168.0.1:8787/health → healthy
   [ ] Sensor version: curl http://192.168.0.1:8787/health → correct version
   [ ] IPv4 reachable: curl -o /dev/null -w "%{http_code}" http://192.168.0.1:8787/health → 200
   [ ] IPv6 reachable: curl -o /dev/null -w "%{http_code}" http://[fe80::...]:8787/health → 200
   [ ] On-router watchdog running: check watchdog.pid
   ```

### 5.3 Sensor resource limits

The Detectic sensor MUST NOT consume excessive router resources:

| Resource | Limit | Verification |
|----------|-------|-------------|
| RAM | < 30 MB | `ps aux \| grep detectic` (RSS column) |
| CPU | < 5% average | `top -b -n 1 \| grep detectic` |
| Disk (misc_rw) | < 5 MB | `du -sh /var/run/misc/misc_rw/detectic/` |
| Disk (/var/tmp) | < 15 MB | `du -sh /var/tmp/detectic/` |
| Network bandwidth | < 1 MB/s | Sensor polls every 5s, events are small |
| File descriptors | < 50 | `ls /proc/<pid>/fd \| wc -l` |

If the sensor exceeds these limits, investigate and fix before re-deploying.

---

## 6. Phoenix / GTPR Safety Rules

### 6.1 Phoenix trigger safety

The Phoenix mechanism (`so DEV2_LIFEMOTE_AGENT`) downloads and executes
arbitrary scripts as root on the router.  This is powerful and must be
controlled.

| Rule | Details |
|------|---------|
| Only serve from trusted URLs | The bootstart URL must point to `http://192.168.0.27:8080/bootstart.sh` or a controlled host |
| Never serve from the internet | Phoenix scripts must come from the local host package server, never from external URLs |
| Verify SHA-256 of all downloads | `bootstart.sh` verifies every part; never bypass this |
| Never execute unverified binaries | `bootstart.sh` reassembles and verifies; never run a binary that hasn't been checksum-verified |
| Toggle, don't spam | Use `enable:0 → enable:1` toggle, not repeated `enable:1` |
| Wait between triggers | Minimum 60s between Phoenix triggers to avoid overlapping downloads |

### 6.2 GTPR write operation safety

GTPR `set` operations modify the router's persistent configuration.  Follow
these rules:

| OID | Write allowed? | Conditions |
|-----|---------------|------------|
| `DEV2_LIFEMOTE_AGENT` | ✅ Yes | Only enable/URL/stack fields; toggle mechanism |
| `DEV2_DNS_RELAY` | ⚠️ Caution | Only if DNS is broken and user authorizes |
| `DEV2_DHCPV4_SERVER` | ⚠️ Caution | Only if DHCP is broken and user authorizes |
| `DEV2_WIFI_*` | 🔴 Risky | Only with user authorization |
| `DEV2_WANSETTINGS` | 🔴 Risky | Only with user authorization |
| `DEV2_ADT_LAN` | 🔴 Risky | Only with user authorization |
| `DEV2_REBOOT` / `ACT_REBOOT` | 🔴 Risky | See section 4 (reboot protocol) |
| `DEV2_TIME` | ⚠️ Caution | Only NTP server fields, not time zone (affects logs) |
| `DEV2_USER_CFG` | 🔴 Forbidden | Can lock out management |
| Any OID not listed here | 🔴 Forbidden | Research before writing |

### 6.3 GTPR error codes

| Code | Meaning | Action |
|------|---------|--------|
| `errorcode: 0` | Success | Proceed |
| `errorcode: 9003` | Permission denied (user level) | Use `user` account, not `admin`; some OIDs are read-only for `user` |
| `errorcode: 9804` | Invalid OID or instance | Check OID name and instance index |
| Other codes | Unknown error | Investigate before retrying |

---

## 7. Firewall Safety Rules

### 7.1 Detectic port rules

The Detectic sensor listens on TCP/8787.  The `launcher.sh` script opens
this port in the router's firewall.  Follow these rules:

| Rule | Details |
|------|---------|
| Only open TCP/8787 | Never open other ports |
| Only on br0 (LAN) | Never open ports on the WAN interface |
| Idempotent rules | Use `-C` check before `-I` insert to avoid duplicate rules |
| IPv4 and IPv6 | Always add both `iptables` and `ip6tables` rules |
| Never flush rules | Never run `iptables -F` or `ip6tables -F` — this removes all router firewall rules |
| Never delete stock rules | Only add Detectic rules; never remove or modify existing rules |

### 7.2 Firewall verification

After `launcher.sh` opens the firewall:

```bash
# IPv4
/usr/sbin/iptables -L INPUT -n --line-numbers | grep 8787
# IPv6
/usr/sbin/ip6tables -L INPUT -n --line-numbers | grep 8787
# Verify from host
curl -sS -o /dev/null -w "%{http_code}" http://192.168.0.1:8787/health  # → 200
curl -sS -o /dev/null -w "%{http_code}" "http://[fe80::...]:8787/health"  # → 200
```

---

## 8. Watchdog Safety Rules

### 8.1 Host-side watchdog (`watchdog.py`)

| Rule | Details |
|------|---------|
| Detect, don't cause | The watchdog must DETECT router reboots, never CAUSE them |
| Toggle, don't spam | Use `enable:0 → enable:1` toggle, with `min_boot_interval` guard |
| Backoff on failure | Exponential backoff if trigger fails or sensor stays unhealthy |
| Single instance | PID file lock prevents duplicate watchdogs |
| Log everything | All state transitions logged with timestamps |
| Grace period | Wait `phoenix_grace` seconds after trigger before checking health |
| Health timeout | Don't re-trigger more than once per `health_timeout` period |

### 8.2 On-router watchdog (`detectic_watchdog.sh`)

| Rule | Details |
|------|---------|
| Only kill detectic processes | Never kill cos, httpd, dnsmasq, or any stock process |
| Spare itself | The watchdog must not kill its own PID |
| Bounded restarts | Max 10 restarts before giving up (prevents infinite loops) |
| Exponential backoff | 30s → 60s → 120s → 240s → 300s between restart attempts |
| Health check, not just process | Check both process existence AND health endpoint |
| Single instance | PID file lock prevents duplicate watchdogs |
| Bounded log | Keep last 50 KiB of log to avoid filling misc_rw |

### 8.3 Watchdog interaction safety

The two watchdogs (host + on-router) must not conflict:

```
Host watchdog (watchdog.py):
  - Monitors router reachability (ping + GTPR)
  - Triggers Phoenix on cold boot (toggle)
  - Monitors sensor health (TCP probe to 8787)
  - Does NOT restart the sensor directly

On-router watchdog (detectic_watchdog.sh):
  - Monitors sensor process + health endpoint
  - Restarts sensor via launcher.sh on crash
  - Does NOT trigger Phoenix
  - Does NOT reboot the router

Interaction:
  - Host triggers Phoenix → bootstart → sensor + on-router watchdog start
  - On-router watchdog handles crash recovery autonomously
  - Host watchdog only re-triggers if sensor stays down after health_timeout
  - Both watchdogs have backoff to prevent trigger storms
```

---

## 9. Testing Safety Protocol

### 9.1 Pre-test checklist

Before ANY test that touches the router:

```
[ ] Test is classified (GREEN / YELLOW / ORANGE / RED)
[ ] If YELLOW+: user has been notified
[ ] If ORANGE: user has explicitly authorized
[ ] If RED: operation is forbidden — do not proceed
[ ] Router services are currently healthy (section 2 baseline)
[ ] No other tests are running on the router
[ ] Rollback plan is ready (known-good state documented)
[ ] Test duration is estimated and reasonable
```

### 9.2 During test monitoring

During any test that affects the router:

```
[ ] Monitor sensor health: curl http://192.168.0.1:8787/health
[ ] Monitor DNS: dig @192.168.0.1 google.com +short
[ ] Monitor DHCP: curl http://192.168.0.1:8787/devices
[ ] If any service fails unexpectedly: STOP the test and investigate
[ ] Log all operations with timestamps for forensic analysis
```

### 9.3 Post-test verification

After any test that affects the router:

```
[ ] All services in section 2 baseline are verified
[ ] Sensor is healthy (curl /health → healthy)
[ ] On-router watchdog is running (if applicable)
[ ] DEV2_LIFEMOTE_AGENT URL is set to bootstart.sh (not a test script)
[ ] No test artifacts left on the router (kill_sensor.sh, etc.)
[ ] Router uptime is reasonable (not in a reboot loop)
[ ] WiFi clients are connected
[ ] DNS resolves correctly
[ ] Internet is accessible
```

### 9.4 Test artifact cleanup

After testing, ensure the router is left in a clean state:

1. **Set DEV2_LIFEMOTE_AGENT URL back to bootstart.sh** (not kill_sensor.sh
   or any test script):
   ```bash
   detectic set DEV2_LIFEMOTE_AGENT '{"enable":"1","URL":"http://192.168.0.27:8080/bootstart.sh","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'
   ```

2. **Remove test scripts from the package server** (kill_sensor.sh, etc.)
   if they are no longer needed.

3. **Verify no stale processes** are running on the router from test scripts.

4. **Verify the on-router watchdog** is running and monitoring the sensor.

---

## 10. Incident Response

### 10.1 "No internet" reported by user

```
Step 1: Check DNS (most common cause after reboot)
  dig @192.168.0.1 google.com +short
  → If SERVFAIL: wait 60s, retry. DNS Relay may still be initializing.
  → If still failing after 5 min: try set DEV2_DNS_RELAY '{"enable":"1"}'

Step 2: Check WAN
  curl -sS -m 5 -o /dev/null -w "%{http_code}" http://example.com
  → If fails: check if router can ping 8.8.8.8 (WAN may be down)

Step 3: Check DHCP
  curl -sS http://192.168.0.1:8787/devices
  → If no devices: DHCP server may be down

Step 4: Check WiFi
  curl -sS http://192.168.0.1:8787/devices
  → If no WiFi devices: APs may be down, check router uptime

Step 5: If router was recently rebooted (< 5 min ago)
  → This is likely normal service startup delay. Wait 180s and recheck.
```

### 10.2 "No DHCP" reported by user

```
Step 1: Check if DHCP server is enabled
  detectic query DEV2_DHCPV4_SERVER
  → enable should be "1"

Step 2: Check if any devices have IPs
  curl -sS http://192.168.0.1:8787/devices
  → If devices have IPs: DHCP is working, issue is device-specific

Step 3: Check router uptime
  curl -sS http://192.168.0.1:8787/health | grep uptime
  → If uptime < 120s: DHCP server may still be starting. Wait.

Step 4: If DHCP server is disabled, re-enable (with user authorization)
  detectic set DEV2_DHCPV4_SERVER '{"enable":"1","stack":"0,0,0,0,0,0"}'
```

### 10.3 Sensor unreachable

```
Step 1: Check if router is up
  ping -c 2 192.168.0.1

Step 2: Check if sensor port is open
  curl -sS -m 5 http://192.168.0.1:8787/health
  → If connection refused: sensor is down, watchdog should restart it

Step 3: Check if on-router watchdog is running
  (via package server log: env_line?n=96&d=watchdog_*)

Step 4: If watchdog is not running, trigger Phoenix
  detectic set DEV2_LIFEMOTE_AGENT '{"enable":"0","URL":"","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'
  sleep 2
  detectic set DEV2_LIFEMOTE_AGENT '{"enable":"1","URL":"http://192.168.0.27:8080/bootstart.sh","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'

Step 5: Wait 60s and verify
  curl -sS http://192.168.0.1:8787/health
```

### 10.4 Trigger storm (repeated Phoenix triggers)

If Phoenix is being triggered repeatedly (seen in package server log as
multiple `GET /bootstart.sh` in quick succession):

```
Step 1: Stop the host-side watchdog
  pkill -f "python3.*watchdog.py"

Step 2: Wait for any in-progress bootstart to complete
  (check package server log for done?status=ok)

Step 3: Verify sensor is healthy
  curl -sS http://192.168.0.1:8787/health

Step 4: Investigate why the watchdog was trigger-storming
  - Check watchdog.py log for state transitions
  - Check if health_timeout is too short
  - Check if min_boot_interval is too short

Step 5: Fix the watchdog configuration before restarting it
```

---

## 11. Production Deployment Safety

### 11.1 Before production deployment

```
[ ] All tests in section 9 have passed
[ ] Sensor resource usage is within limits (section 5.3)
[ ] On-router watchdog is tested and working
[ ] Host-side watchdog is tested and working
[ ] Package server is stable and has the correct version
[ ] DNS, DHCP, WiFi, WAN all verified healthy
[ ] Rollback procedure is documented and tested
[ ] User has authorized the deployment
```

### 11.2 Production watchdog configuration

The host-side watchdog in production should have these settings:

| Parameter | Production Value | Reason |
|-----------|-----------------|--------|
| `poll_interval` | 10s | Fast enough to detect reboots |
| `down_threshold` | 30s | Avoid false positives on brief network blips |
| `min_boot_interval` | 300s | Don't re-trigger within 5 minutes |
| `health_timeout` | 180s | Allow 3 minutes for sensor to come up |
| `phoenix_grace` | 10s | Brief pause before checking post-trigger |
| `backoff_level` | 0-4 | Exponential backoff on repeated failures |

### 11.3 Production monitoring

In production, monitor these signals:

| Signal | Healthy | Action if unhealthy |
|--------|---------|-------------------|
| `curl http://192.168.0.1:8787/health` | `status: healthy` | Check watchdog, trigger Phoenix if needed |
| Package server `done?status=ok` | Recent successful bootstart | Investigate if `status=fail` |
| Package server `heartbeat?t=launcher` | Regular (every 30s) | Sensor may be down if missing |
| Router uptime (from sensor) | > 600s (not rebooting) | Investigate if frequently low |
| DNS `dig @192.168.0.1` | Returns IP | Re-enable DNS Relay if SERVFAIL |
| WiFi device count | ≥ 1 active | Check WiFi APs if 0 |

---

## 12. Forbidden Actions Summary

These actions are **NEVER** allowed without explicit firmware engineering
review and user authorization:

1. ❌ Flashing firmware or writing to flash partitions
2. ❌ Modifying the bootloader (U-Boot)
3. ❌ Overwriting or formatting misc_rw
4. ❌ Modifying stock binaries (cos, httpd, dnsmasq, busybox)
5. ❌ Disabling stock services
6. ❌ Changing the admin password
7. ❌ Factory reset
8. ❌ `iptables -F` or `ip6tables -F` (flush all rules)
9. ❌ Rebooting the router without authorization
10. ❌ Serving Phoenix scripts from external (non-local) URLs
11. ❌ Deploying unverified binaries (no SHA-256 check)
12. ❌ Running Detectic with resource usage exceeding limits (section 5.3)
13. ❌ Triggering Phoenix more than once per 60s
14. ❌ Rebooting the router more than once per 10 minutes
15. ❌ Leaving test scripts as the DEV2_LIFEMOTE_AGENT URL after testing

---

## 13. Document Maintenance

This document MUST be updated when:
- A new operation type is discovered that isn't covered here
- An incident occurs that reveals a gap in the protocols
- The router configuration changes (new services, new OIDs)
- The Detectic architecture changes (new components, new watchdogs)

All updates must be reviewed and approved by the project owner.
