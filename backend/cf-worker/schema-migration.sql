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
