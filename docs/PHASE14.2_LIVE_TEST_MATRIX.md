# PHASE14.2 LIVE TEST MATRIX

## Objective constraints
- No firmware modification
- Read-only reconnaissance only
- GTPR/GDPR proven live via IPv6 fe80::3e6a:d2ff:fe5f:abc1%enp2s0
- Proven OIDs: DEV2_WIFI_APDEV_ASSOCDEV, DEV2_HOST_ENTRY, DEV2_DHCPV4_CLIENT
- Denied OIDs: DEV2_DEV_INFO, DEV2_TELNET_CFG, DEV2_SSH_CFG, DEV2_USER_CFG (errorcode 9003)
- misc_rw existence/writable/executable/persistent = UNKNOWN via GTPR

## Test Matrix

| Test | Purpose | Read-only? | Requires shell? | Requires GTPR? | Risk | Information gained |
|------|---------|------------|-----------------|----------------|------|--------------------|
| T1: Query DEV2_WIFI_APDEV_ASSOCDEV again | Confirm live Wi-Fi observation capability stability | Yes | No | Yes | Low | RF processing viability, device list consistency |
| T2: Query DEV2_HOST_ENTRY | Confirm host table visibility | Yes | No | Yes | Low | ARP/host visibility |
| T3: Query DEV2_DHCPV4_CLIENT | Confirm DHCP client state | Yes | No | Yes | Low | WAN connectivity state |
| T4: Query DEV2_SYS_CFG | Retrieve documented system configuration OID | Yes | No | Yes | Low | System config exposure, possible version hints |
| T5: Query DEV2_MEM_STATUS | Retrieve documented memory status OID | Yes | No | Yes | Low | Resource constraints |
| T6: Query DEV2_PROC_STATUS | Retrieve documented process status OID | Yes | No | Yes | Low | Running services |
| T7: Attempt gl DEV2_TELNET_CFG with user credentials | Verify telnet config accessibility | Yes | No | Yes | Low | Telnet enablement state if accessible |
| T8: Attempt gl DEV2_SSH_CFG with user credentials | Verify SSH config accessibility | Yes | No | Yes | Low | SSH enablement state if accessible |

Notes on test selection:
- Only OIDs present in _rootfs/web/js/oid_str.js are proposed.
- All tests are read-only gl operations via proven GTPR path.
- No OID brute-forcing.
- No writes, no config changes.

## Decision Tree: Shell Requirement

```
GTPR/GDPR sufficient?
  |
  +-- YES → Deploy Detectic via GTPR-only data collection
  |          Detectic can read Wi-Fi association data via GTPR
  |          No shell needed for RF processing
  |
  +-- NO → Persistent deployment + autostart requires shell
           |
           +-- Shell required to:
           |   * Verify misc_rw existence/writable/executable/persistent
           |   * Copy binary to /var/run/misc/misc_rw/detectic
           |   * Register autostart via rcS_hook or procd
           |   * Test Detectic execution
           |
           +-- Shell access paths:
           |   * SSH: INCLUDE_SSH_ACCESS=0 → likely BLOCKED
           |   * Telnet: INCLUDE_WEB_TELNET=1, REMOTE_TELNET=1 → possibly accessible via data model
           |   * Legitimate path via DEV2_TELNET_CFG write → WOULD BE WRITE, not allowed now
           |
           +-- Conclusion: Shell required for persistence/autostart, not for RF observation
```

## Deployment Readiness Review

Artifacts reviewed:
- deploy/launcher.sh
- deploy/recon.sh
- deploy/LIVE_DEPLOYMENT_STEPS.md
- deploy/detectic-ex520.tar.gz

Findings:
- launcher.sh assumes shell access to /var/run/misc/misc_rw/detectic → requires shell
- launcher.sh depends on ps, kill, date, nohup, wc → BusyBox present, plausible
- recon.sh requires shell → requires shell
- LIVE_DEPLOYMENT_STEPS.md requires shell for recon, misc_rw verification, binary copy, autostart
- Architecture: ARM64 static musl binary → matches MT7981
- Dependencies: minimal, POSIX shell only
- Paths: /var/run/misc/misc_rw/detectic → assumes misc_rw exists and writable+executable
- Restart behavior: bounded restart 5, log rotation 100KB
- Failure recovery: restart budget, SIGTERM then SIGKILL
- Cleanup/uninstall: remove directory → requires shell

Classification: Artifacts satisfy architecture requirement IF shell access and misc_rw writable+executable+persistent are proven. Currently UNPROVEN.

## Data Persistence Review

Proposed change:
`/tmp/detectic_buffer.jsonl` → `/var/run/misc/misc_rw/detectic/spool/detectic_buffer.jsonl`

Classification: OPTIONAL

Why:
- Requirement is NOT persistent RF data, only persistent software + autostart + RF processing
- Detectic can buffer in memory and send batches; small local buffer acceptable
- Writing to misc_rw for spool adds write wear and persistence risk without business value
- Persistent RF data may increase privacy exposure
- If misc_rw is volatile or limited, unnecessary writes risk running out of space
- Recommended: keep spool in RAM /tmp unless explicit offline buffering requirement emerges

## Safest Deployment Strategies

### STRATEGY A — GTPR ONLY
What can be proven/deployed without shell?
- RF observation via `DEV2_WIFI_APDEV_ASSOCDEV` is PROVEN live
- Device presence detection via GTPR polling is PLAUSIBLE
- Persistent software deployment: BLOCKED (no shell to copy binary)
- Autostart: BLOCKED
Classification: PROVEN for RF observation, BLOCKED for persistence/autostart

### STRATEGY B — EXISTING SHELL
What becomes possible if legitimate SSH/Telnet shell access is discovered?
- Verify misc_rw existence/writable/executable/persistent → PROVEN-OFFLINE via recon.sh
- Copy Detectic binary to misc_rw → PLAUSIBLE
- Test execution → PLAUSIBLE
- Autostart via rcS_hook or procd → PLAUSIBLE
Classification: UNKNOWN until shell access proven; PLAUSIBLE if shell obtained legitimately

### STRATEGY C — EXTERNAL LAUNCHER
Conditions required before deploying external launcher through misc_rw:
1. misc_rw exists and is writable → UNKNOWN
2. misc_rw is executable → UNKNOWN
3. misc_rw persists across reboot → UNKNOWN
4. Shell access exists to copy files → UNKNOWN
5. Detectic binary runs on router → UNKNOWN until tested
Classification: UNKNOWN

## Next Single Test

Exact test:
`gl DEV2_SYS_CFG` via proven GTPR path IPv6 fe80::3e6a:d2ff:fe5f:abc1%enp2s0 with user=user password=***

Justification:
- OID documented in oid_str.js
- Read-only
- Low risk
- May reveal system configuration, firmware version hints, or service state without shell
- Does not require brute-forcing

If `DEV2_SYS_CFG` returns errorcode 9003/9804, then no further useful GTPR read-only tests remain for deployment decision; shell access is the required boundary.

## Final Phase 14.2 Status

GTPR/GDPR: PROVEN
Useful remaining read-only tests: DEV2_SYS_CFG, DEV2_MEM_STATUS, DEV2_PROC_STATUS
Shell required?: YES for persistence/autostart, NO for RF observation
misc_rw: UNKNOWN
External execution: UNKNOWN
Persistence: UNKNOWN
Autostart: UNKNOWN
Detectic deployment: NOT DEPLOYED
Persistent RF data: NOT REQUIRED
Best next test: gl DEV2_SYS_CFG read-only via GTPR
Why: Documented OID, low risk, may provide system info to inform deployment boundary without shell.
