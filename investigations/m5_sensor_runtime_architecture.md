# M5 — Sensor Runtime Architecture

## Date
2026-08-23

## Objective
Document the production sensor runtime architecture implemented in Milestone M5.
This is the design that turns Detectic from a CLI tool into a continuously-running
sensor process suitable for deployment on the TP-Link EX520V.

## Overview

The M5 runtime is organized into clear layers, each with a single responsibility:

```
┌─────────────────────────────────────────────────────┐
│                    main.rs (CLI)                     │
│         detectic sensor / status / map               │
└──────────────────────┬──────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────┐
│              runtime.rs (SensorRuntime)              │
│  • Polling loop (configurable interval)              │
│  • Signal handling (SIGTERM/SIGINT)                  │
│  • Retry/backoff for GTPR failures                   │
│  • Change detection (diff_snapshots)                 │
│  • Backend dispatch                                  │
└──────┬──────────┬──────────────┬────────────────────┘
       │          │              │
┌──────▼────┐ ┌───▼────┐ ┌──────▼──────────────────┐
│ collector │ │ config │ │ backend (BackendTransport)│
│ (OIDs →   │ │ (env)  │ │  • NullBackend           │
│  NetworkMap)│ │       │ │  • LocalSpoolBackend     │
└──────┬────┘ └────────┘ │  • HttpBackend (M6)      │
       │                 └──────────────────────────┘
┌──────▼──────────────────────────────────────────────┐
│              transport.rs (GtprClient)               │
│  • GDPR encrypted HTTP (RSA + AES-128-CBC)           │
│  • Session management (JSESSIONID, TokenID)          │
└──────────────────────────────────────────────────────┘
```

## New Modules (M5)

### `config.rs` — SensorConfig
All runtime configuration is sourced from environment variables with sensible
defaults. No credentials are hardcoded.

| Env Var | Default | Description |
|---------|---------|-------------|
| `DETECTIC_URL` | `http://192.168.0.1` | Router GTPR endpoint |
| `DETECTIC_USER` | `user` | Router username |
| `DETECTIC_PASSWORD` | (required) | Router password |
| `DETECTIC_SECRET` | (required) | Per-sensor HMAC secret |
| `DETECTIC_SENSOR_ID` | `home-001` | Sensor identifier |
| `DETECTIC_INTERVAL` | `30` | Polling interval (seconds) |
| `DETECTIC_BACKEND_URL` | (none) | Backend upload URL |
| `DETECTIC_BUFFER` | `/tmp/detectic_buffer.jsonl` | Offline spool path |
| `DETECTIC_BUFFER_MAX` | `262144` | Spool max size (bytes) |
| `DETECTIC_LOG_LEVEL` | `info` | Log level (error/warn/info/debug) |
| `DETECTIC_LOG_MACS` | `false` | If true, log raw MACs (debugging only) |
| `DETECTIC_SITE_SURVEY` | `false` | Enable nearby-AP scan |
| `DETECTIC_RADIO_STATS` | `false` | Enable per-radio statistics |

A minimal key=value config file is also supported (`SensorConfig::from_file`).
Env vars always override file values.

### `logging.rs` — Structured Logging
Lightweight structured logging with four levels (ERROR/WARN/INFO/DEBUG).
Never logs passwords, API secrets, or session tokens. MAC addresses are
redacted by default (only OUI prefix shown); full MACs require
`DETECTIC_LOG_MACS=1`.

### `snapshot.rs` — SensorSnapshot
The `SensorSnapshot` is the stable internal representation of a single polling
instant. It is richer than `NetworkMap` (which is just the merged OID data)
because it also carries router identity, uptime, radio statistics, and
optional nearby-AP summary.

`diff_snapshots()` computes the difference between two snapshots for change
detection. This is polling-derived, NOT real-time kernel events.

### `backend.rs` — BackendTransport
A trait-based backend abstraction with three implementations:
- **NullBackend**: discards everything (testing / local-only mode)
- **LocalSpoolBackend**: writes pseudonymized payloads to a bounded JSONL file
- **HttpBackend**: HTTP POST with HMAC auth and retry (skeleton for M6)

The `SpoolBackend` wrapper adds offline buffering to any backend.

### `runtime.rs` — SensorRuntime
The main polling loop. Owns:
- Configuration (`SensorConfig`)
- Polling state (count, errors, last successful poll)
- Previous snapshot (for change detection)
- Backend transport

Key behaviors:
- **Signal handling**: SIGTERM/SIGINT trigger graceful shutdown
- **Retry/backoff**: GTPR failures use bounded exponential backoff (1s → 60s)
- **Change detection**: only emits events when devices join/leave/change
- **Spool drain**: attempts to re-send buffered entries at the start of each cycle
- **Resource monitoring**: reads VmRSS from `/proc/self/status`

