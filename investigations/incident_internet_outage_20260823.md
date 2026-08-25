# Incident — "Wi-Fi clients without Internet" — Diagnostic Report

**Date:** 2026-08-23 15:40–15:57 (-03)
**Mode:** READ-ONLY ONLY. No configuration was changed on any device.
**Vantage:** 192.168.0.27 (wired Ethernet client, also the dev host).
**Access used:** telnet CLI :23 (admin, read-only `show` commands only), local
network probes from the working client. GTPR/HTTP unavailable (see below).

## Verdict (one line)

> The router's **IPv4 WAN path is black-holed upstream** (PPPoE/CGNAT session
> passes control frames but no IPv4 data flows back), while **IPv6 is fully
> functional**. Dual-stack devices silently ride IPv6 and appear "working";
> IPv4-dependent clients — and any client that relies on the **DHCPv4-advertised
> DNS servers (172.20.20.20/.21, unreachable)** — experience "no Internet".

## Failure layer (per incident taxonomy)

| Layer | State | Evidence |
|---|---|---|
| A Association | OK | .20/.25/.28 answer ARP/TCP through the bridge |
| B DHCP | OK | pool .20–.249 enabled; all live clients hold in-pool IPs |
| C LAN/bridge | OK | single group `Default`, isolation=0, all L2 probes succeed |
| D DNS | **AFFECTED (secondary)** | DHCPv4 DNS = 172.20.20.20/.21 → timed out (`dig @172.20.20.20`) because v4 path is dead; IPv6 RDNSS (router link-local) works |
| E NAT/WAN | **PRIMARY FAILURE** | `tracepath -4 8.8.8.8` dies after hop 1 (router); every `curl -4` times out (google.com, 1.1.1.1, neverssl.com) |
| F Client policy | NO | one group incl. both radios, isolation disabled |
| G Router-wide WAN | PARTIAL | WAN **IPv6** fine; WAN **IPv4** dead |

## Key evidence

### Protocol split test (from 192.168.0.27)
```
curl -4 https://www.google.com   -> 000 timeout
curl -6 https://www.google.com   -> 200 (0.22s)
curl -4 https://1.1.1.1          -> 000 timeout
curl -6 https://[2606:4700:4700::1111] -> 301 (0.06s)
```
Earlier domain-only tests ("HTTPS Internet verified") were false positives:
dual-stack happy-eyeballs selected IPv6.

### Traceroutes
```
tracepath -4 8.8.8.8 : hop1 192.168.0.1 (pmtu 1500->1480 = enters PPPoE), then silence
tracepath -6 ...::1111: full ISP transit (2001:12a0:..., 2804:3b0:...) ~3–10 ms
```
Packets are forwarded INTO the tunnel; nothing returns ⇒ upstream/CGNAT,
not LAN-side.

### Router state (telnet CLI, read-only shows)
- WAN `pppoe_0_0`: V4 Connected 100.64.80.76 (**CGNAT 100.64/10**), gw
  100.64.80.1; V6 Connected (SLAAC). NATEnabled=1, SPI=1, IGMP proxy on.
- LAN br0 192.168.0.1/24; DHCPv4 server .20–.249 lease 7200s;
  **DNS advertised via DHCPv4: 172.20.20.20, 172.20.20.21**.
- WLAN REYES / REYES_5G enabled WPA2-AES; BridgeEnable=0.
- Groups: single `Default`, `enableIsolation=0`,
  ports LAN1-4 + Wi-Fi 2.4G + 5G.
- WAN uptime 17774 s (~4 h 56 m) — session survived since last reboot; counters
  show historical traffic (≈96 MB RX), i.e., v4 worked earlier in-session.

### Live hosts (nmap -sn sweep of /24)
| IP | MAC (masked) | Note |
|---|---|---|
| 192.168.0.20 | d6:8a:2b:**xx:xx:xx** | randomized MAC → Wi-Fi client |
| 192.168.0.25 | 02:06:3e:**xx:xx:xx** | randomized MAC → Wi-Fi client |
| 192.168.0.27 | (this host) | wired, working via IPv6 |
| 192.168.0.28 | 22:bf:54:**xx:xx:xx** | randomized MAC → Wi-Fi client |

All three non-router hosts are reachable at L2/L3-LAN through the bridge ⇒
association/DHCP/bridging are NOT the problem.

### Management-plane anomaly (pre-existing, noted)
Router's own IPv4 TCP 22/53/80/443 + ICMP do not respond from LAN; only telnet
23 answers. HTTP-GTPR over IPv6 serves static UI (GET / = 200) but CGI returns
406; over IPv4 port 80/443 closed. This blocks remote config access but does
NOT affect client traffic paths.

## Why only *some* clients "have no Internet"

- Devices with working SLAAC/IPv6: browsers/apps fall back to v6 ⇒ appear OK.
- Devices using **DHCPv4-provided DNS only** (classic Android behavior): cannot
  resolve ANY name (v4 DNS unreachable) ⇒ "connected, no internet" although the
  v6 transport itself is fine.
- IPv4-only clients/IoT/apps with hardcoded v4 endpoints: fully broken.

## Recommended fix (NOT performed — awaiting authorization)

1. **Primary:** re-establish the WAN IPv4 session (vendor-supported action:
   web-UI/GTPR "disconnect & reconnect" of `pppoe_0_0`, equivalent of a WAN
   refresh). This forces a fresh CGNAT binding; typical cause of this exact
   signature (PPP echo alive, v4 data plane dead mid-session) is stale CGNAT
   state at the ISP after the earlier reboot/power event.
   - **Risk:** seconds-to-minutes full-WAN blip (v6 re-negotiates too);
     zero configuration change; self-reverting (session re-establishes).
