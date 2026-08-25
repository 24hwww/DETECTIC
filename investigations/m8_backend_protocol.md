# M8 — Backend Protocol

## Date
2026-08-23

## Objective
Define the Detectic backend event contract and implement an authenticated,
resilient HTTP transport for the sensor.

## Event Payload

The sensor sends batched events to the backend. Each event is an independent
JSON object.

```json
{
  "sensor_id": "home-001",
  "timestamp": 1787498720,
  "event": "join",
  "pseudonym": "01e27336ec9d45640973041cd735a957...",
  "device": {
    "hostname": "realme-9i",
    "ip": "192.168.0.22",
    "ipv6": "2804:5020:...",
    "client_type": "Android",
    "interface": "Device.WiFi.AccessPoint.1.",
    "rssi": 86,
    "noise": 50,
    "tx_rate": 57000,
    "rx_rate": 58000,
    "signal_level": 3,
    "max_link_rate": 72000,
    "active": true
  },
  "presence": {
    "state": "present",
    "proximity": "near",
    "confidence": 0.91
  }
}
```

`event` may be `join`, `update`, or `leave`.

`pseudonym` is the HMAC-SHA256 of the MAC, generated locally with the
per-sensor secret. No raw MAC address leaves the device.

## Transport

`HttpBackend` in `src/backend.rs`:

- POSTs a JSON `UploadPayload` to the configured `DETECTIC_BACKEND_URL`
- Headers:
  - `Content-Type: application/json`
  - `X-Detectic-Sensor: <sensor_id>`
  - `X-Detectic-Signature: <hmac-sha256-hex>`
  - `Authorization: Bearer <token>` (if `DETECTIC_BACKEND_TOKEN` is set)
- Exponential backoff on network/5xx errors
- 4xx responses are not retried or spooled (permanent)
- Failed uploads are appended to a bounded local JSONL spool (`DETECTIC_BUFFER`)
- At the start of each poll cycle, the runtime calls `drain_spool()` to retry
  buffered entries

## Offline Mode

When the backend is unreachable:

1. Events are serialized as JSONL and appended to the spool.
2. If the spool exceeds `DETECTIC_BUFFER_MAX` bytes, the oldest lines are
   dropped.
3. When the backend returns, `drain_spool()` reads the file line by line and
   re-sends each payload.
4. Successfully sent lines are removed; failures are kept for the next cycle.

## Authentication

- `DETECTIC_SECRET` — per-sensor HMAC key, local-only, never transmitted
- `DETECTIC_BACKEND_TOKEN` — optional Bearer token, redacted in logs and
  `detectic config` output

## Security

- No router credentials are sent to the backend
- No raw MACs are sent to the backend
- The token never appears in logs or the `config` command
- HTTPS is supported when the binary is built with TLS (see notes below)

## TLS Notes

The default on-router build (`--no-default-features`) links `ureq` without TLS
to keep the binary small and free of C dependencies. If the backend is HTTP on
the same LAN this is acceptable. For HTTPS, build with the `persist` feature
which enables `rustls`:

```bash
cargo build --release --features persist \
  --target aarch64-unknown-linux-musl
```

This increases binary size and requires a C toolchain for the `rustls`/`ring`
dependencies.
