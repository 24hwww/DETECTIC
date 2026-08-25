# PHASE12F_LIVE_BASELINE

## 12F.0 LIVE ACCESS GATE — RESULT

**Status: BLOCKED — EX520 NOT REACHABLE**

Development machine: 192.168.0.27/24 (enp2s0)
Default gateway: 192.168.0.1 (not pingable)
Other interfaces: virbr0 (192.168.122.1), lxcbr0 (10.0.3.1), docker0 (172.17.0.1), br-3da918ab8c7d (172.18.0.1)

Scanned:
- 192.168.0.1: NOT REACHABLE
- 192.168.1.1: NOT REACHABLE
- 192.168.0.254: NOT REACHABLE
- 192.168.0.{1..10}: 0 hosts reachable

The EX520 is on a different physical network. Live access requires:
1. Physical LAN connection to the EX520, OR
2. VPN/remote access to the EX520's network, OR
3. Direct serial/UART connection

**Classification: BLOCKED**

## 12F.1 HARD SAFETY GATE — OFFLINE ANALYSIS

Since live access is unavailable, we record what we know OFFLINE and what requires LIVE evidence.

### Known from previous offline analysis (PROVEN-OFFLINE):

- **Firmware**: EX520V124101568249n_agc3000_0945460481
- **SoC**: MediaTek MT7981 (ARM64, Cortex-A53)
- **Architecture**: aarch64, little-endian
- **Flash**: SPI NAND 128M, UBI
- **RootFS**: SquashFS/UBI read-only, rootfsA 50 MiB
- **Dual image**: kernelA/B, rootfsA/B
- **Init**: BusyBox inittab → `/etc/init.d/rcS`
- **Persistent RW**: `/var/run/misc/misc_rw` (UBI volume)
- **Optional RW**: `/var/run/runtime_data` (not enabled in this build)
- **Backup format**: DES-ECB + zlib + MD5 XML config

### UNKNOWN (requires LIVE evidence):

| Item | Status | Why |
|------|--------|-----|
| Router IP on target LAN | UNKNOWN | Not on same network |
| LAN connectivity | UNKNOWN | Physical access required |
| Current services running | UNKNOWN | Need `ps` output |
| misc_rw actual free space | UNKNOWN | Need `df -h` output |
| misc_rw mount options | UNKNOWN | Need `mount` output |
| Current firmware build running | UNKNOWN | Need `cat /proc/version` |
| RAM capacity | UNKNOWN | Need `cat /proc/meminfo` or `free` |
| CPU info | UNKNOWN | Need `cat /proc/cpuinfo` |
| Network interfaces | UNKNOWN | Need `ip addr` or `iw dev` |
| Wi-Fi interfaces | UNKNOWN | Need `iw dev` |
| Current running processes | UNKNOWN | Need `ps` |
| Open ports | UNKNOWN | Need `netstat -tuln` |
| Telnet availability | UNKNOWN | Need port scan or telnet attempt |
| SSH availability | UNKNOWN | Need port scan or ssh attempt |
| BusyBox command set | UNKNOWN | Need `busybox` output |
| Shell access method | UNKNOWN | Need to determine |

## 12F.2 PRISTINE BACKUP SAFETY — OFFLINE

### Backup artifacts present:

| File | Size | SHA-256 |
|------|------|---------|
| `EX520V124101568249n_agc3000_0945460481_backupcfg.bin` | present | previously verified |
| `detectic-router-backup.bin` | present | working copy |

### Backup safety status:

- Original backup preserved: YES (file exists in project)
- Working copy created: YES (detectic-router-backup.bin)
- SHA-256 calculated: PROVEN-OFFLINE (previously verified)
- Backup format understood: PROVEN-OFFLINE (DES-ECB + zlib + MD5)
- Key derivation understood: PROVEN-OFFLINE (hardcoded constant XOR DeviceInfo)
- 32-bit DeviceInfo value: UNKNOWN (requires live router or known password)
- Pristine backup from live router: UNKNOWN (not yet exported from live device)

### What is BLOCKED:

- Cannot export pristine backup from live router (no access)
- Cannot verify restore procedure (no access)
- Cannot verify SHA-256 against live router's current config

### Classification:

- Backup format: **PROVEN-OFFLINE**
- Restore procedure: **UNKNOWN** (never tested on live router)
- Pristine backup from live device: **UNKNOWN**

## 12F.3 UART/RECOVERY GATE — OFFLINE

### Known:

- EX520V PCB photos/analysis: not available in this session
- UART pins: not identified from available evidence
- Serial console: not confirmed
- Recovery procedure: not demonstrated

### Classification:

- UART pin identification: **UNKNOWN**
- UART connectivity: **UNKNOWN**
- Recovery procedure: **UNKNOWN**
- Recovery capability: **UNKNOWN**

### Implication:

Without confirmed recovery path, ALL configuration-changing tests must be considered HIGH RISK.

## 12F.4 REAL MISC_RW DISCOVERY — OFFLINE

