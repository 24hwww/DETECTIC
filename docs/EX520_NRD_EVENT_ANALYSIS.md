# EX520 nrd Event Infrastructure — Final Analysis

## Executive Summary

This document records the complete investigation of whether DETECTIC can obtain
real-time Wi-Fi events (probe requests, associations, RSSI measurements) from
the TP-Link EX520's internal `nrd` process without modifying the firmware.

**Verdict: Passive event consumption is NOT possible on the stock EX520 firmware.
The only viable Wi-Fi data source is GTPR polling of `DEV2_WIFI_APDEV_ASSOCDEV`.**

---

## 1. nrd Architecture (Static Analysis)

### 1.1 Binary
- **Path**: `/bin/nrd`
- **Format**: ELF 64-bit LSB, ARM aarch64, dynamically linked, stripped
- **Libraries**: libcrypto, libjson-c, libblobmsg_json, libubox, libcJSON,
  libos, libhyfi-bridge, libgdpr, libcutil, libgcc_s, libc
- **Process**: `nrd -d -C /var/tmp/nrd.conf` (PID 2743, user admin, ~4MB RSS)

### 1.2 Event Reception Paths

nrd uses TWO independent event reception paths:

#### A. Netlink (MediaTek vendor protocol 21)
- **Socket**: `socket(AF_NETLINK, SOCK_RAW, 21)` — MediaTek vendor netlink
- **Bind**: `nl_pid = getpid()`, `nl_groups = 0` (UNICAST)
- **No setsockopt** to join multicast groups
- **Confirmed via /proc/net/netlink**:
  ```
  000000007829cbbc 21 0          00000000   (kernel)
  000000009e05eec2 21 2743       00000000   (nrd, PID 2743)
  ```
- **Event types dispatched** (in `wlanifBSteerEventsMsgRx` at 0x44baf0):
  | Type | Name | Handler |
  |------|------|---------|
  | 2 | PROBE_REQ | `wlanifBSteerEventsHandleProbeReqInd` (0x44a0d4) |
  | 3 | ASSOC | `wlanifBSteerEventsHandleNodeAssociatedInd` (0x44a260) |
  | 4 | AUTH_FAIL | `wlanifBSteerEventsHandleTxAuthFailInd` (0x44a628) |
  | 6 | RSSI_XING | `wlanifBSteerEventsHandleRSSIXingInd` (0x44a784) |
  | 7 | RSSI_MEAS | `wlanifBSteerEventsHandleRSSIMeasurementInd` (0x44a99c) |
  | 9 | WNM | (WNM event handler) |
  | 0x21 | BEACON_RPT | (Beacon report handler) |
  | 0x24 | RRM_RPT | (RRM report handler) |

- **Event payload format** (for probe/assoc events):
  - Netlink header: 16 bytes
  - Payload offset 0x00: event type (u32)
  - Payload offset 0x04: event subtype (u32)
  - Payload offset 0x08: MAC address (6 bytes)
  - Payload offset 0x0e: RSSI (u8)

#### B. libos Unix-domain IPC (control only)
- **Socket**: `msg_srvInit(45, ...)` → binds to `/var/tmp/45`
- **Type**: AF_UNIX, SOCK_DGRAM
- **Max message**: 4104 bytes (0x1008)
- **Handler table** at 0x478488 (2 entries, 64 bytes each):
  | msg_type | Name | Handler |
  |----------|------|---------|
  | 0x13d1 (5073) | CMSG_AI_ROAMING_INFO_RECV | 0x4058e4 |
  | 0x13e6 (5094) | CMSG_EASYMESH_MAP_RELOAD_NRD | 0x405a28 |
- **nrd sends to**: msgType 42 (`/var/tmp/42`) and 43 (`/var/tmp/43`)
  for steering responses via `msg_connCliAndSend`
- **No data query mechanism** — only control notifications

