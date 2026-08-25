# EX520 On-Router Deployment Discovery

## Hardware

CPU: MediaTek MT7981
Architecture: ARM64 aarch64
ABI: linux-musl little endian
libc: musl
Kernel: Linux 5.x OpenWrt 21.02.mtk based
SoC: Cortex-A53 quad-core
Flash: SPI NAND 128 MB
Rootfs: SquashFS read-only
Data partitions: UBI misc_ro, misc_rw, misc_rw_bak, misc_isp

## Runtime

Can Detectic run natively? YES
Required build target: aarch64-unknown-linux-musl
Dependencies: static Rust, ureq, aes/cbc, rsa, md5, hmac, sha2, serde
Binary size target: <3 MB stripped
Filesystem requirements: executable + config in writable UBI
Dynamic dependencies: none for no-default-features build

## Storage

Candidate mechanisms:

| Mechanism | Exists | Writable | Survives Reboot | Survives Power Loss | Autostart | Reversible |
|-----------|--------|----------|-----------------|---------------------|-----------|------------|
| /var/run/misc/misc_rw | YES | YES | YES | YES | NO | YES |
| /var/run/misc/misc_ro | YES | NO | YES | YES | NO | N/A |
| /var/tmp | YES | YES | NO | NO | NO | YES |
| /tmp | YES | YES | NO | NO | NO | YES |
| /var/log | YES | YES | NO* | NO* | NO | YES |
| /etc | YES | NO | YES | YES | NO | N/A |
| /etc/init.d | YES | NO | YES | YES | YES | NO |
| backupcfg.bin restore | YES | Config only | YES | YES | NO | YES |

*Log files may persist on tmpfs until power loss; /var is ramfs per fstab.

Binary:
- Path candidate: /var/run/misc/misc_rw/detectic/
- Persistent: YES
- Notes: UBI partition survives reboot/power

Configuration:
- Path candidate: /var/run/misc/misc_rw/detectic/config.json
- Persistent: YES

Secret:
- Path candidate: /var/run/misc/misc_rw/detectic/secret.key
- Persistent: YES, must survive reboot

Dataset:
- Path candidate: /var/run/misc/misc_rw/detectic/data.db
- Persistent: YES

Logs:
- Path candidate: stdout/syslog or /var/log/detectic.log
- Persistent: NO / volatile

## Autostart

Mechanism:
- Init system: BusyBox init with /etc/init.d/rcS
- Inittab: ::sysinit:/etc/init.d/rcS
- rcS mounts UBI partitions misc_rw/ro
- No user-writable init hooks found in rootfs
- /etc/rcS_hook exists but empty
- No cron daemon started by rcS
- No vendor app plugin mechanism found

Exists: YES for vendor init, NO for user-writable autostart
Persistent: UNKNOWN — requires explicit test authorization to verify a startup hook can be added without firmware modification

## Deployment

Possible without firmware modification: UNKNOWN
Possible without privilege escalation: UNKNOWN
Possible without router configuration changes: UNKNOWN

The stock firmware has read-only rootfs and no writable init hook. Detectic could run manually from misc_rw after a shell is obtained, but automatic start after boot cannot be confirmed without a reboot test or firmware modification analysis.

## Persistence

Reboot persistence: YES for data stored in /var/run/misc/misc_rw
Power-cycle persistence: YES for data stored in /var/run/misc/misc_rw
Autostart persistence: UNKNOWN — REQUIRES EXPLICIT TEST AUTHORIZATION

## Risk

MEDIUM

Explanation:
- Running Detectic from misc_rw is technically feasible with a static aarch64-musl binary.
- No writable autostart mechanism is visible in the extracted rootfs.
- Backup/restore is configuration-only and cannot deploy code.
- Enabling Telnet/SSH via data-model could provide shell access but does not create persistence.
- No firmware modification is required for read-only discovery, but any persistent autostart would likely require either a firmware modification or an undocumented vendor hook.

## Recommended Deployment

Safest legitimate approach:
1. Keep Detectic running externally using GTPR/GDPR API over LAN — already validated.
2. If on-router execution is required, obtain explicit authorization for a controlled deployment test:
   - Verify exact writable paths on live device
   - Verify whether rcS or a vendor hook can be extended from misc_rw
   - Confirm that a static binary runs under BusyBox environment
   - Confirm resource limits and impact on router services
3. Do not modify firmware, bootloader, or configuration until discovery is complete.

## Rollback

Summary:
- Remove binary and data from /var/run/misc/misc_rw/detectic/
- If Telnet/SSH were enabled via data-model, restore original configuration via backupcfg
- No firmware recovery needed if changes are limited to misc_rw
- Detailed rollback procedure in EX520_ON_ROUTER_ROLLBACK.md

## Stop Conditions

STOP immediately if firmware modification, bootloader modification, secure boot bypass, exploit, privilege escalation, router configuration change, persistent write, reboot or power-cycle is required for discovery. This report is based on static analysis only.

## Definition of Done

- EX520 architecture verified: YES
- Detectic runtime requirements documented: YES
- Persistent filesystem locations identified: YES
- Autostart mechanisms identified: PARTIAL — mechanism exists but user-writable hook not confirmed
- Detectic compatibility assessed: YES
- Resource requirements documented: YES
- Persistent secret strategy documented: YES
- Deployment architecture proposed: YES
- Rollback documented: YES
- NO installation performed: YES
- NO persistent changes made: YES
- NO reboot performed: YES