## Extended Device Model (M5-C)

The `Device` struct was extended with fields from `DEV2_WIFI_APDEV_ASSOCDEV`
and `DEV2_HOST_ENTRY`:

| Field | Source | Example |
|-------|--------|---------|
| `tx_rate` | ASSOCDEV `lastDataDownlinkRate` | 57000 (kbps) |
| `rx_rate` | ASSOCDEV `lastDataUplinkRate` | 58000 (kbps) |
| `noise` | ASSOCDEV `noise` | 50 |
| `signal_level` | ASSOCDEV `X_TP_SignalStrengthLevel` | 3 (1-5) |
| `max_link_rate` | ASSOCDEV `X_TP_MaxLinkRate` | 72000 (kbps) |
| `interface` | HOST `X_TP_Layer2Interface` | `Device.WiFi.AccessPoint.1.` |
| `ipv6` | HOST `X_TP_IPv6Address` | `2804:5020:...` |
| `client_type` | HOST `X_TP_ClientType` | `Android` |
| `active` | ASSOCDEV/HOST `active` | `1` |

All new fields are `Option<T>` and default to `None` — the sensor never
invents values that the firmware does not provide.

## Change Detection (M5-D)

The `changed_fields()` function in `events.rs` was extended to detect changes
in all new fields: `tx_rate`, `rx_rate`, `noise`, `signal_level`,
`max_link_rate`, `interface`, `ipv6`, `client_type`, `active`.

Events are emitted only when a field actually changes between snapshots.
The first snapshot generates `DeviceJoined` events for all observed devices.

## Resource Protection (M5-H)

| Limit | Default | Purpose |
|-------|---------|---------|
| `max_stations` | 256 | Prevents unbounded station vectors |
| `max_nearby_aps` | 512 | Prevents unbounded AP scan results |
| `max_response_body` | 1 MB | Caps HTTP response reading |
| `router_timeout` | 15s | GTPR request timeout |
| `backend_timeout` | 10s | Backend upload timeout |
| `max_poll_retries` | 3 | GTPR retry attempts |
| `max_upload_retries` | 3 | Backend retry attempts |
| `spool_max_bytes` | 256 KB | Bounded offline buffer |

## Build Configurations

### Router build (production)
```bash
cargo build --release --no-default-features --target aarch64-unknown-linux-musl
```
- No SQLite (avoids `libsqlite3-sys` C compilation issues on musl)
- No TLS (keeps binary small; backend TLS is M6)
- Binary size: **1.1 MB** (statically linked, stripped)
- Memory: **~1 MB RSS**, **1 thread**

### Host build (development)
```bash
cargo build --features persist
```
- Includes SQLite persistence (`rusqlite`)
- Includes analytics and reporting commands
- Used for development, testing, and data analysis

## Verified on EX520V (M5-M)

The M5 runtime was smoke-tested on the EX520V on 2026-08-23:

1. **`detectic status`**: Printed full configuration and VmRSS (1004 kB)
2. **`detectic map`**: Collected 7 devices (5 Wi-Fi + 2 Ethernet) with all
   new M5 fields populated (tx_rate, rx_rate, noise, signal_level,
   max_link_rate, interface, ipv6, client_type, active)
3. **`detectic sensor`** (25-second run, 10s interval):
   - First poll: 7 stations, 7 `DeviceJoined` events (pseudonymized)
   - Second poll: 7 stations, 0 events (no changes — correct)
   - Third poll: 7 stations, 0 events (stable)
   - Structured logging at INFO and DEBUG levels
   - Clean shutdown on kill signal
4. **Resource profile during sensor run**:
   - VmRSS: 1096 kB (1.07 MB)
   - VmSize: 1336 kB (1.3 MB)
   - Threads: 1
   - VmHWM: 1188 kB (peak RSS)

## Test Coverage

- **62 unit tests** (non-persist build): all pass
- **89 unit tests** (persist build): all pass
- Tests cover: config parsing, log redaction, snapshot diffing, backend
  spooling, backoff, signal handling, runtime status, and all existing
  collector/event/publisher/crypto tests

## Next Steps (M6)

1. **Backend HTTP upload**: Fully wire `HttpBackend` with TLS (rustls)
2. **Site survey integration**: Parse `iwpriv get_site_survey` output
3. **Radio statistics**: Parse `iwpriv stat` output
4. **Persistence**: Firmware modification for auto-start on boot
5. **Backend server**: Implement the receiving API endpoint