### 1.3 nrd Configuration (/var/tmp/nrd.conf)
```ini
[WLANIF]
WlanInterfaces=wlan0:rai0,wlan5:rax0

[STADB]
IncludeOutOfNetwork=1
PollFrequency=1
OutOfNetworkMaxAge=300
InNetworkMaxAge=2592000

[TRIGGERMON]
RunOnCAP=1
```

---

## 2. Live Validation Results

### 2.1 Netlink Probe (Phase 5)
- **Method**: Compiled static aarch64-musl Rust binary, deployed via
  Lifemote/Phoenix mechanism, created own AF_NETLINK socket on protocol 21
- **Result**: `events=0` — ZERO events received in 30 seconds
- **Conclusion**: The MediaTek driver UNICASTS netlink events to nrd's PID
  only. A second socket on the same protocol receives nothing.

### 2.2 Environment Probe (Phase 6)
- **iwconfig/iwlist/iwpriv**: ALL RETURNED EMPTY on all interfaces
  (rai0, rax0, apclii0, apclix0). The MediaTek driver does not support
  standard WEXT ioctls on this firmware.
- **/proc/net/wireless**: Shows interfaces rai0-rai6, rax0-rax6, apclii0,
  apclix0. All show link level -256 (no useful signal data).
- **/proc/net/arp**: Shows 4 connected devices with MAC addresses.
  No signal strength or association events.
- **/tmp/ai_roaming/ar_pat/staInfo**: Does not exist (AiRoamingEnable=0).
- **/var/tmp/clientLinkPreferInfo**: Does not exist.
- **nrd process FDs**: 7 socket descriptors (netlink, IPC, plus internal).

### 2.3 IPC Handler Table (Phase 7)
- nrd's IPC socket (`/var/tmp/45`) only handles 2 control messages:
  AI roaming info and EasyMesh MAP reload.
- No query/response mechanism for station data.
- Sending arbitrary messages to nrd's IPC would not produce useful data.

---

## 3. Approaches Investigated and Rejected

| Approach | Status | Reason |
|----------|--------|--------|
| Passive netlink (protocol 21) | REJECTED | Driver unicasts to nrd PID; 0 events received |
| nrd IPC query (/var/tmp/45) | REJECTED | Only 2 control handlers; no data query |
| iwlist/iwpriv polling | REJECTED | All commands return empty on MediaTek driver |
| /proc/net/wireless | REJECTED | All interfaces show link -256 (no signal data) |
| /tmp/ai_roaming/ar_pat/staInfo | REJECTED | File doesn't exist (AiRoaming disabled) |
| /proc/net/arp | LIMITED | Provides MACs only, no signal/events |
| Shared memory (shm) | N/A | nrd does not use shared memory |
| nl80211/genetlink | N/A | nrd uses vendor netlink, not nl80211 |

---

## 4. Viable Data Source: GTPR Polling

### 4.1 OID: DEV2_WIFI_APDEV_ASSOCDEV
The GTPR API provides associated station data via the `user` account:

```json
{
  "X_TP_HostName": "realme-9i",
  "X_TP_IPAddress": "192.168.0.21",
  "X_TP_RadioMac": "3C:6A:D2:5F:AB:C1",
  "X_TP_ApDeviceMac": "3C:6A:D2:5F:AB:C1",
  "X_TP_BssMac": "3C:6A:D2:5F:AB:C1",
  "MACAddress": "A2:B7:68:FE:7B:60",
  "operatingStandard": "n",
  "active": "1",
  "associationTime": "2026-08-26T15:40:25-03:00",
  "lastDataDownlinkRate": "72000",
  "lastDataUplinkRate": "39000",
  "X_TP_SignalStrengthLevel": "3",
  "signalStrength": "84",
  "noise": "50",
  "X_TP_MaxLinkRate": "72000",
  "stack": "1,1,2,1,0,0"
}
```

