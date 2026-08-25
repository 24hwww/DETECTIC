# M4.4 Phase F — Wi-Fi Capability Discovery

## Date
2026-08-23

## Objective
Inventory all Wi-Fi interfaces, tools, and data sources on the EX520V. Determine
exactly what Wi-Fi observations are available for the Detectic sensor prototype.

## Wi-Fi Hardware

### Chipset
- **MediaTek MT7981B** (also known as MT7981B / Filogic 820)
- Driver version: 7.6.6.1
- Firmware: 0x8a00
- Hardware version: 0x2080000
- Chip ID: 0x7981

### Capabilities
- **Wi-Fi 6 (802.11ax)** — confirmed via `iwconfig` (IEEE 802.11ax)
- **Dual band**: 2.4 GHz + 5 GHz
- **Multi-BSS**: Up to 7 virtual AP interfaces per band (rai0-6, rax0-6)
- **AP Client mode**: apclii0 (2.4 GHz), apclix0 (5 GHz)

## Interface Inventory

### AP (Access Point) Interfaces
| Interface | Band | ESSID | Mode | Channel | Standard | BSSID |
|-----------|------|-------|------|---------|----------|-------|
| rai0 | 2.4 GHz | REYES | Master | 3 | 802.11ax | 3C:6A:D2:5F:AB:C1 |
| rax0 | 5 GHz | REYES_5G | Master | 40 | 802.11ax | 3C:6A:D2:5F:AB:C3 |
| rai1-6 | 2.4 GHz | (virtual) | Master | — | 802.11ax | (derived) |
| rax1-6 | 5 GHz | (virtual) | Master | — | 802.11ax | (derived) |

### AP Client Interfaces
| Interface | Band | Mode | Channel | Standard |
|-----------|------|------|---------|----------|
| apclii0 | 2.4 GHz | Managed | 40 | 802.11ax |
| apclix0 | 5 GHz | Managed | 3 | 802.11ax |

### Bridge
| Interface | IP | MAC |
|-----------|----|----|
| br0 | 192.168.0.1 | 3C:6A:D2:5F:AB:C1 |

## Available Wi-Fi Data

### 1. Associated Stations (via GTPR API — `DEV2_WIFI_APDEV_ASSOCDEV`)

**This is the primary data source for Detectic.**

Data available per station:
| Field | Example | Notes |
|-------|---------|-------|
| MACAddress | A2:B7:68:FE:7B:60 | Station MAC |
| X_TP_HostName | realme-9i | Device hostname |
| X_TP_IPAddress | 192.168.0.22 | IP address |
| signalStrength | 116 | RSSI (vendor scale, 0-127) |
| X_TP_SignalStrengthLevel | 4 | Signal level (1-5) |
| operatingStandard | n | 802.11 standard (a/b/g/n/ac/ax) |
| lastDataDownlinkRate | 96000 | TX rate (kbps) |
| lastDataUplinkRate | 72000 | RX rate (kbps) |
| noise | 50 | Noise floor (vendor scale) |
| associationTime | 2026-08-23T10:53:03-03:00 | Association timestamp |
| active | 1 | Active flag |
| X_TP_RadioMac | 3C:6A:D2:5F:AB:C1 | AP radio MAC |
| X_TP_BssMac | 3C:6A:D2:5F:AB:C1 | BSS MAC |
| X_TP_ApDeviceMac | 3C:6A:D2:5F:AB:C1 | AP device MAC |
| X_TP_MaxLinkRate | 72000 | Max link rate (kbps) |
| steeringHistoryNumberOfEntries | 0 | Band steering history count |

**Access method**: GTPR `gl` operation on `DEV2_WIFI_APDEV_ASSOCDEV`
**Polling interval**: 30 seconds (configurable)
**Authentication**: User account credentials
**Format**: JSON (structured)

### 2. Host Table (via GTPR API — `DEV2_HOST_ENTRY`)

Additional per-device data:
| Field | Example | Notes |
|-------|---------|-------|
| hostName | moto-g54-5G | Hostname |
| IPAddress | 192.168.0.20 | IP address |
| physAddress | D6:8A:2B:93:62:7A | MAC address |
| interfaceType | Wi-Fi | Connection type |
| X_TP_ClientType | Android | Client OS type |
| addressSource | DHCP | Address source |
| leaseTimeRemaining | 5704 | DHCP lease remaining (seconds) |
| X_TP_LanConnDev | br0 | LAN connection device |
| X_TP_Layer2Interface | Device.WiFi.AccessPoint.1. | L2 interface |
| active | 1 | Active flag |
| X_TP_IPv6Address | 2804:5020:... | IPv6 address |