### Known from static analysis (PROVEN-OFFLINE):

- Path: `/var/run/misc/misc_rw`
- Filesystem: UBIFS
- Mounted by: `rcS` init script
- Contains: data model binary `0x00300000`
- Persistence: survives reboot (confirmed by rcS code analysis)
- Persistence: survives service restart
- Persistence: does NOT survive factory reset

### Known from design targets (NOT PROVEN):

- Design target: ≥12 MB free (from 12A inventory)
- Operational target: ~20 MB (from 12A inventory)
- These are DESIGN TARGETS, not measured facts

### UNKNOWN (requires LIVE evidence):

| Item | Status |
|------|--------|
| Total capacity | UNKNOWN |
| Current free space | UNKNOWN |
| Mount options | UNKNOWN |
| Block size | UNKNOWN |
| File count | UNKNOWN |
| Existing files/dirs | UNKNOWN |
| Permissions | UNKNOWN |
| Ownership | UNKNOWN |
| Can create directories | UNKNOWN |
| Can create files | UNKNOWN |
| Can execute from | UNKNOWN |

### Storage budget (design targets, NOT proven):

| Component | Estimated |
|-----------|-----------|
| Binary (current build) | 1,278,728 bytes (1.22 MB) |
| Previous version backup | ~1.3 MB |
| Temporary upload staging | ~1.3 MB |
| State files | <100 KB |
| Spool (in /tmp, not misc_rw) | 0 in misc_rw |
| Logs | 0 in misc_rw |
| Safety margin (50%) | ~2 MB |
| **Total estimated** | **~6 MB** |

NOTE: The spool file defaults to `/tmp/detectic_buffer.jsonl`, NOT to misc_rw. This means misc_rw only needs to store the binary, not the queue.

### Classification:

- misc_rw exists: **PROVEN-OFFLINE**
- misc_rw persists reboot: **PROVEN-OFFLINE** (code analysis)
- Actual capacity: **UNKNOWN**
- Actual free space: **UNKNOWN**
- Can store Detectic binary: **UNKNOWN**

## 12F.5 SAFE WRITE/PERSISTENCE PROBE — BLOCKED

Requires live access. Cannot create markers or test persistence.

**Classification: BLOCKED**

## 12F.6 REAL ARM64 EXECUTION PROBE — OFFLINE ANALYSIS

### Binary verification (PROVEN-OFFLINE):

| Property | Value |
|----------|-------|
| File | `target/aarch64-unknown-linux-musl/release/detectic` |
| Size | 1,278,728 bytes |
| Architecture | ELF 64-bit LSB, ARM aarch64 |
| Type | EXEC (executable) |
| Linking | Statically linked |
| Stripped | Yes |
| Dynamic deps | None |
| Entry point | 0x275f00 |
| Program headers | 9 |
| Section headers | 19 |
| Built with | `--no-default-features` (no persist/TLS) |
| Target CPU | Cortex-A53 (ARMv8-A baseline) |
| Linker | rust-lld (self-contained) |

### Transfer mechanism:

UNKNOWN. Options to evaluate on live router:
- SSH/SCP: not confirmed available
- SFTP: not confirmed available
- Telnet + cat/echo: possible but slow, unproven
- HTTP upload: possible if web UI accepts
- Controller-side mechanism: depends on management transport

### What the code actually does (PROVEN-FROM-SOURCE):

- Does NOT use SCP, SFTP, pidof, pgrep
- Does NOT use /proc/<pid>/exe for process inspection
- Does NOT have --daemon, --log flags
- Does NOT have heartbeat endpoint
- Uses HTTP (ureq) to talk to router GTPR API and backend
- Reads /proc/self/status for VmRSS
- Reads /proc/self/stat for uptime
- Reads /proc/uptime for uptime
- Signal handling: SIGTERM/SIGINT via libc signal()
- Single-threaded sensor loop
- Spool default: `/tmp/detectic_buffer.jsonl`

### Classification:

- Binary architecture: **PROVEN-OFFLINE**
- Binary static linking: **PROVEN-OFFLINE**
- Binary executable on aarch64: **UNKNOWN** (never executed on real hardware)
- Transfer mechanism: **UNKNOWN**
- File can be placed in misc_rw: **UNKNOWN**

## 12F.7 REAL TELNET VALIDATION — OFFLINE

### Known from static analysis (PROVEN-OFFLINE):

- `telnetd` binary present in firmware image
- `dropbear` binary present in firmware image
- Data model objects exist: `DEV2_TELNET_CFG`, `DEV2_SSH_CFG`
- Apply handlers exist: `telnetd -p %d &`, `dropbear -p %d -r %s -d %s -A %s &`
- Config flags: `INCLUDE_WEB_TELNET=y`, `INCLUDE_REMOTE_TELNET=y`, `INCLUDE_SSH_ACCESS` not set
- Backup format can enable Telnet via config modification

### UNKNOWN (requires LIVE evidence):