### 4.2 Available Fields
| Field | Detectic Use |
|-------|-------------|
| MACAddress | Device identity |
| X_TP_HostName | Device label |
| X_TP_IPAddress | Network address |
| associationTime | First seen / session start |
| signalStrength (0-100) | Signal quality |
| X_TP_SignalStrengthLevel (1-5) | Coarse signal |
| lastDataDownlinkRate | Activity indicator |
| lastDataUplinkRate | Activity indicator |
| active | Presence flag |
| operatingStandard | PHY capability |
| noise | Noise floor |

### 4.3 Limitations
- Only ASSOCIATED devices are visible (no probe requests from unassociated devices)
- Polling interval limited by GTPR authentication overhead (~2-3s per query)
- No real-time event notifications (must poll)
- MAC randomization not detectable at this layer

---

## 5. DETECTIC Integration Design

### 5.1 Recommended Architecture

```
EX520 (stock firmware)
  |
  | GTPR over IPv6 link-local HTTP
  | OID: DEV2_WIFI_APDEV_ASSOCDEV
  | Poll interval: 30-60s
  v
Host-side Detectic Sensor (existing)
  |
  | Normalizes, aggregates, pseudonymizes
  v
Detectic Backend (HTTPS)
```

### 5.2 Key Design Decisions

1. **Keep the sensor on the host**, not on the router.
   The EX520 has no viable local event source. The GTPR polling approach
   (Path 1 in AGENTS.md) is the proven, working data source.

2. **Poll DEV2_WIFI_APDEV_ASSOCDEV every 30-60 seconds.**
   This provides:
   - Device presence (associated = present)
   - Signal strength trends (signalStrength over time)
   - Activity patterns (data rates, active flag)
   - Association events (new MAC = arrival, missing MAC = departure)

3. **Augment with /proc/net/arp via Phoenix.**
   When higher-frequency data is needed, deploy a Phoenix script that
   reads /proc/net/arp every 5-10s and posts changes to the host.
   This provides faster presence detection than GTPR alone.

4. **No firmware modification required.**
   The stock firmware's GTPR API + Lifemote/Phoenix mechanism is sufficient
   for the core Detectic use case (associated device presence/absence).

### 5.3 What We Cannot Do (Without Firmware Modification)

- Detect unassociated devices (probe requests)
- Get real-time association/disassociation events (sub-second)
- Get per-station RSSI measurements on demand
- Monitor Wi-Fi channel activity (beacons, probe responses)
- Detect MAC randomization at the Wi-Fi layer

### 5.4 Future Enhancement Paths

If probe request detection becomes necessary:

1. **USB Wi-Fi adapter in monitor mode** — Attach a USB adapter that
   supports monitor mode (e.g., RTL8812AU) and use it to capture
   raw 802.11 frames. Requires USB host support on the EX520.

2. **Firmware modification** — Modify the firmware to add a netlink
   multicast group or a raw packet socket that forwards probe requests
   to a DETECTIC listener. Requires signed firmware bypass.

3. **Driver ioctl hook** — Use the MediaTek driver's private ioctls
   (via iwpriv) to request probe request data. Requires reverse
   engineering the MediaTek driver's ioctl interface.

---

## 6. Conclusion

The EX520's stock firmware provides a single viable Wi-Fi data source:
GTPR polling of `DEV2_WIFI_APDEV_ASSOCDEV`. This provides associated
device presence, signal strength, and activity data — sufficient for
DETECTIC's core presence/absence sensing use case.

Real-time event consumption (probe requests, RSSI crossings, association
events) is NOT possible without firmware modification, because:
1. The MediaTek driver unicasts netlink events to nrd's PID only
2. nrd's IPC socket has no data query mechanism
3. Standard wireless tools (iwlist/iwpriv) are non-functional on this driver

The existing host-based GTPR polling approach (Path 1 in AGENTS.md)
remains the recommended architecture for DETECTIC on the EX520.
