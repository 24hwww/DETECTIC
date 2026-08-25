# M5-G — Sensor Configuration Reference

## Date
2026-08-23

## Objective
Complete reference for all Detectic sensor configuration options, including
environment variables, config file format, defaults, and validation rules.

## Configuration Sources

Configuration is loaded in order of precedence (highest first):
1. **Environment variables** — always override file values
2. **Config file** — key=value format, optional
3. **Built-in defaults** — sensible values for EX520V

## Environment Variables

### Router Connection

| Variable | Default | Required | Description |
|----------|---------|----------|-------------|
| `DETECTIC_URL` | `http://192.168.0.1` | No | Router GTPR endpoint URL. Must use `http://` (not `https://`) and the LAN IP (not `127.0.0.1`). |
| `DETECTIC_USER` | `user` | No | Router username. The EX520V uses `user` for the admin account. |
| `DETECTIC_PASSWORD` | (empty) | **Yes** | Router password. Required for all commands except `status`. |
| `DETECTIC_DIALECT` | `json` | No | GTPR dialect: `json` (GDPR-JSON, EX-series) or `text` (GDPR-Text). |
| `DETECTIC_ROUTER_TIMEOUT` | `15` | No | GTPR HTTP request timeout in seconds. |

### Sensor Identity

| Variable | Default | Required | Description |
|----------|---------|----------|-------------|
| `DETECTIC_SENSOR_ID` | `home-001` | No | Unique sensor identifier sent to the backend. |
| `DETECTIC_SECRET` | (empty) | **Yes** | Per-sensor HMAC secret used for pseudonymization and upload signing. Must be unique per sensor and kept secret. |

### Polling

| Variable | Default | Description |
|----------|---------|-------------|
| `DETECTIC_INTERVAL` | `30` | Polling interval in seconds. Must be > 0. |

### Backend

| Variable | Default | Description |
|----------|---------|-------------|
| `DETECTIC_BACKEND_URL` | (none) | Backend upload URL. If not set, data is discarded (NullBackend). If set, data is pseudonymized and spooled locally. |
| `DETECTIC_BACKEND_TOKEN` | (none) | Optional bearer token for backend auth. |
| `DETECTIC_BACKEND_TIMEOUT` | `10` | Backend upload timeout in seconds. |
| `DETECTIC_UPLOAD_URL` | (none) | Legacy alias for `DETECTIC_BACKEND_URL`. |

### Offline Buffer (Spool)

| Variable | Default | Description |
|----------|---------|-------------|
| `DETECTIC_BUFFER` | `/tmp/detectic_buffer.jsonl` | Path to the bounded offline spool file. |
| `DETECTIC_BUFFER_MAX` | `262144` (256 KB) | Maximum spool file size in bytes. Oldest entries are dropped when exceeded. |

### Data Sources

| Variable | Default | Description |
|----------|---------|-------------|
| `DETECTIC_SITE_SURVEY` | `false` | If `1`/`true`, enable nearby-AP scan (`iwpriv get_site_survey`). |
| `DETECTIC_RADIO_STATS` | `false` | If `1`/`true`, enable per-radio statistics (`iwpriv stat`). |

### Logging

| Variable | Default | Description |
|----------|---------|-------------|
| `DETECTIC_LOG_LEVEL` | `info` | Log level: `error`, `warn`, `info`, or `debug`. |
| `DETECTIC_LOG_MACS` | `false` | If `1`/`true`, log raw MAC addresses. **Default false for privacy.** Only enable for debugging. |

### Resource Limits

| Variable | Default | Description |
|----------|---------|-------------|
| `DETECTIC_MAX_STATIONS` | `256` | Maximum stations per snapshot (prevents unbounded vectors). |
| `DETECTIC_MAX_NEARBY_APS` | `512` | Maximum nearby APs in site survey. |

## Config File Format

A minimal key=value config file is supported. Lines starting with `#` are
comments. Env vars always override file values.

Example `/etc/detectic.conf`:
```ini
# Detectic sensor configuration
router_url=http://192.168.0.1
router_user=user
router_password=your-password-here
secret=your-hmac-secret-here
sensor_id=home-living-room
interval=30
backend_url=https://api.detectic.example/upload
spool_path=/tmp/detectic_buffer.jsonl
spool_max_bytes=262144
enable_site_survey=false
enable_radio_stats=false
log_level=info
log_macs=false
```

Load with: `SensorConfig::from_file(Path::new("/etc/detectic.conf"))`

## Validation Rules

The `SensorConfig::validate()` method checks:
1. `router_password` is not empty
2. `secret` is not empty
3. `interval` is > 0

If validation fails, the sensor logs an error and exits without polling.

## Privacy Notes

- **Passwords** are never logged at any level
- **Secrets** are never logged at any level
- **MAC addresses** are redacted by default (only OUI prefix shown)
- **IP addresses** may appear in debug logs (use `log_level=info` in production)
- **Pseudonyms** (HMAC-SHA256) are always safe to log

## Example Configurations

### Minimal (local testing, no backend)
```bash
export DETECTIC_PASSWORD='your-password'
export DETECTIC_SECRET='test-secret'
detectic sensor
```

### Production (with backend)
```bash
export DETECTIC_PASSWORD='your-password'
export DETECTIC_SECRET='production-secret-unique-per-sensor'
export DETECTIC_SENSOR_ID='home-001'
export DETECTIC_INTERVAL=30
export DETECTIC_BACKEND_URL='https://api.detectic.example/upload'
export DETECTIC_BUFFER='/tmp/detectic_buffer.jsonl'
export DETECTIC_BUFFER_MAX=262144
export DETECTIC_LOG_LEVEL=info
detectic sensor
```

### Debug (verbose, with MACs)
```bash
export DETECTIC_PASSWORD='your-password'
export DETECTIC_SECRET='test-secret'
export DETECTIC_LOG_LEVEL=debug
export DETECTIC_LOG_MACS=1
export DETECTIC_INTERVAL=10
detectic sensor
```