2. **If v4 stays dead after reconnect:** ISP-side CGNAT/BNG outage — escalate
   to the carrier with this evidence (session up, gw 100.64.80.1 silent,
   v6 transit healthy).
3. **Interim mitigation (per-device, optional, reversible):** configure a
   reachable resolver (e.g. IPv6 DNS or DoH) on affected clients so resolution
   works while v4 is down.
4. **Follow-up after restoration:** verify router mgmt-plane (httpd/ICMP)
   returns; if not, request an authorized maintenance reboot.

## Changes performed

**NONE.** Read-only diagnostics exclusively (telnet `show` commands, sweeps,
protocol-split probes). No reboot, no service restarts, no config writes, no
Detectic activity on the router.

---

## Addendum (15:58–16:05) — WAN counter experiment (decisive, read-only)

Method: read vendor counters (`wan show connection info` → X_TP_BytesSent /
X_TP_BytesReceived) around controlled traffic bursts from 192.168.0.27.

```
delta baseline (10s idle) : sent +13,718   rx +3,388
delta v6 burst (control)  : sent +9,379    rx +60,945
delta idle (8s)           : sent +3,237    rx +713
delta V4 BURST            : sent +38,024   rx +6,011   <— decisive
delta idle (8s)           : sent +2,312    rx +1,020
```

Interpretation:
- LAN→WAN IPv4 packets ARE forwarded and egress ppp0 (sent counter jumps).
- Zero return traffic beyond keepalive noise ⇒ upstream black-hole.
- Forwarding (E), NAT-emission (F), firewall LAN→WAN (G): all functionally
  PASS traffic out; not the failure point.
- Even hop-2 never answers (tracepath -4: silence after local router; no
  TTL-exceeded from 100.64.80.1) ⇒ CGNAT/LNS not returning on this session.

FINAL CLASSIFICATION: **D — WAN IPv4 upstream unreachable**
(possible H vendor/ISP WAN service malfunction upstream; indistinguishable
from here).

NO NETWORK CHANGES PERFORMED.

---

## RECOVERY ATTEMPT LOG (16:10) — AUTHORIZED: reconnect pppoe_0_0 ONLY

Pre-check recorded (read-only):
  connStatusV4=Connected connIPv4=100.64.80.76 gw=100.64.80.1
  connStatusV6=Connected v6=2804:5020:10:0:3c6a:d24a:8a5f:abc2
  uptime=18611s sent=82,222,559 rx=98,469,879 trigger=AlwaysOn echo=30s

Vendor disconnect/connect mechanism availability:
1. telnet CLI 'wan' verbs: add/set/delete/show ONLY.
   Probed 'wan connect' / 'wan disconnect' -> "Command not found".
   NO runtime reconnect verb exists in this CLI build.
   ('wan set/delete/add service' would MODIFY config -> forbidden, not used.)
2. GTPR CGI action API (ACT_*): UNREACHABLE.
   - IPv4 httpd 80/443: closed (pre-existing).
   - IPv6 CGI getGDPRParm: 406 on global addr, link-local%zone, HTTP and HTTPS;
     header-hypothesis (Host/Origin/Referer=192.168.0.1) tested and DISPROVEN.
3. SSH :22 closed. No other management plane available.

Firmware rootfs contains the exact vendor action names (for future use once
mgmt HTTP returns): ACT_PPP_CONN / ACT_PPP_DISCONN (web UI Connect/Disconnect).

DECISION PER AUTHORIZATION TERMS:
"cannot be identified with certainty ... STOP and report" →
RECONNECT **NOT EXECUTED**. Zero-risk rule honored over improvisation with
config-mutating verbs (set/delete/add) or non-vendor workarounds.

Incident classification: ISP/CGNAT/BNG upstream IPv4 data-plane failure
(evidence package above ready for carrier escalation).

NO NETWORK CHANGES PERFORMED.

---

## RECOVERY EXECUTED (16:33–16:35) — AUTHORIZED PPPoE RECONNECT

Mechanism used (Phase 2 #3): existing Detectic GTPR client invoking the
vendor web-UI action byte-exactly:

    POST /cgi_gdpr?9  {"data":{"stack":"1,0,0,0,0,0","pstack":"0,0,0,0,0,0"},
                       "operation":"op","oid":"ACT_PPP_DISCONN"}

(stack value read live from DEV2_ADT_WAN via `gl`; payload shape taken from
web/js/gdprProxy.js dutPrefilter which always adds pstack and stringifies.)

Execution notes:
- Client-side request timed out (server holds the response while LCP tears
  down against the half-dead upstream). The action DID execute server-side.
- AlwaysOn trigger auto-redialed immediately (no ACT_PPP_CONN needed).
- During redial the LAN lost v6 RAs briefly; our test PC's stale default
  route expired (expected, self-healing).

RESULT (16:34:56):
    NEW IPv4   : 100.64.110.130  (was 100.64.80.76 → fresh CGNAT binding)
    uptime     : fresh session, PPPLastConnError=ERROR_NONE
    curl -4 google : HTTP 200 (0.15s)      [was timeout]
    curl -4 1.1.1.1: HTTP 301 (0.06s)      [was timeout]
    IPv4 DNS @172.20.20.20: resolving      [was dead]
    IPv6 WAN   : Connected (new addr); LAN PD re-delegation in progress
                 (old /64 deprecated; clients fall back to working IPv4)

ROOT CAUSE CONFIRMED: stale CGNAT binding on the long-lived PPPoE session.
Forcing a new session restored the IPv4 data plane instantly.

Stability window monitoring started 16:39 (45 min, 120 s cadence):
samples show g4=200 cf4=301 continuously, counters rising both directions,
3 Wi-Fi clients up (.20/.21/.22 = realme-9i).

NO configuration values changed. NO reboot. WLAN/DHCP/NAT/firewall untouched.
