-- Detectic Backend — D1 Schema
-- Applied via: npx wrangler d1 execute detectic-db --file=schema.sql

CREATE TABLE IF NOT EXISTS sensors (
  id         TEXT PRIMARY KEY,
  name       TEXT,
  location   TEXT,
  created_at INTEGER NOT NULL,
  last_seen  INTEGER
);

CREATE TABLE IF NOT EXISTS snapshots (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  sensor_id    TEXT NOT NULL,
  received_at  INTEGER NOT NULL,
  captured_at  INTEGER,
  device_count INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS detections (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  snapshot_id  INTEGER NOT NULL,
  sensor_id    TEXT NOT NULL,
  pseudonym    TEXT NOT NULL,
  rssi         INTEGER,
  source       TEXT,
  standard     TEXT,
  radio_mac    TEXT
);

CREATE INDEX IF NOT EXISTS idx_det_pseudo ON detections(pseudonym);
CREATE INDEX IF NOT EXISTS idx_det_sensor ON detections(sensor_id);

CREATE TABLE IF NOT EXISTS events (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  sensor_id       TEXT NOT NULL,
  event_id        TEXT NOT NULL UNIQUE,
  event_type      TEXT NOT NULL,
  event_timestamp INTEGER NOT NULL,
  device_id       TEXT,
  snapshot_json   TEXT,
  schema_version  TEXT,
  received_at     INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_evt_sensor ON events(sensor_id);
CREATE INDEX IF NOT EXISTS idx_evt_device ON events(device_id);

-- Canonical temporal model (schema v3): current device state, connection
-- sessions, and per-sensor sequence tracking. All identifiers are HMAC
-- pseudonyms; raw MACs are never stored.

CREATE TABLE IF NOT EXISTS device_state (
  sensor_id           TEXT NOT NULL,
  device_id           TEXT NOT NULL,
  state               TEXT NOT NULL,
  last_signal         INTEGER,
  noise               INTEGER,
  band                TEXT,
  interface           TEXT,
  current_session_id  TEXT,
  first_seen          INTEGER,
  last_seen           INTEGER,
  total_connected_time INTEGER NOT NULL DEFAULT 0,
  connection_count    INTEGER NOT NULL DEFAULT 0,
  updated_at          INTEGER NOT NULL,
  PRIMARY KEY (sensor_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_ds_state ON device_state(sensor_id, state);
CREATE INDEX IF NOT EXISTS idx_ds_last_seen ON device_state(sensor_id, last_seen);

CREATE TABLE IF NOT EXISTS device_sessions (
  session_id       TEXT PRIMARY KEY,
  sensor_id        TEXT NOT NULL,
  device_id        TEXT NOT NULL,
  started_at       INTEGER NOT NULL,
  ended_at         INTEGER,
  duration_seconds INTEGER,
  band             TEXT,
  last_signal      INTEGER,
  last_noise       INTEGER,
  received_at      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dss_dev ON device_sessions(sensor_id, device_id);
CREATE INDEX IF NOT EXISTS idx_dss_start ON device_sessions(started_at);

CREATE TABLE IF NOT EXISTS sensor_sequences (
  sensor_id     TEXT PRIMARY KEY,
  last_sequence INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);

ALTER TABLE events ADD COLUMN payload_json TEXT;
ALTER TABLE events ADD COLUMN sequence INTEGER;
ALTER TABLE events ADD COLUMN acked INTEGER NOT NULL DEFAULT 0;

-- AP / network state and RF environment snapshots.
-- Identifiers are HMAC pseudonyms; BSSID/MAC are never stored in plaintext.

CREATE TABLE IF NOT EXISTS ap_state (
  sensor_id          TEXT NOT NULL,
  ap_id              TEXT NOT NULL,
  status             TEXT NOT NULL,
  ssid               TEXT,
  band               TEXT,
  channel            INTEGER,
  current_signal     INTEGER,
  average_signal     REAL,
  min_signal         INTEGER,
  max_signal         INTEGER,
  rssi_variance      REAL,
  observation_count  INTEGER NOT NULL DEFAULT 0,
  session_count      INTEGER NOT NULL DEFAULT 0,
  channel_history    TEXT,
  first_seen         INTEGER,
  last_seen          INTEGER,
  online_since       INTEGER,
  security           TEXT,
  w_mode             TEXT,
  extch              TEXT,
  proximity          TEXT,
  proximity_detail   TEXT,
  updated_at         INTEGER NOT NULL,
  PRIMARY KEY (sensor_id, ap_id)
);

CREATE INDEX IF NOT EXISTS idx_ap_sensor_status ON ap_state(sensor_id, status);
CREATE INDEX IF NOT EXISTS idx_ap_last_seen ON ap_state(sensor_id, last_seen);

CREATE TABLE IF NOT EXISTS ap_sessions (
  session_id        TEXT PRIMARY KEY,
  sensor_id         TEXT NOT NULL,
  ap_id             TEXT NOT NULL,
  started_at        INTEGER NOT NULL,
  ended_at          INTEGER,
  duration_seconds  INTEGER,
  observation_count INTEGER NOT NULL DEFAULT 0,
  rssi_average      REAL,
  rssi_min          INTEGER,
  rssi_max          INTEGER,
  channel_history   TEXT,
  received_at       INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_aps_ap ON ap_sessions(sensor_id, ap_id);
CREATE INDEX IF NOT EXISTS idx_aps_start ON ap_sessions(started_at);

CREATE TABLE IF NOT EXISTS rf_environment_snapshots (
  event_id            TEXT PRIMARY KEY,
  sensor_id           TEXT NOT NULL,
  event_timestamp     INTEGER NOT NULL,
  ap_count            INTEGER NOT NULL DEFAULT 0,
  ap_count_2_4        INTEGER NOT NULL DEFAULT 0,
  ap_count_5          INTEGER NOT NULL DEFAULT 0,
  strongest_signal    INTEGER,
  weakest_signal      INTEGER,
  average_signal      INTEGER,
  rssi_variance       REAL,
  channel_distribution TEXT,
  top_aps             TEXT,
  received_at         INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rf_sensor_ts ON rf_environment_snapshots(sensor_id, event_timestamp);

-- Temporary debug log for ingest auth mismatches. Stores only metadata/pseudonyms,
-- not raw device details. Retain only the last 20 entries per sensor.
CREATE TABLE IF NOT EXISTS debug_ingest_log (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  sensor_id     TEXT NOT NULL,
  captured_at   INTEGER,
  received_at   INTEGER NOT NULL,
  got_id        TEXT,
  expected_id   TEXT,
  pseudos_json  TEXT,
  body_sha256   TEXT,
  reason        TEXT
);

CREATE INDEX IF NOT EXISTS idx_debug_ingest_sensor ON debug_ingest_log(sensor_id, received_at);

-- Human-editable labels for devices. Separated from device_identity so edits
-- do not depend on the (pseudonym, sensor_id) composite key of that table.
CREATE TABLE IF NOT EXISTS device_label (
  pseudonym TEXT PRIMARY KEY,
  alias TEXT,
  owner TEXT,
  room TEXT,
  tags TEXT,  -- JSON array, validated by the API
  notes TEXT,
  updated_at INTEGER NOT NULL
);

-- Report configuration and generated queue.
CREATE TABLE IF NOT EXISTS report_config (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  enabled INTEGER NOT NULL DEFAULT 0,
  frequency_hours INTEGER NOT NULL DEFAULT 24,
  changes_only INTEGER NOT NULL DEFAULT 0,
  top_devices INTEGER NOT NULL DEFAULT 5,
  new_detections INTEGER NOT NULL DEFAULT 1,
  nearby_aps INTEGER NOT NULL DEFAULT 1,
  email_to TEXT,
  email_subject TEXT,
  updated_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS email_queue (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  report_id TEXT NOT NULL,
  scheduled_at INTEGER NOT NULL,
  generated_at INTEGER NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending',
  html TEXT,
  text TEXT,
  config_json TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  last_attempt_at INTEGER,
  sent_at INTEGER,
  error TEXT
);

-- Trust / unknown-device tracking. `status` can be 'unknown', 'known', or 'ignored'.
-- When a device is first observed it is inserted as 'unknown' and may trigger alerts.
CREATE TABLE IF NOT EXISTS device_trust (
  pseudonym TEXT PRIMARY KEY,
  sensor_id TEXT,
  status TEXT NOT NULL DEFAULT 'unknown',
  first_seen INTEGER,
  last_seen INTEGER,
  alert_count INTEGER NOT NULL DEFAULT 0,
  acknowledged_at INTEGER,
  updated_at INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_device_trust_status ON device_trust(status);

-- IP/MAC bindings observed from ARP, IPv6 NDP, DHCP or Wi-Fi association.
-- Used for faster presence detection and correlation when the sensor has
-- shell access to read neighbor tables.
CREATE TABLE IF NOT EXISTS device_ip (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  pseudonym TEXT NOT NULL,
  ip TEXT NOT NULL,
  mac TEXT,
  source TEXT NOT NULL DEFAULT 'arp',
  sensor_id TEXT,
  first_seen INTEGER,
  last_seen INTEGER,
  confidence REAL NOT NULL DEFAULT 1.0,
  UNIQUE (pseudonym, ip, source)
);

CREATE INDEX IF NOT EXISTS idx_device_ip_pseudo ON device_ip(pseudonym);
CREATE INDEX IF NOT EXISTS idx_device_ip_ip ON device_ip(ip);
CREATE INDEX IF NOT EXISTS idx_device_ip_mac ON device_ip(mac);

-- Historical sensor health / telemetry metrics.
CREATE TABLE IF NOT EXISTS sensor_health (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  sensor_id TEXT NOT NULL,
  reported_at INTEGER NOT NULL,
  cpu_percent REAL,
  memory_percent REAL,
  memory_used_mb REAL,
  memory_total_mb REAL,
  uptime_seconds INTEGER,
  temperature_c REAL,
  load_1m REAL,
  load_5m REAL,
  load_15m REAL,
  network_rx_mb REAL,
  network_tx_mb REAL,
  disk_used_percent REAL,
  wifi_clients INTEGER,
  wifi_aps INTEGER,
  custom_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_sensor_health_time ON sensor_health(sensor_id, reported_at);

