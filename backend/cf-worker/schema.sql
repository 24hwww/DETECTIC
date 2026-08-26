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

