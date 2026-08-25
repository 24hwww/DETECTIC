# PHASE14.1_RUNTIME_RECON_UPDATE

Test: gl DEV2_DEV_INFO
HTTP status: 200
Decoded response:
{
  "data": [],
  "operation": "gl",
  "oid": "DEV2_DEV_INFO",
  "success": false,
  "errorcode": 9003
}

Classification: DEV2_DEV_INFO = NOT ACCESSIBLE to user 'user' via GTPR. Errorcode 9003.

Alternative OIDs proven accessible:
DEV2_WIFI_APDEV_ASSOCDEV → success true
DEV2_HOST_ENTRY → success true
DEV2_DHCPV4_CLIENT → success true

No hardware model/firmware version obtained via GTPR with current user credentials.

DEV2_DEV_INFO = PROVEN-LIVE NOT ACCESSIBLE
misc_rw = UNKNOWN
arbitrary execution = UNKNOWN
persistence = UNKNOWN
autostart = UNKNOWN

Next lowest-risk test: query DEV2_SYSTEM_STATUS or other read-only system OID to attempt version exposure.
