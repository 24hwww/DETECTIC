# M4.4 Phase D — Detectic GTPR/GDPR Runtime Validation

## Date
2026-08-23

## Objective
Determine whether Detectic can communicate with the local GTPR/GDPR service from
inside the router.

## Method
Detectic binary deployed to `/var/tmp/detectic` on the router. Commands executed
via the legitimate administrative shell (port 8888).

## Key Finding: Localhost vs LAN IP

The router's `httpd` returns **406 Not Acceptable** for requests to `127.0.0.1`,
but accepts requests to `192.168.0.1` (the LAN IP).

| URL | Method | Result |
|-----|--------|--------|
| `http://127.0.0.1/cgi/getGDPRParm` | GET | 406 |
| `http://127.0.0.1/cgi/getGDPRParm` | POST | 406 |
| `http://192.168.0.1/cgi/getGDPRParm` | GET | 406 |
| `http://192.168.0.1/cgi/getGDPRParm` | POST | **200** |

The httpd checks the destination address and rejects loopback requests to the
GDPR endpoint. Detectic must use `--url http://192.168.0.1` when running inside
the router.

## Test Results

### `detectic --help`
```
/var/tmp/detectic --help
```
**Result**: Exit code 0. Full help text displayed. No errors.

### `detectic query DEV2_DEV_INFO`
```
/var/tmp/detectic --url http://192.168.0.1 --password <REDACTED> --secret test-secret query DEV2_DEV_INFO
```
**Result**: Exit code 0. Full device info received:
- manufacturer: TP-Link
- modelName: EX520
- description: AX3000 Dual Band Wi-Fi 6 Router
- X_TP_MACAddress: 3C:6A:D2:5F:AB:C1
- X_TP_SerialNumber: R252036001104

### `detectic map`
```
/var/tmp/detectic --url http://192.168.0.1 --password <REDACTED> --secret test-secret map
```
**Result**: Exit code 0. Complete network map received with 4 devices:

#### Wi-Fi Connected Devices (3)
| Hostname | IP | MAC | RSSI | Standard | TX Rate | RX Rate | Noise |
|----------|----|----|------|----------|---------|---------|-------|
| realme-9i | 192.168.0.22 | A2:B7:68:FE:7B:60 | 116 | n | 96000 | 72000 | 50 |
| moto-g42 | 192.168.0.21 | A6:9D:50:62:05:E6 | 104 | n | 39000 | 1000 | 50 |
| moto-g54-5G | 192.168.0.20 | D6:8A:2B:93:62:7A | 120 | n | 65000 | 78000 | 50 |

#### Ethernet Connected Devices (1)
| Hostname | IP | MAC | Interface |
|----------|----|----|-----------|
| Unknown | 192.168.0.27 | 8C:B0:E9:C2:8C:06 | Ethernet |

### `detectic sensor` (continuous mode)
```
/var/tmp/detectic --url http://192.168.0.1 --password <REDACTED> --secret test-secret sensor
```
**Result**: Sensor started successfully. Polled every 30 seconds. Buffer file
`/tmp/detectic_buffer.jsonl` created with pseudonymized observations:

```json
{
  "sensor_id": "home-001",
  "id": "631de40e...",
  "captured_at": 1787494701,
  "devices": [
    {
      "pseudonym": "040305bb...",
      "rssi": 116,
      "standard": "n",
      "source": "wifi",
      "radio_mac": "8f4ebec8..."
    },
    ...
  ]
}
```

## Discovered Endpoints/OIDs

### Reachable & Readable (via `gl` operation)
| OID | Data | Notes |
|-----|------|-------|
| `DEV2_DEV_INFO` | Device info | manufacturer, model, serial, MAC |
| `DEV2_WIFI_APDEV_ASSOCDEV` | **Associated stations** | MAC, RSSI, rates, noise, standard, hostname, IP |
| `DEV2_HOST_ENTRY` | Host table | IP, MAC, hostname, interface type, lease time |
| `DEV2_DHCPV4_CLIENT` | DHCP client | WAN DHCP status |
| `DEV2_USER_CFG` | User config | Account settings (masked passwords) |
| `DEV2_TELNET_CFG` | Telnet config | Telnet enable state |
| `DEV2_LIFEMOTE_AGENT` | Lifemote agent | Enable state, URL |
| `DEV2_WIFI_STEERINGSTATS` | Band steering | Steering statistics |

### Reachable but Not Applicable (errorcode 9003)
| OID | Notes |
|-----|-------|
| `DEV2_WIFI_MACTABLE` | Not populated on this firmware |
| `DEV2_WIFI_DE_UNASSOCSTA` | Not populated |
| `DEV2_WIFI_APDEV_ETHASSOCDEV` | Not populated |

### Not Accessible (errorcode 9804)
| OID | Notes |
|-----|-------|
| `DEV2_WIFI_APDEV_NEIGHBORSIG` | Requires specific conditions |

### Writable (via `so` operation)
| OID | Fields | Notes |
|-----|--------|-------|
| `DEV2_USER_CFG` | pwdSign, adminTempLock, cliTempLock, etc. | User account can write some fields |
| `DEV2_TELNET_CFG` | telnetLocalEnabled | Enable/disable Telnet |
| `DEV2_LIFEMOTE_AGENT` | enable, URL | Enable/disable Lifemote agent |

## GTPR API Flow (from inside router)

1. **POST** `http://192.168.0.1/cgi/getGDPRParm` → RSA parameters (nn, ee, seq)
2. **POST** `http://192.168.0.1/cgi/login` → Login with AES-encrypted credentials
3. **GET** `http://192.168.0.1/cgi/getTokenc` → Token ID for session
4. **POST** `http://192.168.0.1/cgi_gdpr?N` → Encrypted GTPR operations (go, gl, so, op)

All requests include:
- `Referer: http://192.168.0.1/`
- `Origin: http://192.168.0.1`
- `Accept: */*`
- `X-Requested-With: XMLHttpRequest` (for POST operations)

## Conclusion

**Detectic can successfully communicate with the local GTPR/GDPR service from
inside the router** using `http://192.168.0.1` as the URL. The `map` command
returns complete Wi-Fi station data including MAC addresses, RSSI, TX/RX rates,
noise, operating standard, hostnames, and IP addresses. The `sensor` command
runs continuously, polling every 30 seconds and writing pseudonymized
observations to a local buffer file.

**Classification: PROVEN**
