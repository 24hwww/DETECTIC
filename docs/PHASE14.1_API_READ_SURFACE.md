# PHASE14.1_API_READ_SURFACE.md

## Known read-only OIDs from evidence

Device/system:
DEV2_DEV_INFO — device info, manufacturer, model, serial, MAC

Wi-Fi / stations:
DEV2_WIFI_APDEV_ASSOCDEV — associated devices, MAC, RSSI, hostname, IP, standard, rates
DEV2_HOST_ENTRY — host table
DEV2_DHCPV4_CLIENT — DHCP client
DEV2_WIFI_STEERINGSTATS — band steering
DEV2_WIFI_MACTABLE — not populated
DEV2_WIFI_DE_UNASSOCSTA — not populated
DEV2_WIFI_APDEV_ETHASSOCDEV — not populated

Configuration metadata:
DEV2_TELNET_CFG — telnet enable state
DEV2_SSH_CFG — SSH enable state
DEV2_USER_CFG — user account settings
DEV2_LIFEMOTE_AGENT — lifemote agent enable/URL

All OIDs accessed via gl operation. Read-only semantics proven for DEV2_WIFI_APDEV_ASSOCDEV.

Safe to query: YES for listed OIDs.

No arbitrary shell execution proven.
