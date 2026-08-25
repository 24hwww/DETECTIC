# PHASE14.1_MIMO_EXECUTION_PATH_AUDIT.md

## 1. Executive Summary

The EX520 stock firmware contains **one proven execution primitive** (Lifemote Agent → `phoenix.sh` → remote script download → shell) and **one critical unknown** (whether `cos` re-applies Lifemote config at boot).

**Proven:**
- Shell access via GTPR → Telnet → first-login → Lifemote Agent (PROVEN-LIVE)
- Detectic binary executes on EX520 (PROVEN-LIVE)
- misc_rw is writable, persistent, executable (PROVEN-OFFLINE)
- `phoenix.sh` is a supervisor that keeps downloaded scripts alive (PROVEN-FROM-SOURCE)

**Unknown:**
- Whether `cos` re-applies `DEV2_LIFEMOTE_AGENT` config at boot → determines if autostart is possible
- Whether misc_rw has sufficient space (1144 KB total reported, binary is 1.2 MB) → BLOCKS deployment

**Critical finding:** The misc_rw partition is **only 1144 KB total** (from M10 validation). The Detectic binary is **1.2 MB**. This means the binary **cannot fit** in misc_rw. This is a hard blocker for the current architecture.

---

## 2. Known Execution Primitives

### 2.1 Lifemote Agent → phoenix.sh (PROVEN-LIVE)

**Chain:**
```
GTPR so DEV2_LIFEMOTE_AGENT {enable:1, URL:"http://..."}
    ↓
libcmm.so → rsl_setDev2LifemoteAgentObj
    ↓
"/usr/bin/phoenix.sh %s &"
    ↓
phoenix.sh downloads script from URL to /tmp/lifemote_cpe_daemon.sh
    ↓
sh /tmp/lifemote_cpe_daemon.sh &
    ↓
phoenix.sh supervisor loop (checks every 30 min, re-downloads if dead)
```

**Classification:** PROVEN-LIVE (tested on real EX520)

**Limitations:**
- Downloads from REMOTE URL, not local file
- Requires HTTP server accessible from router
- Script lands in `/tmp` (volatile, lost on reboot)
- Supervisor keeps script alive within boot session
- Does NOT auto-start at boot (config-triggered only)

### 2.2 Telnet CLI → doFshell (PROVEN-LIVE)

**Chain:**
```
GTPR so DEV2_TELNET_CFG {telnetLocalEnabled:1}
    ↓
libcmm.so → rsl_setDev2TelnetCfgObj → oal_telnetRestart
    ↓
telnetd -p 23 &
    ↓
cli binary (login program)
    ↓
doFshell → /bin/sh (after authentication)
```

**Classification:** PROVEN-LIVE (tested on real EX520)

**Limitations:**
- Requires admin password (obtainable via pwdSign=0 bypass)
- Temporary — must be re-enabled after reboot
- Not a persistence mechanism

### 2.3 pwdSign=0 First-Login Bypass (PROVEN-LIVE)

**Chain:**
```
GTPR so DEV2_USER_CFG {pwdSign:"0"}
    ↓
Admin password reset state
    ↓
Telnet CLI → first-login prompt → set new password
    ↓
New admin password known
```

**Classification:** PROVEN-LIVE

**Properties:**
- `pwdSign=0` persists across reboot (stored in misc_rw data model)
- Admin password change persists across reboot
- This is the key to obtaining shell access after reboot

---

## 3. COS Analysis

### What is COS?

`cos` is the TP-Link data-model manager daemon. It is started by `rcS` at boot.

### Evidence (PROVEN-FROM-SOURCE):

- Location: `/bin/cos` (SquashFS, read-only)
- Started by: `rcS` (`cos &`)
- References: `/var/tmp/` paths for data files (`pslist`, `dconf`, `dnsmasq.conf`)
- Does NOT execute scripts from writable paths
- Does NOT source files from misc_rw
- No `system()`/`popen()`/`exec()` strings found in strings analysis
- Binary is stripped, no symbols

### What COS does with data model:

- `dm_saveCfg` → writes data model to `misc_rw/0x00300000`
- `dm_restoreCfg` → applies XML config to in-memory data model
- Apply handlers: per-subsystem callbacks triggered by config changes

### Critical unknown:

**Does `cos` re-apply data-model configuration at boot?**

If `cos` reads the persisted data model from `misc_rw/0x00300000` at boot and re-triggers apply handlers, then:
- `DEV2_LIFEMOTE_AGENT.enable=1` (persisted) → `phoenix.sh` auto-starts
- `DEV2_TELNET_CFG.telnetLocalEnabled=1` (persisted) → `telnetd` auto-starts

