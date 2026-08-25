# PHASE14_ACCESS_RECOVERY.md

## Management path recovery

### Step 1 — Network state
ip -6 neigh show
fe80::3e6a:d2ff:fe5f:abc1 dev enp2s0 lladdr 3c:6a:d2:5f:ab:c1 router STALE

MAC 3c:6a:d2:5f:ab:c1 matches EX520.

Interface enp2s0 confirmed.

### Step 2 — IPv6 discovery
IPv6 link-local fe80::3e6a:d2ff:fe5f:abc1 reachable via enp2s0.

### Step 3 — GTPR/GDPR path
Previous implementation detectic_client.py uses:
POST /cgi/getGDPRParm
POST /cgi_gdpr?9
GET /
gl/go operations

### Step 4 — Credentials
User: user
Password: ***
URL: http://[fe80::3e6a:d2ff:fe5f:abc1%enp2s0]

Evidence of successful login:
[DEBUG login] status=200 jsessionid-present=True
[DEBUG gl] status=200 body_len=6424

### Step 5 — Management path matrix

| Access path | Address | Port/API | Previously proven? | Currently reachable? |
| IPv4 HTTP | 192.168.0.1 | 80 | Yes | No |
| IPv4 HTTPS | 192.168.0.1 | 443 | ? | No |
| IPv6 HTTP | fe80::3e6a:d2ff:fe5f:abc1%enp2s0 | 80 | Yes | YES |
| IPv6 HTTPS | fe80::3e6a:d2ff:fe5f:abc1%enp2s0 | 443 | ? | ? |
| GTPR/GDPR | fe80::3e6a:d2ff:fe5f:abc1%enp2s0 | /cgi/getGDPRParm /cgi_gdpr | YES | YES |
| Telnet | ? | 23 | ? | UNKNOWN |
| SSH | ? | 22 | ? | UNKNOWN |

### Conclusion
Access recovered via IPv6 link-local GTPR/GDPR.
Router is reachable.
No changes made to router.
Read-only API access confirmed.

Next: Phase 14 read-only reconnaissance via API.
