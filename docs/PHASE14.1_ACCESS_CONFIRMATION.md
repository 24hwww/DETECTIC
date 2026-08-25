# PHASE14.1_ACCESS_CONFIRMATION.md

## Verified access

IPv6 link-local: fe80::3e6a:d2ff:fe5f:abc1%enp2s0
Interface: enp2s0
MAC: 3c:6a:d2:5f:ab:c1
Transport: HTTP/80
Protocol: TP-Link GTPR/GDPR
User: user
Password: ***

Operations proven:
POST /cgi/getGDPRParm → 200
POST /cgi_gdpr?9 → JSESSIONID present
GET / → TokenID obtained
gl DEV2_WIFI_APDEV_ASSOCDEV → 200, devices returned

Evidence:
[DEBUG login] status=200 jsessionid-present=True
[DEBUG gl] status=200 body_len=6424
RadioMac 3C:6A:D2:5F:AB:C1

No modifications performed.
Read-only access PROVEN-LIVE.
