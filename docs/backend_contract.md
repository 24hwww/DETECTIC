# Detectic Backend Ingestion Contract v1

## Security Audit
Credentials removed from repo. Hardcoded password `***` replaced with `<REDACTED>` in all markdown and Python test files. Hardcoded secret default removed from `python/detectic_client.py`. Secrets must be provided via environment variables only.

## Current Sensor Output Audit

Pipeline:
GTPR → collector → NetworkMap → PresenceEngine → Events → Pseudonymization → Publisher

Structures:
- NetworkMap { captured_at: i64, devices: Vec<Device>, raw: HashMap }
- Device { hostname?, ip?, mac?, rssi?, standard?, onemesh_stack?, assoc_time?, radio_mac?, source?, tx_rate?, rx_rate?, noise?, signal_level?, max_link_rate?, active? }
- PresenceObservation { identity, rssi, rssi_smoothed, presence, proximity, confidence, first_seen, last_seen, consecutive_seen, consecutive_missing }
- EventKind { DeviceJoined, DeviceLeft, DeviceUpdated }

No raw MAC leaves sensor. Pseudonymization via HMAC-SHA256(sensor_secret, normalized_mac).

## Canonical Ingestion Contract

### Snapshot
DeviceSnapshot {
  device_id: string // HMAC-SHA256
  observed_at: i64 epoch seconds
  associated: bool
  signal_strength: i64? // 0-128
  smoothed_signal: f64?
  signal_level: u8?
  noise: u64?
  radio_id: string // radio MAC
  bssid: string? // pseudonymized or safe identifier
  operating_standard: string?
  tx_rate: u64?
  rx_rate: u64?
  association_time: i64?
  active: string?
  proximity_score: f32 0.0-1.0
  proximity_class: enum
  confidence: f64 0.0-1.0
}

### Event
Event {
  sensor_id: string
  device_id: string
  observed_at: i64
  received_at: i64
  event_type: enum [station_appeared, station_continued, station_signal_changed, station_missing, station_absent]
  snapshot: DeviceSnapshot
  idempotency_key: string
}

Fields never sent: raw_mac, raw_ip, hostname, admin credentials, sensor secret.

## Idempotency
idempotency_key = SHA256(sensor_id || device_id || observed_at || event_type || seq)

Backend must deduplicate on idempotency_key.

## Ordering
observed_at used for analytics. received_at for ingestion latency. Out-of-order delivery tolerated.

## Sensor Identity
sensor_id stable, non-secret, configured via DETECTIC_SENSOR_ID. Authentication via HMAC-SHA256 of payload with sensor_secret. Secret never transmitted.

## Proximity Algorithm Audit
Current:
- Input signalStrength 0-128
- EWMA alpha 0.3
- Proximity thresholds based on dBm mapping; current thresholds assume dBm but EX520 reports 0-128 scale → classification currently monotonic but not calibrated.
- Confidence = sample_conf*0.6 + stability*0.3 + recency*0.1
- No band separation currently; radio_id present but not used to offset.

Limitations: No physical distance, no band correction, thresholds in dBm mismatch scale.

## Calibration Data Model
CalibrationSample {
  device_id_pseudonym
  radio_id
  band
  known_distance_m
  observed_rssi
  smoothed_rssi
  orientation
  environment_tag
  timestamp
}

Supports global + device-specific + environment-specific models.

## Radio Separation
radio_id from X_TP_RadioMac distinguishes 2.4GHz / 5GHz. Maintain separate models per radio.

## Database
PostgreSQL required. Existing tables: sensors, devices, observations, events. No SQLite.

## Privacy
Raw MAC exists only at sensor boundary before pseudonymization. Never logged to backend. Pseudonymization deterministic per sensor secret.

## Integration Test
Mock GTPR → collector → pseudonymization → presence → event → HMAC → backend ingestion verified. No raw MAC leakage. Duplicate retry idempotent.

## Live Validation
EX520V read-only polling at 10s. Observed station_appeared/continued/signal_changed/missing/absent with natural changes. No router modification.

## Security Issues
- Previous docs exposed credentials; remediated.
- Backend transport TLS must be enforced in production.
- Sensor secret must be stored securely.

## Next Milestone
Calibration experiment tooling and proximity model with band-aware thresholds.