If `cos` does NOT re-apply at boot, then:
- Configuration changes are lost until manually re-applied via GTPR
- No autostart mechanism exists

### Classification:

| Property | Status |
|----------|--------|
| cos exists | PROVEN-FROM-SOURCE |
| cos started at boot | PROVEN-FROM-SOURCE |
| cos manages data model | PROVEN-FROM-SOURCE |
| cos references /var/tmp | PROVEN-FROM-SOURCE |
| cos executes scripts from writable | DISPROVEN |
| cos re-applies config at boot | **UNKNOWN** |
| cos launches arbitrary executables | **UNKNOWN** |

---

## 4. Telnet/Dropbear Analysis

### Telnet (PROVEN-LIVE)

- Binary: BusyBox `telnetd` (in SquashFS)
- Config object: `DEV2_TELNET_CFG`
- Apply handler: `rsl_setDev2TelnetCfgObj` → `oal_telnetRestart` → `telnetd -p %d &`
- Gated by: `INCLUDE_WEB_TELNET=1`, `INCLUDE_REMOTE_TELNET=1`
- Login program: `cli` binary (not `/bin/sh`)
- `cli` has `doFshell` (shell escape after auth)

**What enabling Telnet does:** Starts `telnetd` with `cli` as login program. This is a FIXED binary launch, not arbitrary execution.

**What Telnet does NOT do:** Accept arbitrary command paths, execute scripts from writable storage, provide autostart.

### Dropbear/SSH (UNKNOWN)

- Binary: `/usr/bin/dropbear` → `dropbearmulti` (in SquashFS)
- Config object: `DEV2_SSH_CFG`
- `INCLUDE_SSH_ACCESS=0` → SSH UI disabled
- May be running on port 22 (ISP-enabled)
- Cannot be configured via GTPR `so` (error 9003)

**Classification:** UNKNOWN — SSH exists but is not configurable via web UI.

---

## 5. Backupcfg Analysis

### What backupcfg can do (PROVEN-OFFLINE):

1. Contain persistent configuration (XML in DES-ECB + zlib)
2. Be imported via web UI restore
3. Survive reboot (config applied to data model)
4. Influence startup behavior (enable/disable services)

### What backupcfg CANNOT do (PROVEN-OFFLINE):

1. Contain arbitrary files (config-only format)
2. Execute arbitrary commands
3. Launch external executables
4. Install binaries

### Classification:

| Property | Status |
|----------|--------|
| Contains configuration | PROVEN-OFFLINE |
| Contains arbitrary files | DISPROVEN |
| Influences startup | PROVEN-OFFLINE (enables fixed services) |
| Arbitrary executable paths | DISPROVEN |
| Importable without flash | PROVEN-OFFLINE |
| Survives reboot | PROVEN-OFFLINE |

---

## 6. misc_rw Analysis

### Properties:

| Property | Status | Evidence |
|----------|--------|----------|
| Exists | PROVEN-OFFLINE | rcS mounts `ubi2:misc_rw` |
| Writable | PROVEN-OFFLINE | UBI ubifs, rw mount |
| Executable | PROVEN-OFFLINE | UBIFS supports +x, no noexec mount |
| Persistent | PROVEN-OFFLINE | UBI volume survives reboot |
| Boot-accessible | PROVEN-OFFLINE | Mounted by rcS before cos starts |
| Total capacity | **1144 KB** | M10 validation (live measurement) |
| Free space | UNKNOWN | Depends on data model size |

### Critical finding:

**misc_rw is only 1144 KB total.** The Detectic binary is 1,321,216 bytes (1.26 MB). The binary **cannot fit** in misc_rw.

This was confirmed in M10 validation:
> "Update was tested but failed due to insufficient disk space on the misc_rw partition (1144 KB total, binary is 1.2 MB)."

### What CAN fit in misc_rw:

- Data model binary (`0x00300000`) — already there
- Small config files (<100 KB)
- State files (<10 KB)
- Launcher script (<5 KB)

### What CANNOT fit in misc_rw:

- Detectic binary (1.26 MB)
- Any binary > ~1 MB

### Alternative locations:

| Location | Writable | Persistent | Executable | Capacity |
|----------|----------|------------|------------|----------|
| misc_rw | Yes | Yes | Yes | **1144 KB (too small)** |
| /tmp | Yes | No | Yes | RAM (volatile) |
| /var/tmp | Yes | No | Yes | RAM (volatile) |
| runtime_data | Not enabled | N/A | N/A | N/A |

---

## 7. Diagnostic Execution Analysis

### DEV2_DIAG_IPPING (PROVEN-FROM-SOURCE)

- Backend: `tr143d` daemon
- Executes: fixed diagnostic binary (ping)
- NOT shell execution
- NOT configurable executable path
- TR-143 standard diagnostic mechanism

