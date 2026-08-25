# EX520 On-Router Rollback Design

## Objective

Ensure Detectic can be completely removed from a stock TP-Link EX520 without firmware recovery or router reconfiguration if on-router deployment is ever authorized.

## Preconditions

- Deployment is limited to /var/run/misc/misc_rw
- No firmware modification
- No bootloader modification
- No persistent router configuration changes

## Rollback Steps

1. Stop Detectic process
   - killall detectic or pkill detectic
   - Verify process not running: ps | grep detectic

2. Remove binary and data
   - rm -rf /var/run/misc/misc_rw/detectic
   - rm -f /var/run/misc/misc_rw/detectic.db
   - rm -f /var/run/misc/misc_rw/detectic_buffer.jsonl

3. Remove configuration
   - rm -f /var/run/misc/misc_rw/detectic_config.json
   - rm -f /var/run/misc/misc_rw/detectic_secret.key

4. Remove logs
   - rm -f /var/log/detectic.log

5. Restore access if changed
   - If Telnet/SSH were enabled via data-model for deployment access, restore original backupcfg or set:
     DEV2_TELNET_CFG telnetLocalEnabled=0
     DEV2_SSH_CFG enable=0
   - This step requires GTPR write authorization and is out of scope for discovery-only phase.

6. Verify removal
   - ls /var/run/misc/misc_rw/detectic → should not exist
   - ps → no detectic process
   - df → storage reclaimed

## Recovery without Reboot

Rollback can be performed without reboot if changes are limited to misc_rw.

## Firmware Recovery Not Required

If deployment never writes to rootfs, init scripts, or bootloader, firmware recovery is not needed.

## Safety Notes

- Never modify /etc/init.d/rcS
- Never remount rootfs read-write for permanent changes
- Keep original backupcfg.bin untouched
- Document any configuration changes made to enable shell access

## Rollback Test Procedure

1. Deploy Detectic to test environment with authorization
2. Run for 24h
3. Execute rollback steps above
4. Verify router services unchanged: WAN, LAN, DHCP, Wi-Fi, DNS
5. Verify no files remain in misc_rw

## Risk

LOW if deployment remains in misc_rw and no config changes are persisted.

MEDIUM if Telnet/SSH are enabled via data-model — requires config restore.

## Definition of Done

- Removal script documented
- No persistent artifacts remain
- Router configuration restored to pre-deployment state
- No firmware modification required for removal
