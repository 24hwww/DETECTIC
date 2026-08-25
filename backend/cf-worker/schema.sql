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
