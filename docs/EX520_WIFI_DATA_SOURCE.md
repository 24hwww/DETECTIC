# EX520 Wi-Fi Data Sources

## Canonical source

```text
GTPR  DEV2_WIFI_APDEV_ASSOCDEV
```

This OID returns the authoritative list of associated stations on the EX520.
It is the **only** proven, supported, stock-firmware data source for:

* MAC address (then pseudonymized)
* Hostname
* IP address
* Radio MAC / BSS MAC / AP device MAC
* Operating standard (11b/g/n/ax)
* Active flag
* Association time
* Last downlink/uplink rate
* Signal strength (dBm / level)
* Noise

The GTPR client (`src/transport.rs`) authenticates to the router, queries this
OID every `DETECTIC_INTERVAL` seconds (default 30), and the collector
(`src/collector.rs`) normalizes the response into `src/model.rs` `Device`
objects.

## Presence engine

`src/presence.rs` and `src/temporal.rs` maintain per-device state:

```text
UNKNOWN → PRESENT → ABSENT
```

and emit canonical events:

```text
sensor.started
sensor.ready
sensor.healthy
sensor.unhealthy
sensor.stopped
sensor.gtp_poll_success
sensor.gtp_poll_failure
device.first_seen
device.arrived
device.reappeared
device.updated
device.departed
device.signal_changed
device.activity_changed
sensor.backend_connected
sensor.backend_disconnected
```

## ARP fast-path (supplementary only)

`src/arp.rs` reads `/proc/net/arp` every `DETECTIC_ARP_INTERVAL` seconds
(default 10).  It provides a higher-frequency hint that an IP is still in the
bridge neighbor cache, which can accelerate re-detection of already-known
devices.

**ARP is NOT authoritative for Wi-Fi association.**  It is used only as a
presence hint and never creates a new device by itself.

ARP does NOT provide:

* RSSI
* Signal level
* Operating standard
* Rates
* Association time

## Rejected data sources

The following were investigated and rejected for the stock EX520 firmware:

| Source | Status | Reason |
|--------|--------|--------|
| MediaTek netlink protocol 21 | **NOT POSSIBLE** | Driver unicasts to `nrd`; passive sockets receive nothing. |
| `/var/tmp/45` nrd IPC socket | **NOT POSSIBLE** | Only control messages, no data query. |
| `iwlist` / `iwpriv` / `iwconfig` | **NOT FUNCTIONAL** | Returns empty output on all interfaces. |
| `/proc/net/wireless` | **NOT FUNCTIONAL** | Link level is `-256` for all interfaces. |

## Data semantics

DETECTIC CAN reliably detect:

* associated Wi-Fi devices
* presence transitions
* signal strength trends
* coarse signal level
* activity metadata
* hostname/IP metadata
* association time
* device count

DETECTIC CANNOT claim:

* probe requests
* unassociated clients
* raw 802.11 frame visibility
* sub-second association events
* Wi-Fi channel scanning
* packet capture
* MAC randomization detection at RF level