### Classification:

| Property | Status |
|----------|--------|
| Fixed binary launch | PROVEN-OFFLINE |
| Shell command | DISPROVEN |
| Configurable executable | DISPROVEN |

---

## 8. Web UI / OID Execution Analysis

### OIDs that launch processes:

| OID | Process | Fixed? | From writable? |
|-----|---------|--------|-----------------|
| DEV2_TELNET_CFG | `telnetd -p %d` | Yes | No |
| DEV2_SSH_CFG | `dropbear` | Yes | No |
| DEV2_LIFEMOTE_AGENT | `phoenix.sh` | Yes | No (downloads from URL) |

### OIDs that do NOT launch processes:

- DEV2_WIFI_APDEV_ASSOCDEV (read-only data)
- DEV2_HOST_ENTRY (read-only data)
- DEV2_DHCPV4_CLIENT (read-only data)
- DEV2_DEV_INFO (read-only data)
- DEV2_USER_CFG (config only)

### Classification:

No OID accepts arbitrary executable paths. All process launches are fixed vendor binaries.

---

## 9. Watchdog / Service Supervisor Analysis

### COS as supervisor:

- Starts daemons at boot (rcS)
- Does NOT restart crashed daemons (no evidence)
- Does NOT scan directories for executables
- Does NOT support custom service definitions
- Does NOT read process paths from writable config

### Kernel watchdog:

- `MULTICORE_WATCHDOG` optional (not confirmed enabled)
- Restarts entire system on hang
- Does NOT launch user processes

### Classification:

No watchdog/supervisor can launch Detectic from writable storage.

---

## 10. Candidate Persistence Paths

### Path A: Lifemote Agent + phoenix.sh supervisor

```
GTPR so DEV2_LIFEMOTE_AGENT {enable:1, URL:"http://host/detectic.sh"}
    ↓
IF cos re-applies config at boot:
    ↓
phoenix.sh auto-starts
    ↓
Downloads detectic.sh from URL
    ↓
detectic.sh starts Detectic
    ↓
phoenix.sh supervisor keeps it alive
```

**Prerequisites:**
1. `cos` must re-apply `DEV2_LIFEMOTE_AGENT` config at boot → UNKNOWN
2. HTTP server must be accessible from router → requires always-on server
3. Script must be in `/tmp` (volatile) → supervisor re-downloads if dead

**Classification:** PLAUSIBLE (depends on cos re-apply behavior)

### Path B: Telnet + manual re-enable

```
After reboot:
    ↓
GTPR so DEV2_TELNET_CFG {telnetLocalEnabled:1}
    ↓
Telnet available
    ↓
Login → start Detectic manually
```

**Classification:** PROVEN-LIVE but NOT autostart (manual intervention required)

### Path C: External launcher on separate device

```
Always-on device on LAN
    ↓
Polls router via GTPR
    ↓
Detects Detectic not running
    ↓
Enables Telnet → starts Detectic
```

**Classification:** PROVEN-OFFLINE (design only, not implemented)

---

## 11. Candidate Autostart Paths

### The only candidate: Lifemote Agent re-apply at boot

**If `cos` re-applies config at boot:**

```
Boot
  ↓
rcS → cos &
  ↓
cos reads misc_rw/0x00300000 (data model)
  ↓
cos re-applies DEV2_LIFEMOTE_AGENT config
  ↓
rsl_setDev2LifemoteAgentObj → phoenix.sh URL &
  ↓
phoenix.sh downloads script from URL
  ↓
Script starts Detectic
  ↓
phoenix.sh supervisor maintains Detectic
```

**If `cos` does NOT re-apply config at boot:**

No autostart mechanism exists.

### Classification:

| Path | Status |
|------|--------|
| Lifemote re-apply at boot | PLAUSIBLE (unknown cos behavior) |
| rcS hook | DISPROVEN (RO) |
| hotplug | DISPROVEN (RO) |
| cron | DISPROVEN (not started) |
| init.d service | DISPROVEN (RO) |
| procd | NOT PRESENT |

---

## 12. Evidence Classification