### 3. Radio Statistics (via `iwpriv stat`)

Per-radio data:
| Field | Example | Notes |
|-------|---------|-------|
| CurrentTemperature | 33 | Chip temperature (Celsius) |
| Tx success | 57924 | Successful TX packets |
| Tx fail count | 3270 | Failed TX packets |
| PER | 5.3% | Packet error rate |
| Rx success | 93775 | Successful RX packets |
| Rx with CRC | 139368 | CRC errors |
| Rssi | -53 -53 -109 -109 | Per-antenna RSSI |
| Last TX Rate | MCS7, BW20, 0.8us GI, HT_MM, LDPC | Last TX modulation |
| Last RX Rate | NSS1_MCS8, BW20, LGI, VHT, BCC | Last RX modulation |

### 4. Site Survey (via `iwpriv get_site_survey`)

Nearby AP scan data:
| Field | Example | Notes |
|-------|---------|-------|
| Channel | 1 | Channel number |
| SSID | Juliana | SSID |
| BSSID | 64:61:40:41:e0:e0 | BSSID |
| Security | WPA2PSK/AES | Security mode |
| Signal(%) | 13 | Signal strength percentage |
| W-Mode | 11b/g/n/ax | Wireless modes |
| ExtCH | NONE | Extension channel |
| NT | In | Network type |
| WPS | YES | WPS support |
| BcnRept | NO | Beacon report |

### 5. `/proc/net/wireless`

Per-interface data:
| Field | Example | Notes |
|-------|---------|-------|
| link | 10 | Link quality (static) |
| level | -256 | Signal level (invalid for AP) |
| noise | -51 | Noise floor (dBm) |

## Wi-Fi Tools Available

| Tool | Path | Notes |
|------|------|-------|
| iwpriv | /usr/sbin/iwpriv → iwconfig | MediaTek private ioctls |
| iwconfig | /usr/sbin/iwconfig | Wireless interface config |
| iwlist | /usr/sbin/iwlist | Wireless list (scan not supported) |
| wlNetlinkTool | /bin/wlNetlinkTool | TP-Link wireless event monitor |

### Tools NOT Available
- `iw` (cfg80211/nl80211 tool) — not installed
- `hostapd_cli` — not installed
- `tcpdump` — not installed
- `wpa_cli` — not installed

## `get_mac_table` Crash Investigation

### Symptom
`iwpriv rai0 get_mac_table` segfaults (exit code 139) on all AP interfaces.

### Investigation
1. **iwpriv binary**: Symlink to `iwconfig` (wireless-tools). Standard
   wireless-tools `iwpriv` does not handle MediaTek's binary response format.
2. **Core dump**: Generated at `/var/core-iwpriv` (2832 bytes).
3. **Driver response**: The MediaTek driver returns a binary structure (not a
   null-terminated string) for `get_mac_table`. The wireless-tools `iwpriv`
   attempts to print this as a string, causing a segfault.
4. **Argument format**: Tried with argument (`get_mac_table 0`) — still crashes.
   The crash is in the response handling, not the request.
5. **Router stability**: Router remains fully operational after the crash. The
   crash is isolated to the `iwpriv` process.

### Root Cause
Incompatibility between wireless-tools `iwpriv` and MediaTek MT7981 driver's
binary response format for `get_mac_table` ioctl (0x8BEF).

### Alternative
The GTPR API (`DEV2_WIFI_APDEV_ASSOCDEV` OID via `gl` operation) provides the
same station data (MAC, RSSI, rates, etc.) in a structured JSON format without
crashing. This is the recommended mechanism.

## Data Available Summary

