# PHASE14.1 OID CLASSIFICATION

Classification keys:
DOCUMENTED = OID name present in _rootfs/web/js/oid_str.js, web UI, or reverse-engineered data model
OBSERVED-LIVE = OID returned data from live router via GTPR
ACCESSIBLE = gl returns success=true on live router
DENIED = gl returns errorcode 9003 / 9804 or permission denied on live router
UNKNOWN = No live test performed

## System status / firmware / version

| OID | DOCUMENTED | OBSERVED-LIVE | ACCESSIBLE | DENIED | UNKNOWN | Notes |
|-----|------------|---------------|------------|--------|---------|-------|
| DEV2_DEV_INFO | YES | NO | NO | YES | - | oid_str.js:782. Live gl → errorcode 9003. Not accessible to user role |
| DEV2_DEVICE_INFO | NO | NO | NO | YES | - | Not in oid_str.js. Live gl → errorcode 9804 |
| DEV2_MEM_STATUS | YES | NO | - | - | YES | oid_str.js:786 |
| DEV2_PROC_STATUS | YES | NO | - | - | YES | oid_str.js:788 |
| DEV2_STATUS_PROC | YES | NO | - | - | YES | oid_str.js:790 |
| DEV2_RUNNING_STATUS | YES | NO | - | - | YES | oid_str.js:798 |
| DEV2_BOOT_CAUSE | YES | NO | - | - | YES | oid_str.js:806 |
| DEV2_FW_IMAGE | YES | NO | - | - | YES | oid_str.js:796 |
| DEV2_RB_INFO | YES | NO | - | - | YES | oid_str.js:802 |

## Service state / remote access

| OID | DOCUMENTED | OBSERVED-LIVE | ACCESSIBLE | DENIED | UNKNOWN | Notes |
|-----|------------|---------------|------------|--------|---------|-------|
| DEV2_TELNET_CFG | YES | NO | - | YES | - | oid_str.js:876. Live gl → errorcode 9003 |
| DEV2_SSH_CFG | YES | NO | - | YES | - | oid_str.js:838. Live gl → errorcode 9003 |
| DEV2_HTTP_CFG | YES | NO | - | - | YES | oid_str.js:840 |
| DEV2_USER_CFG | YES | NO | - | YES | - | oid_str.js:828. Live gl → errorcode 9003 |
| DEV2_CURRENT_USER | YES | NO | - | - | YES | oid_str.js:830 |
| DEV2_LOGINUSER | YES | NO | - | - | YES | oid_str.js:832 |
| DEV2_WEBLOGINUSER | YES | NO | - | - | YES | oid_str.js:834 |
| DEV2_MANAGEMENT_SERVER | YES | NO | - | - | YES | oid_str.js:808 |
| DEV2_SYSMODE | YES | NO | - | - | YES | oid_str.js:884 |
| DEV2_SYS_CFG | YES | NO | - | - | YES | oid_str.js:826 |
| DEV2_UI_REMOTE_ACCESS | YES | NO | - | - | YES | oid_str.js:822 |

## Startup / diagnostics / storage / filesystem

| OID | DOCUMENTED | OBSERVED-LIVE | ACCESSIBLE | DENIED | UNKNOWN | Notes |
|-----|------------|---------------|------------|--------|---------|-------|
| DEV2_DIAG_TOOL | YES | NO | - | - | YES | oid_str.js:850 |
| DEV2_EASYDIAG_CFG | YES | NO | - | - | YES | oid_str.js:852 |
| DEV2_VENDOR_CFG_FILE | YES | NO | - | - | YES | oid_str.js:794 |
| DEV2_VENDOR_LOG_FILE | YES | NO | - | - | YES | oid_str.js:792 |
| DEV2_SYSLOG_CFG | YES | NO | - | - | YES | oid_str.js:784 |
| DEV2_REBOOT_SCHEDULE_CFG | YES | NO | - | - | YES | oid_str.js:866 |

## Proven live read-only OIDs

| OID | ACCESSIBLE | Purpose |
|-----|------------|---------|
| DEV2_WIFI_APDEV_ASSOCDEV | YES | Associated Wi-Fi devices, RSSI, MAC |
| DEV2_HOST_ENTRY | YES | ARP/host table |
| DEV2_DHCPV4_CLIENT | YES | DHCP client/lease info |

## Summary

* DEV2_DEV_INFO is DOCUMENTED but DENIED for user role via GTPR.
* DEV2_DEVICE_INFO is NOT DOCUMENTED, live test returned errorcode 9804.
* No system/firmware/version OID has been proven ACCESSIBLE with current credentials.
* Service state OIDs for Telnet/SSH/User are DOCUMENTED but DENIED / UNKNOWN via GTPR.
* Phase 14 objective remains DEPLOY+PERSIST+AUTOSTART; GTPR read-only enumeration is secondary.

No writes, no reboot, no configuration changes performed.