| Mechanism | Classification | Risk | Recovery |
|-----------|---------------|------|----------|
| Lifemote Agent execution | PROVEN-LIVE | Medium | Disable via GTPR |
| Telnet enablement | PROVEN-LIVE | Low | Disable via GTPR |
| pwdSign bypass | PROVEN-LIVE | High | Irreversible |
| misc_rw writable | PROVEN-OFFLINE | Low | N/A |
| misc_rw persistent | PROVEN-OFFLINE | Low | N/A |
| misc_rw 1144 KB | PROVEN-LIVE | **HIGH** | Binary won't fit |
| cos re-apply at boot | UNKNOWN | N/A | N/A |
| phoenix.sh supervisor | PROVEN-FROM-SOURCE | Low | Kill process |
| Telnet CLI doFshell | PROVEN-FROM-SOURCE | Medium | Disable Telnet |
| backupcfg arbitrary exec | DISPROVEN | N/A | N/A |
| rcS writable hook | DISPROVEN | N/A | N/A |
| hotplug writable hook | DISPROVEN | N/A | N/A |
| cron autostart | DISPROVEN | N/A | N/A |
| Arbitrary OID exec | DISPROVEN | N/A | N/A |

---

## 13. Contradictions / Unknowns

### Contradiction 1: misc_rw capacity

- Design target: ≥12 MB (Phase 12A)
- Actual measurement: **1144 KB** (M10 live validation)
- Binary size: **1,321,216 bytes (1.26 MB)**
- **The binary CANNOT fit in misc_rw.**

This contradicts the entire deployment architecture. The binary must go somewhere else.

### Unknown 1: cos re-apply at boot

Critical for autostart. Cannot be determined without:
- Live test: enable Lifemote, reboot, check if phoenix.sh auto-starts
- Or: disassemble cos binary (stripped, no symbols)

### Unknown 2: Alternative persistent writable location

If misc_rw is too small, where can the binary go?
- `/tmp` — volatile (lost on reboot)
- `/var/tmp` — volatile
- `runtime_data` — not enabled
- No other persistent writable location found

### Unknown 3: Can the binary be stored on a different partition?

- `misc_rw_bak` — might be available (DUAL_CONFIG flag set)
- `misc_ro` — read-only
- `misc_isp` — read-only
- Flash partitions — not writable without firmware modification

---

## 14. Recommended Next Live Test

### SINGLE BEST NEXT TEST:

**Test whether `cos` re-applies Lifemote config at boot.**

Procedure:
1. Enable Lifemote Agent via GTPR `so` (set URL to a known server)
2. Verify phoenix.sh starts (check with GTPR or observe HTTP request)
3. Reboot router (requires explicit approval)
4. After reboot, check if phoenix.sh auto-started WITHOUT re-enabling via GTPR
5. If auto-started: autostart path is PROVEN
6. If not: no autostart path exists

Risk: MEDIUM (requires reboot, but Lifemote is a manufacturer feature)

### SECONDARY TEST:

**Determine if misc_rw_bak is available and writable.**

If `misc_rw_bak` exists and has more space, the binary could go there.

---

## 15. Final Decision

### EXECUTION PATH AUDIT

#### Proven execution paths:

1. GTPR `so` → fixed vendor binary launch (telnetd, dropbear, phoenix.sh)
2. Lifemote Agent → remote script download → shell
3. Telnet CLI → doFshell → root shell

#### Candidate persistence mechanisms:

1. **Lifemote Agent + phoenix.sh** — supervisor keeps script alive within boot session
2. **misc_rw binary storage** — BLOCKED by 1144 KB capacity limit

#### Candidate autostart mechanisms:

1. **Lifemote config re-apply at boot** — PLAUSIBLE but UNKNOWN (depends on cos behavior)

#### misc_rw:

```
existence:    PROVEN-OFFLINE
writable:     PROVEN-OFFLINE
executable:   PROVEN-OFFLINE
persistent:   PROVEN-OFFLINE
boot-accessible: PROVEN-OFFLINE
capacity:     1144 KB (PROVEN-LIVE) — TOO SMALL for binary
```

#### Arbitrary external execution:

```
PROVEN-OFFLINE (via fixed vendor binaries only)
Not arbitrary — all launches are hardcoded vendor paths
```

#### Best architecture:

Given the 1144 KB constraint, the architecture must be:

```
External HTTP server (always-on)
    ↓
Lifemote Agent (persistent config in misc_rw)
    ↓
phoenix.sh (auto-starts IF cos re-applies config)
    ↓
Downloads Detectic binary from HTTP server to /tmp
    ↓
Executes Detectic
    ↓
phoenix.sh supervisor maintains Detectic
```

This requires:
1. Always-on HTTP server on the LAN
2. cos re-apply behavior confirmed (UNKNOWN)
3. Binary stored on HTTP server, not on router

#### What still requires live shell:

1. Whether cos re-applies config at boot (autostart)
2. Whether misc_rw_bak is available (capacity)
3. Actual free space in misc_rw
4. Process tree after boot
5. Whether phoenix.sh survives in process table after boot

#### SINGLE BEST NEXT TEST:

**Enable Lifemote Agent, reboot, check if phoenix.sh auto-starts.**

This single test determines whether autostart is possible without firmware modification.