### Associated Stations (Connected Devices)
| Data | Available | Source | Mechanism |
|------|-----------|--------|-----------|
| MAC address | YES | GTPR API | `DEV2_WIFI_APDEV_ASSOCDEV` |
| RSSI / signal strength | YES | GTPR API | `signalStrength` field |
| Signal level (1-5) | YES | GTPR API | `X_TP_SignalStrengthLevel` |
| TX rate | YES | GTPR API | `lastDataDownlinkRate` |
| RX rate | YES | GTPR API | `lastDataUplinkRate` |
| Max link rate | YES | GTPR API | `X_TP_MaxLinkRate` |
| Noise floor | YES | GTPR API | `noise` field |
| Operating standard | YES | GTPR API | `operatingStandard` |
| Hostname | YES | GTPR API | `X_TP_HostName` |
| IP address | YES | GTPR API | `X_TP_IPAddress` |
| Association time | YES | GTPR API | `associationTime` |
| Active status | YES | GTPR API | `active` |
| Client type | YES | GTPR API | `DEV2_HOST_ENTRY.X_TP_ClientType` |
| Interface type | YES | GTPR API | `DEV2_HOST_ENTRY.interfaceType` |
| DHCP lease time | YES | GTPR API | `DEV2_HOST_ENTRY.leaseTimeRemaining` |
| IPv6 address | YES | GTPR API | `DEV2_HOST_ENTRY.X_TP_IPv6Address` |

### Radio/Environment
| Data | Available | Source | Mechanism |
|------|-----------|--------|-----------|
| Temperature | YES | iwpriv | `stat` ioctl |
| TX/RX packet counts | YES | iwpriv | `stat` ioctl |
| PER | YES | iwpriv | `stat` ioctl |
| Per-antenna RSSI | YES | iwpriv | `stat` ioctl |
| Last TX/RX modulation | YES | iwpriv | `stat` ioctl |
| Noise floor (dBm) | YES | procfs | `/proc/net/wireless` |
| Channel | YES | iwconfig | `iwconfig rai0` |

### Nearby APs (Site Survey)
| Data | Available | Source | Mechanism |
|------|-----------|--------|-----------|
| SSID | YES | iwpriv | `get_site_survey` |
| BSSID | YES | iwpriv | `get_site_survey` |
| Channel | YES | iwpriv | `get_site_survey` |
| Signal (%) | YES | iwpriv | `get_site_survey` |
| Security | YES | iwpriv | `get_site_survey` |
| W-Mode | YES | iwpriv | `get_site_survey` |
| WPS | YES | iwpriv | `get_site_survey` |

### NOT Available
| Data | Status | Notes |
|------|--------|-------|
| Unassociated station metrics | NOT AVAILABLE | `DEV2_WIFI_DE_UNASSOCSTA` returns 9003 |
| Channel utilization | NOT AVAILABLE | Not exposed via any interface |
| Connection/disconnection events (real-time) | NOT AVAILABLE | `wlNetlinkTool` receives events but does not expose them via API |
| Probe request data | NOT AVAILABLE | Not exposed |
| Per-packet RSSI | NOT AVAILABLE | Not exposed |

## Frequency Bands

### 2.4 GHz (rai0)
- Channel: 3
- ESSID: REYES
- Standard: 802.11ax
- Noise floor: -51 dBm
- Temperature: 33°C
- Active stations: 3

### 5 GHz (rax0)
- Channel: 40
- ESSID: REYES_5G
- Standard: 802.11ax
- Noise floor: -51 dBm
- Temperature: 35°C
- Active stations: 0 (all stations on 2.4 GHz during test)

## Conclusion

The EX520V provides comprehensive Wi-Fi observation data through the GTPR API:

1. **Associated stations**: Full telemetry (MAC, RSSI, rates, noise, standard,
   hostname, IP, association time) via `DEV2_WIFI_APDEV_ASSOCDEV`
2. **Host table**: Additional device info (client type, interface type, DHCP
   lease, IPv6) via `DEV2_HOST_ENTRY`
3. **Radio stats**: Temperature, packet counts, PER, per-antenna RSSI, modulation
   via `iwpriv stat`
4. **Site survey**: Nearby AP scan via `iwpriv get_site_survey`

The `get_mac_table` ioctl crashes due to wireless-tools incompatibility but is
not needed — the GTPR API provides the same data in a structured format.

**Wi-Fi Observation Classification: PROVEN** (for associated stations)
**Nearby Device Detection Classification: PROVEN** (for site survey/AP scan)
**Unassociated Station Metrics Classification: NOT AVAILABLE**