| Item | Status |
|------|--------|
| Telnet currently enabled on live router | UNKNOWN |
| Telnet port on live router | UNKNOWN |
| Telnet credentials | UNKNOWN (default admin?) |
| SSH currently enabled | UNKNOWN |
| Telnet accessible from LAN | UNKNOWN |
| Telnet accessible from WAN | UNKNOWN |
| Telnet persists reboot | UNKNOWN (design predicts yes) |
| Telnet survives config restore | UNKNOWN |
| Backup password (if any) | UNKNOWN |
| 32-bit DeviceInfo value | UNKNOWN |

### Classification:

- Telnet binary in firmware: **PROVEN-OFFLINE**
- Telnet enablement via config: **PROVEN-OFFLINE** (code analysis)
- Telnet actually enabled: **UNKNOWN**
- Telnet persists reboot: **UNKNOWN**
- Telnet credentials: **UNKNOWN**
- SSH availability: **UNKNOWN**

## 12F.8 TELNET PERSISTENCE — BLOCKED

Requires live access + Telnet enabled first.

**Classification: BLOCKED**

## 12F.9 MANAGEMENT TRANSPORT — OFFLINE

### Known (PROVEN-FROM-SOURCE):

The Detectic binary communicates via:
- HTTP to router GTPR API (192.168.0.1:80 by default)
- HTTP to backend URL (configurable)

The binary does NOT:
- Use SSH/SCP for file transfer
- Use Telnet for management
- Execute shell commands remotely
- Use pidof/pgrep

### External controller transport options:

1. **SSH/SCP**: Requires SSH enabled on router. Not confirmed.
2. **Telnet + manual file transfer**: Possible via `cat`/`echo` base64 encoding. Slow, fragile.
3. **HTTP upload via web UI**: Possible if CGI endpoint accepts file upload. Unconfirmed.
4. **Physical media**: USB, SD card. Not investigated.
5. **Detectic itself could serve as transport**: If Detectic runs on router, it could receive updates via backend API. But this requires Detectic to already be running.

### Classification:

- HTTP transport (GTPR): **PROVEN-FROM-SOURCE**
- SSH transport: **UNKNOWN** (SSH not confirmed)
- Telnet transport: **UNKNOWN** (Telnet not confirmed)
- SCP file transfer: **UNKNOWN**
- Best transfer mechanism: **UNKNOWN**

## 12F.10 DETECTIC ARTIFACT VALIDATION — OFFLINE

### Binary properties (PROVEN-OFFLINE):

- Version: 0.1.0
- Size: 1,278,728 bytes
- SHA-256: 89abf70c17c4cab3703f6ed52f946f989413f10a7d0a5002a5b06d519cb797cd
- Architecture: aarch64
- Statically linked: yes
- Stripped: yes
- No C dependencies
- No dynamic libraries required

### CLI subcommands (PROVEN-FROM-SOURCE):

- `sensor` (loop or --once)
- `map`
- `presence`
- `status`
- `version`
- `health`
- `config`
- `spool`
- `update` (--check only)
- `rollback` (prints instructions)
- `uninstall` (prints instructions)
- `query` (GTPR OID)
- `set` (GTPR OID)
- `cgi` (CGI endpoint)
- `op` (action)
- `driver`
- `realtime`
- `launcher` (install/remove/status)
- Capture, Stats, Report, Analytics (persist-gated, not in this build)

### Environment variables (PROVEN-FROM-SOURCE):

- DETECTIC_URL (default: http://192.168.0.1)
- DETECTIC_USER (default: user)
- DETECTIC_PASSWORD
- DETECTIC_SECRET
- DETECTIC_DIALECT (json/text)
- DETECTIC_INTERVAL
- DETECTIC_UPLOAD_URL
- DETECTIC_SENSOR_ID (default: home-001)
- DETECTIC_BUFFER (default: /tmp/detectic_buffer.jsonl)
- DETECTIC_BUFFER_MAX (default: 65536)
- DETECTIC_DB (default: detectic.db)
- DETECTIC_LOG_LEVEL
- DETECTIC_LOG_MACS
- DETECTIC_BACKEND_URL
- DETECTIC_HEALTH_JSON

### Writable paths (PROVEN-FROM-SOURCE):

- Spool: `/tmp/detectic_buffer.jsonl` (NOT persistent across reboot)
- State: `/var/run/misc/misc_rw/detectic/state/sensor_id`
- Config scripts: `/var/run/misc/misc_rw/detectic/current/detectic-*.sh`

### NOT supported (PROVEN-FROM-SOURCE):

- `--daemon` flag
- `--log` flag
- Heartbeat endpoint
- Structured health file
- PID file
- stdout-based heartbeat

### Classification:

- Binary properties: **PROVEN-OFFLINE**
- CLI behavior: **PROVEN-FROM-SOURCE**
- Environment variables: **PROVEN-FROM-SOURCE**
- Writable paths: **PROVEN-FROM-SOURCE**
- Execution on real hardware: **UNKNOWN**
- Actual RSS/memory: **UNKNOWN**
- Actual CPU usage: **UNKNOWN**
