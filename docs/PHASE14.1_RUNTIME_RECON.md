# PHASE14.1_RUNTIME_RECON.md

## Runtime information obtained via GTPR

DEV2_WIFI_APDEV_ASSOCDEV returned:
- RadioMac 3C:6A:D2:5F:AB:C1
- Associated devices with hostname, IP, MAC, signalStrength, operatingStandard, associationTime

No filesystem mount info obtained via API.
No process list obtained via API.
No storage topology exposed via GTPR.

API does not expose:
- mount points
- /proc
- /var/run/misc/misc_rw existence
- writable partitions

Classification:
PROVEN-LIVE: Wi-Fi association data via GTPR
UNKNOWN: filesystem/storage topology via GTPR
UNKNOWN: process/service status via GTPR
