/**
 * Detectic Backend — Cloudflare Worker + D1
 *
 * Ultra-lightweight, zero-server backend for EX520 sensors.
 * Free tier: 100K req/day, 5 GB D1 storage.
 *
 * Endpoints:
 *   POST /api/v1/events       — ingest snapshot (HMAC auth)
 *   POST /api/v1/events/batch — batch ingest
 *   GET  /api/v1/devices      — device history
 *   GET  /api/v1/presence     — presence analytics
 *   GET  /api/v1/sensors      — sensor list
 *   GET  /api/v1/stats        — global stats
 *   GET  /api/v1/networks     — AP state by sensor or all sensors
 *   GET  /api/v1/fusion       — cross-sensor AP correlation
 *   GET  /api/v1/healthz      — health check
 *   GET  /                  — real-time dashboard UI
 */

import dashboardHtml from './dashboard.html';
import { RealtimeHub } from './realtime';

interface Env {
  DB: D1Database;
  DETECTIC_SENSORS: string;  // JSON: {"sensor_id": "secret", ...}
  DETECTIC_MASTER_SECRET: string;
  REALTIME_HUB: DurableObjectNamespace<RealtimeHub>;
}

interface SensorPayload {
  sensor_id?: string;
  id?: string;
  captured_at?: number;
  devices?: Array<{
    pseudonym?: string;
    rssi?: number;
    source?: string;
    standard?: string;
    radio_mac?: string;
    mac?: string;
    ip?: string;
    hostname?: string;
  }>;
  events?: Array<{
    event_id?: string;
    event_type?: string;
    event_timestamp?: number;
    device_id?: string;
    snapshot?: unknown;
    schema_version?: string;
    // Canonical envelope (schema v3)
    type?: string;
    timestamp?: number;
    sequence?: number;
    payload?: unknown;
  }>;
}

// ---------------------------------------------------------------------------
// HMAC verification
// ---------------------------------------------------------------------------

function hexToBytes(hex: string): Uint8Array {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) {
    bytes[i / 2] = parseInt(hex.substr(i, 2), 16);
  }
  return bytes;
}

async function hmacSha256(secret: string, data: Uint8Array): Promise<string> {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"]
  );
  const sig = await crypto.subtle.sign("HMAC", key, data);
  return Array.from(new Uint8Array(sig))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

async function verifyAuth(
  env: Env,
  sensorId: string,
  signature: string,
  body: string,
  timestamp?: string | null
): Promise<boolean> {
  const sensors = JSON.parse(env.DETECTIC_SENSORS || "{}");
  const secret = sensors[sensorId];
  if (!secret || !signature) return false;

  // Canonical HMAC contract V1 (with replay protection):
  //   signed content = "<timestamp>\n<body>"
  //   key = UTF-8 bytes of secret string
  // When X-Detectic-Timestamp is present, verify the timestamped signature
  // and reject replays outside the ±300s window.
  if (timestamp) {
    const tsNum = parseInt(timestamp, 10);
    if (isNaN(tsNum)) return false;
    const now = Math.floor(Date.now() / 1000);
    if (Math.abs(now - tsNum) > 300) return false; // replay window
    const signed = new TextEncoder().encode(timestamp + "\n" + body);
    const expected = await hmacSha256(secret, signed);
    return expected === signature;
  }

  // Legacy fallback (no timestamp): sign body only.
  // Kept temporarily for backward compatibility during migration.
  const expectedLegacy = await hmacSha256(secret, new TextEncoder().encode(body));
  return expectedLegacy === signature;
}

function pseudoHmac(masterSecret: string, identifier: string): string {
  // Use Web Crypto for HMAC (must be async, but we sync-call in D1 batch)
  // Fallback: simple hash-based pseudonym for inline use
  let hash = 0;
  const salt = masterSecret;
  const str = salt + identifier;
  for (let i = 0; i < str.length; i++) {
    const chr = str.charCodeAt(i);
    hash = (hash << 5) - hash + chr;
    hash |= 0;
  }
  // Return hex representation
  return Math.abs(hash).toString(16).padStart(8, "0") +
    Date.now().toString(16).slice(-8);
}

// Better pseudonym using SubtleCrypto (async)
async function pseudonymize(masterSecret: string, identifier: string): Promise<string> {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(masterSecret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"]
  );
  const sig = await crypto.subtle.sign(
    "HMAC",
    key,
    new TextEncoder().encode(identifier)
  );
  return Array.from(new Uint8Array(sig))
    .slice(0, 16)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

// ---------------------------------------------------------------------------
// Schema init
// ---------------------------------------------------------------------------

let schemaReady = false;

async function patchColumns(db: D1Database): Promise<void> {
  // Plain ADD COLUMN (SQLite has no ADD COLUMN IF NOT EXISTS); a duplicate
  // column error is expected and ignored so this is idempotent/safe.
  const alters = [
    `ALTER TABLE collector_devices ADD COLUMN hostname TEXT`,
    `ALTER TABLE collector_devices ADD COLUMN band TEXT`,
    `ALTER TABLE collector_devices ADD COLUMN signal_level INTEGER`,
    `ALTER TABLE collector_devices ADD COLUMN noise INTEGER`,
    `ALTER TABLE collector_devices ADD COLUMN operating_standard TEXT`,
    `ALTER TABLE collector_devices ADD COLUMN tx_rate_kbps INTEGER`,
    `ALTER TABLE collector_devices ADD COLUMN rx_rate_kbps INTEGER`,
    `ALTER TABLE collector_devices ADD COLUMN status TEXT`,
    `ALTER TABLE collector_devices ADD COLUMN bssid_pseudonym TEXT`,
    `ALTER TABLE collector_devices ADD COLUMN identity_json TEXT`,
    `ALTER TABLE collector_captures ADD COLUMN payload_hash TEXT`,
    `ALTER TABLE device_identity ADD COLUMN bssid_manufacturer TEXT`,
    `ALTER TABLE device_identity ADD COLUMN identity_json TEXT`,
    `ALTER TABLE events ADD COLUMN payload_json TEXT`,
    `ALTER TABLE events ADD COLUMN sequence INTEGER`,
    `ALTER TABLE events ADD COLUMN acked INTEGER NOT NULL DEFAULT 0`,
  ];
  for (const sql of alters) {
    try { await db.exec(sql); } catch { /* column already exists */ }
  }
}

async function ensureSchema(db: D1Database): Promise<void> {
  if (schemaReady) return;
  try {
    // Use batch for atomic schema creation
    await db.batch([
      db.prepare(`CREATE TABLE IF NOT EXISTS sensors (
        id TEXT PRIMARY KEY, name TEXT, location TEXT,
        created_at INTEGER NOT NULL, last_seen INTEGER
      )`),
      db.prepare(`CREATE TABLE IF NOT EXISTS snapshots (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        sensor_id TEXT NOT NULL, received_at INTEGER NOT NULL,
        captured_at INTEGER, device_count INTEGER DEFAULT 0
      )`),
      db.prepare(`CREATE TABLE IF NOT EXISTS detections (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        snapshot_id INTEGER NOT NULL, sensor_id TEXT NOT NULL,
        pseudonym TEXT NOT NULL, rssi INTEGER, source TEXT,
        standard TEXT, radio_mac TEXT
      )`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_det_pseudo ON detections(pseudonym)`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_det_sensor ON detections(sensor_id)`),
      db.prepare(`CREATE TABLE IF NOT EXISTS events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        sensor_id TEXT NOT NULL, event_id TEXT NOT NULL UNIQUE,
        event_type TEXT NOT NULL, event_timestamp INTEGER NOT NULL,
        device_id TEXT, snapshot_json TEXT, schema_version TEXT,
        received_at INTEGER NOT NULL
      )`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_evt_sensor ON events(sensor_id)`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_evt_device ON events(device_id)`),
      db.prepare(`CREATE TABLE IF NOT EXISTS collector_captures (capture_id TEXT PRIMARY KEY, run_id TEXT NOT NULL, sensor_id TEXT NOT NULL, scheduled_at INTEGER NOT NULL, started_at INTEGER NOT NULL, completed_at INTEGER, status TEXT NOT NULL, api_latency_ms INTEGER, auth_latency_ms INTEGER, device_count INTEGER, active_device_count INTEGER, payload_hash TEXT, created_at INTEGER NOT NULL)`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_cc_sensor ON collector_captures(sensor_id)`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_cc_status ON collector_captures(status)`),
      db.prepare(`CREATE TABLE IF NOT EXISTS collector_devices (id INTEGER PRIMARY KEY AUTOINCREMENT, capture_id TEXT NOT NULL, pseudonym TEXT NOT NULL, hostname TEXT, band TEXT, signal_strength INTEGER, signal_level INTEGER, noise INTEGER, operating_standard TEXT, tx_rate_kbps INTEGER, rx_rate_kbps INTEGER, status TEXT, bssid_pseudonym TEXT, identity_json TEXT)`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_cd_capture ON collector_devices(capture_id)`),
      db.prepare(`CREATE TABLE IF NOT EXISTS collector_runs (run_id TEXT PRIMARY KEY, scheduled_at INTEGER NOT NULL, started_at INTEGER NOT NULL, completed_at INTEGER, status TEXT NOT NULL, duration_ms INTEGER)`),
      db.prepare(`CREATE TABLE IF NOT EXISTS device_identity (pseudonym TEXT NOT NULL, sensor_id TEXT NOT NULL, manufacturer TEXT, brand TEXT, model_guess TEXT, device_class TEXT, mac_type TEXT, confidence REAL, confidence_label TEXT, bssid_manufacturer TEXT, identity_json TEXT, last_seen INTEGER, PRIMARY KEY (pseudonym, sensor_id))`),
      db.prepare(`CREATE TABLE IF NOT EXISTS device_fingerprint (pseudonym TEXT NOT NULL, model TEXT, confidence REAL, evidence TEXT, PRIMARY KEY (pseudonym, model))`),
      db.prepare(`CREATE TABLE IF NOT EXISTS identity_evidence (id INTEGER PRIMARY KEY AUTOINCREMENT, pseudonym TEXT NOT NULL, sensor_id TEXT NOT NULL, evidence_type TEXT, description TEXT, weight REAL, captured_at INTEGER)`),
      db.prepare(`CREATE TABLE IF NOT EXISTS wifi_network_observation (bssid_pseudonym TEXT NOT NULL, ssid TEXT, manufacturer TEXT, band TEXT, first_seen INTEGER, last_seen INTEGER, observation_count INTEGER, sensor_id TEXT, PRIMARY KEY (bssid_pseudonym, sensor_id))`),
      db.prepare(`CREATE TABLE IF NOT EXISTS device_state (
        sensor_id TEXT NOT NULL, device_id TEXT NOT NULL, state TEXT NOT NULL,
        last_signal INTEGER, noise INTEGER, band TEXT, interface TEXT,
        current_session_id TEXT, first_seen INTEGER, last_seen INTEGER,
        total_connected_time INTEGER NOT NULL DEFAULT 0,
        connection_count INTEGER NOT NULL DEFAULT 0,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (sensor_id, device_id)
      )`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_ds_state ON device_state(sensor_id, state)`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_ds_last_seen ON device_state(sensor_id, last_seen)`),
      db.prepare(`CREATE TABLE IF NOT EXISTS device_sessions (
        session_id TEXT PRIMARY KEY, sensor_id TEXT NOT NULL, device_id TEXT NOT NULL,
        started_at INTEGER NOT NULL, ended_at INTEGER, duration_seconds INTEGER,
        band TEXT, last_signal INTEGER, last_noise INTEGER, received_at INTEGER NOT NULL
      )`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_dss_dev ON device_sessions(sensor_id, device_id)`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_dss_start ON device_sessions(started_at)`),
      db.prepare(`CREATE TABLE IF NOT EXISTS sensor_sequences (
        sensor_id TEXT PRIMARY KEY, last_sequence INTEGER NOT NULL, updated_at INTEGER NOT NULL
      )`),
    ]);
    schemaReady = true;
    // Self-heal: older live tables may predate columns added later.
    // ADD COLUMN is a no-op if the column already exists; failures are ignored.
    await patchColumns(db);
  } catch (e: any) {
    // If batch fails (e.g., tables already exist), try individual execs
    const sqls = [
      `CREATE TABLE IF NOT EXISTS sensors (id TEXT PRIMARY KEY, name TEXT, location TEXT, created_at INTEGER NOT NULL, last_seen INTEGER)`,
      `CREATE TABLE IF NOT EXISTS snapshots (id INTEGER PRIMARY KEY AUTOINCREMENT, sensor_id TEXT NOT NULL, received_at INTEGER NOT NULL, captured_at INTEGER, device_count INTEGER DEFAULT 0)`,
      `CREATE TABLE IF NOT EXISTS detections (id INTEGER PRIMARY KEY AUTOINCREMENT, snapshot_id INTEGER NOT NULL, sensor_id TEXT NOT NULL, pseudonym TEXT NOT NULL, rssi INTEGER, source TEXT, standard TEXT, radio_mac TEXT)`,
      `CREATE INDEX IF NOT EXISTS idx_det_pseudo ON detections(pseudonym)`,
      `CREATE INDEX IF NOT EXISTS idx_det_sensor ON detections(sensor_id)`,
      `CREATE TABLE IF NOT EXISTS events (id INTEGER PRIMARY KEY AUTOINCREMENT, sensor_id TEXT NOT NULL, event_id TEXT NOT NULL UNIQUE, event_type TEXT NOT NULL, event_timestamp INTEGER NOT NULL, device_id TEXT, snapshot_json TEXT, schema_version TEXT, received_at INTEGER NOT NULL)`,
      `CREATE INDEX IF NOT EXISTS idx_evt_sensor ON events(sensor_id)`,
      `CREATE INDEX IF NOT EXISTS idx_evt_device ON events(device_id)`,
      `CREATE TABLE IF NOT EXISTS collector_captures (capture_id TEXT PRIMARY KEY, run_id TEXT NOT NULL, sensor_id TEXT NOT NULL, scheduled_at INTEGER NOT NULL, started_at INTEGER NOT NULL, completed_at INTEGER, status TEXT NOT NULL, api_latency_ms INTEGER, auth_latency_ms INTEGER, device_count INTEGER, active_device_count INTEGER, payload_hash TEXT, created_at INTEGER NOT NULL)`,
      `CREATE INDEX IF NOT EXISTS idx_cc_sensor ON collector_captures(sensor_id)`,
      `CREATE INDEX IF NOT EXISTS idx_cc_status ON collector_captures(status)`,
      `CREATE TABLE IF NOT EXISTS collector_devices (id INTEGER PRIMARY KEY AUTOINCREMENT, capture_id TEXT NOT NULL, pseudonym TEXT NOT NULL, hostname TEXT, band TEXT, signal_strength INTEGER, signal_level INTEGER, noise INTEGER, operating_standard TEXT, tx_rate_kbps INTEGER, rx_rate_kbps INTEGER, status TEXT, bssid_pseudonym TEXT, identity_json TEXT)`,
      `CREATE INDEX IF NOT EXISTS idx_cd_capture ON collector_devices(capture_id)`,
      `CREATE TABLE IF NOT EXISTS collector_runs (run_id TEXT PRIMARY KEY, scheduled_at INTEGER NOT NULL, started_at INTEGER NOT NULL, completed_at INTEGER, status TEXT NOT NULL, duration_ms INTEGER)`,
      `CREATE TABLE IF NOT EXISTS device_identity (pseudonym TEXT NOT NULL, sensor_id TEXT NOT NULL, manufacturer TEXT, brand TEXT, model_guess TEXT, device_class TEXT, mac_type TEXT, confidence REAL, confidence_label TEXT, bssid_manufacturer TEXT, identity_json TEXT, last_seen INTEGER, PRIMARY KEY (pseudonym, sensor_id))`,
      `CREATE TABLE IF NOT EXISTS device_fingerprint (pseudonym TEXT NOT NULL, model TEXT, confidence REAL, evidence TEXT, PRIMARY KEY (pseudonym, model))`,
      `CREATE TABLE IF NOT EXISTS identity_evidence (id INTEGER PRIMARY KEY AUTOINCREMENT, pseudonym TEXT NOT NULL, sensor_id TEXT NOT NULL, evidence_type TEXT, description TEXT, weight REAL, captured_at INTEGER)`,
      `CREATE TABLE IF NOT EXISTS wifi_network_observation (bssid_pseudonym TEXT NOT NULL, ssid TEXT, manufacturer TEXT, band TEXT, first_seen INTEGER, last_seen INTEGER, observation_count INTEGER, sensor_id TEXT, PRIMARY KEY (bssid_pseudonym, sensor_id))`,
      `CREATE TABLE IF NOT EXISTS device_state (sensor_id TEXT NOT NULL, device_id TEXT NOT NULL, state TEXT NOT NULL, last_signal INTEGER, noise INTEGER, band TEXT, interface TEXT, current_session_id TEXT, first_seen INTEGER, last_seen INTEGER, total_connected_time INTEGER NOT NULL DEFAULT 0, connection_count INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL, PRIMARY KEY (sensor_id, device_id))`,
      `CREATE INDEX IF NOT EXISTS idx_ds_state ON device_state(sensor_id, state)`,
      `CREATE INDEX IF NOT EXISTS idx_ds_last_seen ON device_state(sensor_id, last_seen)`,
      `CREATE TABLE IF NOT EXISTS device_sessions (session_id TEXT PRIMARY KEY, sensor_id TEXT NOT NULL, device_id TEXT NOT NULL, started_at INTEGER NOT NULL, ended_at INTEGER, duration_seconds INTEGER, band TEXT, last_signal INTEGER, last_noise INTEGER, received_at INTEGER NOT NULL)`,
      `CREATE INDEX IF NOT EXISTS idx_dss_dev ON device_sessions(sensor_id, device_id)`,
      `CREATE INDEX IF NOT EXISTS idx_dss_start ON device_sessions(started_at)`,
      `CREATE TABLE IF NOT EXISTS sensor_sequences (sensor_id TEXT PRIMARY KEY, last_sequence INTEGER NOT NULL, updated_at INTEGER NOT NULL)`,
    ];
    for (const sql of sqls) {
      try { await db.exec(sql); } catch { /* ignore */ }
    }
    schemaReady = true;
  }
}

// ---------------------------------------------------------------------------
// CORS
// ---------------------------------------------------------------------------

function corsHeaders(origin?: string): Record<string, string> {
  return {
    "Access-Control-Allow-Origin": origin || "*",
    "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
    "Access-Control-Allow-Headers": "Content-Type, X-Detectic-Sensor, X-Detectic-Signature, Authorization",
    "Access-Control-Max-Age": "86400",
  };
}

function jsonResponse(status: number, body: unknown, origin?: string): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "Content-Type": "application/json",
      "X-Content-Type-Options": "nosniff",
      "Cache-Control": "no-store",
      ...corsHeaders(origin),
    },
  });
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async function handleIngest(
  request: Request,
  env: Env,
  ctx: ExecutionContext
): Promise<Response> {
  const origin = request.headers.get("Origin") || undefined;

  if (request.method === "OPTIONS") {
    return new Response(null, { status: 204, headers: corsHeaders(origin) });
  }

  const sensorId = request.headers.get("X-Detectic-Sensor") || "";
  const signature = request.headers.get("X-Detectic-Signature") || "";
  const timestamp = request.headers.get("X-Detectic-Timestamp");
  const bodyText = await request.text();

  if (bodyText.length > 4 * 1024 * 1024) {
    return jsonResponse(400, { error: "body too large" }, origin);
  }

  if (!(await verifyAuth(env, sensorId, signature, bodyText, timestamp))) {
    return jsonResponse(401, { error: "unauthorized" }, origin);
  }

  let payload: SensorPayload;
  try {
    payload = JSON.parse(bodyText);
  } catch {
    return jsonResponse(400, { error: "invalid json" }, origin);
  }

  // Handle event batch
  if (payload.events && Array.isArray(payload.events)) {
    return handleEventBatch(env, ctx, sensorId, payload, origin);
  }

  // Handle snapshot
  return handleSnapshot(env, sensorId, payload, origin);
}

async function handleSnapshot(
  env: Env,
  sensorId: string,
  payload: SensorPayload,
  origin?: string
): Promise<Response> {
  const now = Math.floor(Date.now() / 1000);
  const capturedAt = payload.captured_at || now;
  const devices = payload.devices || [];

  // Sanitize: only persist pseudonym + radio metadata
  const sanitizedDevices = devices
    .filter((d) => d.pseudonym)
    .map((d) => ({
      pseudonym: d.pseudonym!,
      rssi: d.rssi ?? null,
      source: d.source ?? null,
      standard: d.standard ?? null,
      radio_mac: d.radio_mac ?? null,
    }));

  // Insert snapshot
  const snapResult = await env.DB.prepare(
    "INSERT INTO snapshots (sensor_id, received_at, captured_at, device_count) VALUES (?, ?, ?, ?)"
  )
    .bind(sensorId, now, capturedAt, sanitizedDevices.length)
    .run();

  const snapId = snapResult.meta.last_row_id;

  // Batch insert detections
  if (sanitizedDevices.length > 0) {
    const stmts = sanitizedDevices.map((d) =>
      env.DB.prepare(
        "INSERT INTO detections (snapshot_id, sensor_id, pseudonym, rssi, source, standard, radio_mac) VALUES (?, ?, ?, ?, ?, ?, ?)"
      ).bind(snapId, sensorId, d.pseudonym, d.rssi, d.source, d.standard, d.radio_mac)
    );
    await env.DB.batch(stmts);
  }

  // Update sensor last_seen
  await env.DB.prepare(
    "INSERT INTO sensors (id, created_at, last_seen) VALUES (?, ?, ?) ON CONFLICT(id) DO UPDATE SET last_seen = ?"
  )
    .bind(sensorId, now, now, now)
    .run();

  return jsonResponse(
    200,
    { snapshot: snapId, devices_stored: sanitizedDevices.length },
    origin
  );
}

async function handleEventBatch(
  env: Env,
  ctx: ExecutionContext,
  sensorId: string,
  payload: SensorPayload,
  origin?: string
): Promise<Response> {
  const now = Math.floor(Date.now() / 1000);
  const events = (payload.events || []).slice(0, 100); // max 100 per batch

  let accepted = 0;
  let duplicates = 0;
  let maxSeq: number | null = null;
  const sideEffects: D1PreparedStatement[] = [];

  for (const evt of events) {
    const eventId = evt.event_id || "";
    if (!eventId) continue;

    const type = evt.type || evt.event_type || "";
    const ts = evt.timestamp ?? evt.event_timestamp ?? now;
    const deviceId = evt.device_id ?? null;
    const payloadJson = evt.payload !== undefined ? JSON.stringify(evt.payload) : null;
    const seq = typeof evt.sequence === "number" ? evt.sequence : null;
    if (seq !== null) {
      maxSeq = maxSeq === null ? seq : Math.max(maxSeq, seq);
    }

    try {
      await env.DB.prepare(
        "INSERT INTO events (sensor_id, event_id, event_type, event_timestamp, device_id, snapshot_json, payload_json, sequence, schema_version, received_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
      )
        .bind(
          sensorId,
          eventId,
          type,
          ts,
          deviceId,
          evt.snapshot ? JSON.stringify(evt.snapshot) : null,
          payloadJson,
          seq,
          evt.schema_version || (evt.type ? "3.0" : "2.0"),
          now
        )
        .run();
      accepted++;

      if (deviceId) {
        if (type.startsWith("device.")) {
          applyTemporalSideEffects(env, sideEffects, sensorId, type, ts, now, deviceId, evt.payload);
        } else if (type.startsWith("network.")) {
          applyApSideEffects(env, sideEffects, sensorId, type, ts, now, deviceId, evt.payload);
        }
      } else if (type === "rf.environment_snapshot") {
        applyRfSnapshot(env, sideEffects, sensorId, eventId, ts, now, evt.payload);
      }
    } catch (e: any) {
      if (e.message?.includes("UNIQUE")) {
        duplicates++;
      }
    }
  }

  if (maxSeq !== null) {
    sideEffects.push(
      env.DB.prepare(
        `INSERT INTO sensor_sequences (sensor_id, last_sequence, updated_at)
         VALUES (?, ?, ?)
         ON CONFLICT(sensor_id) DO UPDATE SET
           last_sequence = MAX(last_sequence, excluded.last_sequence),
           updated_at = excluded.updated_at`
      ).bind(sensorId, maxSeq, now)
    );
  }
  if (sideEffects.length > 0) {
    await env.DB.batch(sideEffects);
  }

  // Fan accepted events out to subscribed frontends via the realtime hub.
  if (accepted > 0 && env.REALTIME_HUB) {
    const acceptedEvents = events.filter((_, i) => i < accepted);
    ctx.waitUntil(
      env.REALTIME_HUB.getByName("hub").notify(acceptedEvents, sensorId)
    );
  }

  return jsonResponse(202, { accepted, duplicates }, origin);
}

interface SessionPayload {
  session_id?: string;
  started_at?: number;
  ended_at?: number;
  duration_seconds?: number;
  band?: string | null;
  last_signal?: number | null;
  last_noise?: number | null;
}

function applyTemporalSideEffects(
  env: Env,
  stmts: D1PreparedStatement[],
  sensorId: string,
  type: string,
  ts: number,
  now: number,
  deviceId: string,
  rawPayload: unknown
): void {
  const p = (rawPayload && typeof rawPayload === "object" ? rawPayload : {}) as Record<string, any>;

  const stateUpdate = (state: string, extra: Record<string, any> = {}) => {
    stmts.push(
      env.DB.prepare(
        `INSERT INTO device_state
         (sensor_id, device_id, state, last_signal, noise, band, interface,
          current_session_id, first_seen, last_seen, total_connected_time,
          connection_count, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(sensor_id, device_id) DO UPDATE SET
           state = COALESCE(excluded.state, device_state.state),
           last_signal = COALESCE(excluded.last_signal, device_state.last_signal),
           noise = COALESCE(excluded.noise, device_state.noise),
           band = COALESCE(excluded.band, device_state.band),
           interface = COALESCE(excluded.interface, device_state.interface),
           current_session_id = COALESCE(excluded.current_session_id, device_state.current_session_id),
           first_seen = COALESCE(device_state.first_seen, excluded.first_seen),
           last_seen = excluded.last_seen,
           total_connected_time = device_state.total_connected_time + excluded.total_connected_time,
           connection_count = device_state.connection_count + excluded.connection_count,
           updated_at = excluded.updated_at`
      ).bind(
        sensorId,
        deviceId,
        state,
        extra.last_signal ?? null,
        extra.noise ?? null,
        extra.band ?? null,
        extra.interface ?? null,
        extra.current_session_id ?? null,
        ts,
        ts,
        extra.total_connected_time ?? 0,
        extra.connection_count ?? 0,
        now
      )
    );
  };

  switch (type) {
    case "device.connected": {
      const sess = typeof p.session_id === "string" ? p.session_id : null;
      stateUpdate("CONNECTED", {
        last_signal: numOrNull(p.rssi ?? p.signal),
        noise: numOrNull(p.noise),
        band: strOrNull(p.band),
        current_session_id: sess,
        connection_count: typeof p.connection_count === "number" ? 1 : 0,
      });
      if (sess) {
        stmts.push(
          env.DB.prepare(
            `INSERT OR REPLACE INTO device_sessions
             (session_id, sensor_id, device_id, started_at, ended_at, duration_seconds, band, last_signal, last_noise, received_at)
             VALUES (?, ?, ?, ?, NULL, NULL, ?, ?, ?, ?)`
          ).bind(sess, sensorId, deviceId, ts, strOrNull(p.band), numOrNull(p.rssi ?? p.signal), numOrNull(p.noise), now)
        );
      }
      break;
    }
    case "device.disconnected": {
      stateUpdate("DISCONNECTED", {
        total_connected_time: typeof p.duration_seconds === "number" ? p.duration_seconds : 0,
      });
      if (typeof p.session_id === "string") {
        stmts.push(
          env.DB.prepare(
            `INSERT INTO device_sessions
             (session_id, sensor_id, device_id, started_at, ended_at, duration_seconds, band, last_signal, last_noise, received_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(session_id) DO UPDATE SET
               ended_at = excluded.ended_at,
               duration_seconds = excluded.duration_seconds,
               last_signal = COALESCE(excluded.last_signal, device_sessions.last_signal),
               last_noise = COALESCE(excluded.last_noise, device_sessions.last_noise),
               received_at = excluded.received_at`
          ).bind(
            p.session_id,
            sensorId,
            deviceId,
            typeof p.started_at === "number" ? p.started_at : ts,
            typeof p.ended_at === "number" ? p.ended_at : ts,
            typeof p.duration_seconds === "number" ? p.duration_seconds : null,
            strOrNull(p.band),
            numOrNull(p.last_signal),
            numOrNull(p.last_noise),
            now
          )
        );
        stmts.push(
          env.DB.prepare(
            `UPDATE device_state SET current_session_id = NULL
             WHERE sensor_id = ? AND current_session_id = ?`
          ).bind(sensorId, p.session_id)
        );
      }
      break;
    }
    case "device.signal_changed": {
      stateUpdate("", {
        last_signal: numOrNull(p.new_signal),
        band: strOrNull(p.band),
      });
      break;
    }
    case "device.band_changed": {
      stateUpdate("", { band: strOrNull(p.new_band) });
      break;
    }
    case "device.network_changed": {
      stateUpdate("", { interface: strOrNull(p.new_interface) });
      break;
    }
    case "device.presence_changed": {
      if (typeof p.to_state === "string") {
        stateUpdate(p.to_state);
      }
      break;
    }
    default:
      break;
  }
}

function numOrNull(v: unknown): number | null {
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}

function strOrNull(v: unknown): string | null {
  return typeof v === "string" && v.length > 0 ? v : null;
}

function newOrValue(v: unknown): unknown {
  if (v && typeof v === "object" && !Array.isArray(v) && "new" in (v as Record<string, unknown>)) {
    return (v as Record<string, unknown>).new;
  }
  return v;
}

function applyApSideEffects(
  env: Env,
  stmts: D1PreparedStatement[],
  sensorId: string,
  type: string,
  ts: number,
  now: number,
  apId: string,
  rawPayload: unknown
): void {
  const p = (rawPayload && typeof rawPayload === "object" ? rawPayload : {}) as Record<string, any>;

  if (type === "network.detected" || type === "network.changed") {
    const signal = numOrNull(newOrValue(p.signal));
    stmts.push(
      env.DB.prepare(
        `INSERT INTO ap_state
         (sensor_id, ap_id, status, ssid, band, channel, current_signal, security,
          w_mode, extch, observation_count, first_seen, last_seen, online_since,
          updated_at, average_signal, min_signal, max_signal, rssi_variance)
         VALUES (?, ?, 'ONLINE', ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(sensor_id, ap_id) DO UPDATE SET
           status = 'ONLINE',
           ssid = COALESCE(excluded.ssid, ap_state.ssid),
           band = COALESCE(excluded.band, ap_state.band),
           channel = COALESCE(excluded.channel, ap_state.channel),
           current_signal = COALESCE(excluded.current_signal, ap_state.current_signal),
           security = COALESCE(excluded.security, ap_state.security),
           w_mode = COALESCE(excluded.w_mode, ap_state.w_mode),
           extch = COALESCE(excluded.extch, ap_state.extch),
           observation_count = ap_state.observation_count + 1,
           first_seen = COALESCE(ap_state.first_seen, excluded.first_seen),
           last_seen = excluded.last_seen,
           online_since = COALESCE(ap_state.online_since, excluded.online_since),
           updated_at = excluded.updated_at,
           min_signal = MIN(COALESCE(ap_state.min_signal, excluded.current_signal), excluded.current_signal),
           max_signal = MAX(COALESCE(ap_state.max_signal, excluded.current_signal), excluded.current_signal),
           average_signal = (COALESCE(ap_state.average_signal, 0) * ap_state.observation_count + excluded.current_signal) / (ap_state.observation_count + 1),
           rssi_variance = CASE
             WHEN ap_state.rssi_variance IS NULL THEN 0.0
             ELSE (
               ap_state.observation_count * ap_state.rssi_variance
               + (excluded.current_signal - ap_state.average_signal)
               * (excluded.current_signal - ((COALESCE(ap_state.average_signal, 0) * ap_state.observation_count + excluded.current_signal) / (ap_state.observation_count + 1)))
             ) / (ap_state.observation_count + 1)
           END`
      ).bind(
        sensorId,
        apId,
        strOrNull(newOrValue(p.ssid)),
        strOrNull(newOrValue(p.band)),
        numOrNull(newOrValue(p.channel)),
        signal,
        strOrNull(newOrValue(p.security)),
        strOrNull(newOrValue(p.w_mode)),
        strOrNull(newOrValue(p.extch)),
        ts,
        ts,
        ts,
        now,
        signal,
        signal,
        signal,
        null
      )
    );
  } else if (type === "network.disappeared") {
    stmts.push(
      env.DB.prepare(
        `UPDATE ap_state
         SET status = 'OFFLINE', last_seen = ?, online_since = NULL, updated_at = ?
         WHERE sensor_id = ? AND ap_id = ?`
      ).bind(ts, now, sensorId, apId)
    );
  }
}

function applyRfSnapshot(
  env: Env,
  stmts: D1PreparedStatement[],
  sensorId: string,
  eventId: string,
  ts: number,
  now: number,
  rawPayload: unknown
): void {
  if (!rawPayload || typeof rawPayload !== "object") return;
  const p = rawPayload as Record<string, any>;
  const top = p.top_aps && Array.isArray(p.top_aps) ? JSON.stringify(p.top_aps) : null;
  const channels = p.channel_distribution ? JSON.stringify(p.channel_distribution) : null;

  stmts.push(
    env.DB.prepare(
      `INSERT INTO rf_environment_snapshots
       (event_id, sensor_id, event_timestamp, ap_count, ap_count_2_4, ap_count_5,
        strongest_signal, weakest_signal, average_signal, rssi_variance,
        channel_distribution, top_aps, received_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
       ON CONFLICT(event_id) DO UPDATE SET
         received_at = excluded.received_at`
    ).bind(
      eventId,
      sensorId,
      ts,
      typeof p.ap_count === "number" ? p.ap_count : 0,
      typeof p.ap_count_2_4 === "number" ? p.ap_count_2_4 : 0,
      typeof p.ap_count_5 === "number" ? p.ap_count_5 : 0,
      numOrNull(p.strongest_signal),
      numOrNull(p.weakest_signal),
      numOrNull(p.average_signal),
      typeof p.rssi_variance === "number" ? p.rssi_variance : null,
      channels,
      top,
      now
    )
  );
}

async function handleDevices(
  request: Request,
  env: Env
): Promise<Response> {
  const url = new URL(request.url);
  const limit = Math.min(parseInt(url.searchParams.get("limit") || "200"), 1000);
  const origin = request.headers.get("Origin") || undefined;

  const idRows = await env.DB.prepare(
    `SELECT pseudonym, manufacturer, brand, model_guess, device_class,
            mac_type, confidence, confidence_label, bssid_manufacturer, last_seen
     FROM device_identity ORDER BY last_seen DESC LIMIT ?`
  ).bind(limit).all();

  const devRows = await env.DB.prepare(
    `SELECT d.pseudonym, d.signal_strength, d.band, d.hostname, d.operating_standard,
            d.status, d.bssid_pseudonym, c.started_at
     FROM collector_devices d JOIN collector_captures c ON d.capture_id = c.capture_id
     ORDER BY c.started_at DESC LIMIT 2000`
  ).all();

  const fpRows = await env.DB.prepare(
    `SELECT pseudonym, model, confidence FROM device_fingerprint ORDER BY confidence DESC`
  ).all();

  const fsRows = await env.DB.prepare(
    `SELECT d.pseudonym, MIN(c.started_at) AS first_seen
     FROM collector_devices d JOIN collector_captures c ON d.capture_id = c.capture_id
     GROUP BY d.pseudonym`
  ).all();

  const latest = new Map<string, any>();
  const obs = new Map<string, number>();
  const rs = new Map<string, number>();
  const rn = new Map<string, number>();
  for (const d of devRows.results as any[]) {
    if (!latest.has(d.pseudonym)) latest.set(d.pseudonym, d);
    obs.set(d.pseudonym, (obs.get(d.pseudonym) || 0) + 1);
    if (d.signal_strength != null) {
      rs.set(d.pseudonym, (rs.get(d.pseudonym) || 0) + d.signal_strength);
      rn.set(d.pseudonym, (rn.get(d.pseudonym) || 0) + 1);
    }
  }
  const fp = new Map<string, any>();
  for (const f of fpRows.results as any[]) {
    if (!fp.has(f.pseudonym)) fp.set(f.pseudonym, f);
  }
  const fs = new Map<string, number>();
  for (const r of fsRows.results as any[]) fs.set(r.pseudonym, r.first_seen);

  // Build a unified device list: start from device_identity (enriched),
  // then add any pseudonyms that exist only in collector_devices (not yet
  // identified by the identity engine).
  const seenPseudonyms = new Set<string>();
  const devices: any[] = [];

  for (const i of idRows.results as any[]) {
    seenPseudonyms.add(i.pseudonym);
    const l = latest.get(i.pseudonym) || {};
    const f = fp.get(i.pseudonym);
    const n = rn.get(i.pseudonym) || 0;
    devices.push({
      pseudonym: i.pseudonym,
      manufacturer: i.manufacturer,
      brand: i.brand,
      model_guess: i.model_guess,
      device_class: i.device_class || "Unknown",
      mac_type: i.mac_type,
      confidence: i.confidence,
      confidence_label: i.confidence_label,
      bssid_manufacturer: i.bssid_manufacturer,
      last_seen: i.last_seen ?? l.started_at ?? null,
      first_seen: fs.get(i.pseudonym) ?? l.started_at ?? null,
      hostname: l.hostname ?? null,
      band: l.band ?? null,
      operating_standard: l.operating_standard ?? null,
      status: l.status ?? null,
      bssid_pseudonym: l.bssid_pseudonym ?? null,
      signal_strength: l.signal_strength ?? null,
      avg_rssi: n ? Math.round(rs.get(i.pseudonym)! / n) : null,
      observations: obs.get(i.pseudonym) || 0,
      fingerprint_model: f?.model ?? null,
      fingerprint_confidence: f?.confidence ?? null,
    });
  }

  // Add devices from collector_devices that have no device_identity row yet
  for (const [pseudonym, l] of latest) {
    if (seenPseudonyms.has(pseudonym)) continue;
    seenPseudonyms.add(pseudonym);
    const n = rn.get(pseudonym) || 0;
    devices.push({
      pseudonym,
      manufacturer: null,
      brand: null,
      model_guess: null,
      device_class: "Unknown",
      mac_type: null,
      confidence: null,
      confidence_label: null,
      bssid_manufacturer: null,
      last_seen: l.started_at ?? null,
      first_seen: fs.get(pseudonym) ?? l.started_at ?? null,
      hostname: l.hostname ?? null,
      band: l.band ?? null,
      operating_standard: l.operating_standard ?? null,
      status: l.status ?? null,
      bssid_pseudonym: l.bssid_pseudonym ?? null,
      signal_strength: l.signal_strength ?? null,
      avg_rssi: n ? Math.round(rs.get(pseudonym)! / n) : null,
      observations: obs.get(pseudonym) || 0,
      fingerprint_model: null,
      fingerprint_confidence: null,
    });
  }

  return jsonResponse(200, { devices }, origin);
}

async function handlePresence(
  request: Request,
  env: Env
): Promise<Response> {
  const url = new URL(request.url);
  const hours = Math.min(parseInt(url.searchParams.get("hours") || "24"), 168);
  const cutoff = Math.floor(Date.now() / 1000) - hours * 3600;
  const origin = request.headers.get("Origin") || undefined;

  // Query collector_devices directly (not device_identity) so devices
  // appear even before the identity engine has enriched them.
  const { results } = await env.DB.prepare(
    `SELECT d.pseudonym,
            COUNT(DISTINCT d.capture_id) AS observations,
            MAX(c.started_at) AS last_seen,
            MIN(c.started_at) AS first_seen,
            AVG(d.signal_strength) AS avg_signal,
            MAX(d.band) AS band,
            MAX(d.hostname) AS hostname,
            MAX(d.status) AS status
     FROM collector_devices d
     JOIN collector_captures c ON d.capture_id = c.capture_id
     WHERE c.started_at >= ?
     GROUP BY d.pseudonym ORDER BY last_seen DESC LIMIT 500`
  )
    .bind(cutoff)
    .all();

  return jsonResponse(200, { hours, devices: results }, origin);
}

async function handleSensors(
  _request: Request,
  env: Env
): Promise<Response> {
  const ap = await env.DB.prepare(
    `SELECT sensor_id AS id,
            MAX(updated_at) AS last_seen,
            COUNT(*) AS ap_count
     FROM ap_state GROUP BY sensor_id`
  ).all();

  const dev = await env.DB.prepare(
    `SELECT sensor_id, COUNT(DISTINCT device_id) AS distinct_devices,
            COUNT(*) AS total_devices,
            MAX(updated_at) AS last_seen
     FROM device_state GROUP BY sensor_id`
  ).all();

  const bySensor = new Map<string, any>();
  for (const r of (ap.results as any[])) bySensor.set(r.id, r);
  for (const r of (dev.results as any[])) {
    const existing = bySensor.get(r.sensor_id) || { last_seen: 0, ap_count: 0 };
    bySensor.set(r.sensor_id, {
      ...existing,
      distinct_devices: r.distinct_devices,
      total_devices: r.total_devices,
      last_seen: Math.max(existing.last_seen || 0, r.last_seen || 0),
    });
  }

  const sensors = Array.from(bySensor.entries()).map(([id, s]) => ({
    id,
    last_seen: s.last_seen,
    ap_count: s.ap_count || 0,
    distinct_devices: s.distinct_devices || 0,
    total_devices: s.total_devices || 0,
  })).sort((a, b) => (b.last_seen || 0) - (a.last_seen || 0));

  return jsonResponse(200, { sensors });
}

async function handleStats(
  _request: Request,
  env: Env
): Promise<Response> {
  const now = Math.floor(Date.now() / 1000);
  const hourAgo = now - 3600;
  const dayAgo = now - 86400;
  const { results } = await env.DB.prepare(
    `SELECT
      (SELECT COUNT(DISTINCT pseudonym) FROM collector_devices) AS distinct_devices,
      (SELECT COUNT(*) FROM collector_devices) AS total_detections,
      (SELECT COUNT(*) FROM collector_captures) AS total_snapshots,
      (SELECT COUNT(*) FROM collector_captures WHERE started_at >= ?) AS snapshots_last_hour,
      (SELECT COUNT(*) FROM collector_captures WHERE started_at >= ?) AS snapshots_last_day,
      (SELECT COUNT(DISTINCT sensor_id) FROM collector_captures) AS total_sensors,
      (SELECT COUNT(*) FROM device_identity WHERE mac_type = 'randomized') AS randomized_macs,
      (SELECT COUNT(*) FROM device_identity WHERE device_class IS NOT NULL AND device_class <> 'Unknown') AS identified_devices,
      (SELECT COUNT(DISTINCT manufacturer) FROM device_identity WHERE manufacturer IS NOT NULL) AS known_vendors,
      (SELECT ROUND(AVG(signal_strength)) FROM collector_devices WHERE signal_strength IS NOT NULL) AS avg_rssi,
      (SELECT COUNT(*) FROM wifi_network_observation) AS total_networks`
  )
    .bind(hourAgo, dayAgo)
    .all();

  return jsonResponse(200, results[0] || {});
}

async function handleHealthz(
  _request: Request,
  _env: Env
): Promise<Response> {
  return jsonResponse(200, {
    status: "ok",
    version: "0.2.0-cf-worker",
    runtime: "cloudflare-workers",
    timestamp: Math.floor(Date.now() / 1000),
  });
}

async function handleReadyz(
  _request: Request,
  env: Env
): Promise<Response> {
  try {
    await env.DB.prepare("SELECT 1").first();
    return jsonResponse(200, { status: "ready" });
  } catch {
    return jsonResponse(503, { status: "not ready" });
  }
}

async function handleNetworks(
  request: Request,
  env: Env
): Promise<Response> {
  const url = new URL(request.url);
  const sensorId = url.searchParams.get("sensor_id");
  const hours = Math.min(parseInt(url.searchParams.get("hours") || "24"), 168);
  const cutoff = Math.floor(Date.now() / 1000) - hours * 3600;
  const origin = request.headers.get("Origin") || undefined;

  const apConds: string[] = ["last_seen >= ?"];
  const apBinds: (string | number)[] = [cutoff];
  if (sensorId && sensorId !== "all") {
    apConds.push("sensor_id = ?");
    apBinds.push(sensorId);
  }
  const { results } = await env.DB.prepare(
    `SELECT sensor_id, ap_id, status, ssid, band, channel, current_signal, average_signal,
            min_signal, max_signal, rssi_variance, observation_count, session_count,
            first_seen, last_seen, online_since, security, w_mode, extch
     FROM ap_state WHERE ${apConds.join(" AND ")}
     ORDER BY last_seen DESC LIMIT 500`
  )
    .bind(...apBinds)
    .all();

  const snapConds: string[] = ["event_timestamp >= ?"];
  const snapBinds: (string | number)[] = [cutoff];
  if (sensorId && sensorId !== "all") {
    snapConds.push("sensor_id = ?");
    snapBinds.push(sensorId);
  }
  const { results: snapResults } = await env.DB.prepare(
    `SELECT event_id, sensor_id, event_timestamp, ap_count, ap_count_2_4, ap_count_5,
            strongest_signal, weakest_signal, average_signal, rssi_variance,
            channel_distribution, top_aps
     FROM rf_environment_snapshots WHERE ${snapConds.join(" AND ")}
     ORDER BY event_timestamp DESC LIMIT 100`
  )
    .bind(...snapBinds)
    .all();

  return jsonResponse(200, { hours, sensor_id: sensorId, aps: results, rf_snapshots: snapResults }, origin);
}

async function handleFusion(
  request: Request,
  env: Env
): Promise<Response> {
  const url = new URL(request.url);
  const ssid = url.searchParams.get("ssid");
  const band = url.searchParams.get("band");
  const channel = url.searchParams.get("channel");
  const apId = url.searchParams.get("ap_id");
  const hours = Math.min(parseInt(url.searchParams.get("hours") || "24"), 168);
  const cutoff = Math.floor(Date.now() / 1000) - hours * 3600;
  const origin = request.headers.get("Origin") || undefined;

  if (!ssid && !apId) {
    return jsonResponse(400, { error: "missing ssid or ap_id" }, origin);
  }

  const conds: string[] = ["last_seen >= ?"];
  const binds: (string | number)[] = [cutoff];
  if (ssid) {
    conds.push("ssid = ?");
    binds.push(ssid);
  }
  if (band) {
    conds.push("band = ?");
    binds.push(band);
  }
  if (channel) {
    conds.push("channel = ?");
    binds.push(channel);
  }
  if (apId) {
    conds.push("ap_id = ?");
    binds.push(apId);
  }

  const { results } = await env.DB.prepare(
    `SELECT sensor_id, ap_id, status, ssid, band, channel, current_signal, average_signal,
            min_signal, max_signal, security, w_mode, extch, first_seen, last_seen
     FROM ap_state WHERE ${conds.join(" AND ")}
     ORDER BY current_signal DESC, last_seen DESC`
  )
    .bind(...binds)
    .all();

  const sensorMap = new Map<string, any[]>();
  for (const r of results as any[]) {
    if (!sensorMap.has(r.sensor_id)) sensorMap.set(r.sensor_id, []);
    sensorMap.get(r.sensor_id)!.push(r);
  }
  const sensors = Array.from(sensorMap.entries()).map(([id, rows]) => ({
    id,
    best_signal: rows[0].current_signal,
    ap_count: rows.length,
    aps: rows,
  })).sort((a, b) => (b.best_signal || 0) - (a.best_signal || 0));

  return jsonResponse(200, { hours, ssid, band, channel, ap_id: apId, sensors }, origin);
}

async function handleDeviceState(
  request: Request,
  env: Env
): Promise<Response> {
  const url = new URL(request.url);
  const sensorId = url.searchParams.get("sensor_id");
  const origin = request.headers.get("Origin") || undefined;

  let query = `SELECT device_id, state, last_signal, noise, band, interface,
                      current_session_id, first_seen, last_seen,
                      total_connected_time, connection_count, updated_at
               FROM device_state`;
  const binds: string[] = [];
  if (sensorId) {
    query += ` WHERE sensor_id = ?`;
    binds.push(sensorId);
  }
  query += ` ORDER BY last_seen DESC LIMIT 500`;

  const stmt = binds.length
    ? env.DB.prepare(query).bind(...binds)
    : env.DB.prepare(query);
  const { results } = await stmt.all();
  return jsonResponse(200, { devices: results }, origin);
}

async function handleSessions(
  request: Request,
  env: Env
): Promise<Response> {
  const url = new URL(request.url);
  const hours = Math.min(parseInt(url.searchParams.get("hours") || "168"), 720);
  const deviceId = url.searchParams.get("device_id");
  const cutoff = Math.floor(Date.now() / 1000) - hours * 3600;
  const origin = request.headers.get("Origin") || undefined;

  const conds: string[] = [`started_at >= ?`];
  const binds: (string | number)[] = [cutoff];
  if (deviceId) {
    conds.push(`device_id = ?`);
    binds.push(deviceId);
  }
  const { results } = await env.DB.prepare(
    `SELECT session_id, sensor_id, device_id, started_at, ended_at,
            duration_seconds, band, last_signal, last_noise
     FROM device_sessions WHERE ${conds.join(" AND ")}
     ORDER BY started_at DESC LIMIT 500`
  )
    .bind(...binds)
    .all();
  return jsonResponse(200, { hours, sessions: results }, origin);
}

async function handleEvents(
  request: Request,
  env: Env
): Promise<Response> {
  const url = new URL(request.url);
  const origin = request.headers.get("Origin") || undefined;
  const sensorId = url.searchParams.get("sensor_id");
  const deviceId = url.searchParams.get("device_id");
  const eventType = url.searchParams.get("event_type");
  const afterEventId = url.searchParams.get("after_event_id");
  const since = parseInt(url.searchParams.get("since") || "0");
  const hours = parseInt(url.searchParams.get("hours") || "24");
  const limit = Math.min(parseInt(url.searchParams.get("limit") || "100"), 1000);

  const conds: string[] = [];
  const binds: (string | number)[] = [];

  const afterIdNum = async () => {
    if (!afterEventId) return 0;
    const row = await env.DB.prepare("SELECT id FROM events WHERE event_id = ?").bind(afterEventId).first<{ id: number }>();
    return row?.id ?? 0;
  };

  if (afterEventId) {
    const afterId = await afterIdNum();
    conds.push("e.id > ?");
    binds.push(afterId);
  } else if (since) {
    conds.push("e.event_timestamp >= ?");
    binds.push(since);
  } else {
    const cutoff = Math.floor(Date.now() / 1000) - Math.min(hours, 720) * 3600;
    conds.push("e.event_timestamp >= ?");
    binds.push(cutoff);
  }
  if (sensorId) { conds.push("e.sensor_id = ?"); binds.push(sensorId); }
  if (deviceId) { conds.push("e.device_id = ?"); binds.push(deviceId); }
  if (eventType) { conds.push("e.event_type = ?"); binds.push(eventType); }

  const { results } = await env.DB.prepare(
    `SELECT e.id, e.sensor_id, e.event_id, e.event_type, e.event_timestamp,
            e.device_id, e.payload_json, e.snapshot_json, e.sequence, e.received_at
     FROM events e
     WHERE ${conds.join(" AND ")}
     ORDER BY e.id ASC
     LIMIT ?`
  ).bind(...binds, limit).all();
  return jsonResponse(200, { events: results, after_event_id: afterEventId, limit }, origin);
}

async function handleDeviceEvents(
  request: Request,
  env: Env
): Promise<Response> {
  const path = new URL(request.url).pathname;
  const m = path.match(/^\/api\/v1\/devices\/([^/]+)\/events$/);
  const deviceId = m ? decodeURIComponent(m[1]) : null;
  if (!deviceId) return jsonResponse(404, { error: "not found" });
  const url = new URL(request.url);
  url.searchParams.set("device_id", deviceId);
  return handleEvents(new Request(url.toString(), { method: "GET", headers: request.headers }), env);
}

async function handleDeviceSessions(
  request: Request,
  env: Env
): Promise<Response> {
  const path = new URL(request.url).pathname;
  const m = path.match(/^\/api\/v1\/devices\/([^/]+)\/sessions$/);
  const deviceId = m ? decodeURIComponent(m[1]) : null;
  if (!deviceId) return jsonResponse(404, { error: "not found" });
  const url = new URL(request.url);
  url.searchParams.set("device_id", deviceId);
  const request2 = new Request(url.toString(), { method: "GET", headers: request.headers });
  return handleSessions(request2, env);
}

async function handleDeviceSignals(
  request: Request,
  env: Env
): Promise<Response> {
  const path = new URL(request.url).pathname;
  const m = path.match(/^\/api\/v1\/devices\/([^/]+)\/signals$/);
  const deviceId = m ? decodeURIComponent(m[1]) : null;
  if (!deviceId) return jsonResponse(404, { error: "not found" });
  const url = new URL(request.url);
  const origin = request.headers.get("Origin") || undefined;
  const hours = Math.min(parseInt(url.searchParams.get("hours") || "24"), 720);
  const cutoff = Math.floor(Date.now() / 1000) - hours * 3600;
  const { results } = await env.DB.prepare(
    `SELECT d.pseudonym, d.rssi, d.source, d.standard, d.radio_mac,
            s.received_at AS ts
     FROM detections d JOIN snapshots s ON d.snapshot_id = s.id
     WHERE d.pseudonym = ? AND s.received_at >= ?
     ORDER BY d.id DESC
     LIMIT 500`
  ).bind(deviceId, cutoff).all();
  return jsonResponse(200, { device_id: deviceId, hours, signals: results }, origin);
}

async function handleTimeline(
  request: Request,
  env: Env
): Promise<Response> {
  const url = new URL(request.url);
  const hours = Math.min(parseInt(url.searchParams.get("hours") || "24"), 168);
  const cutoff = Math.floor(Date.now() / 1000) - hours * 3600;
  const origin = request.headers.get("Origin") || undefined;

  // Get device observations with timestamps for charting
  const { results } = await env.DB.prepare(
    `SELECT d.pseudonym, d.signal_strength AS rssi, d.band, d.bssid_pseudonym,
            c.started_at AS ts
     FROM collector_devices d JOIN collector_captures c ON d.capture_id = c.capture_id
     WHERE c.started_at >= ?
     ORDER BY ts ASC LIMIT 3000`
  )
    .bind(cutoff)
    .all();

  return jsonResponse(200, { hours, points: results }, origin);
}

// ---------------------------------------------------------------------------
// Collector sync (authenticated HMAC, matches autonomous/collector.py)
// ---------------------------------------------------------------------------

interface CollectorCapture {
  capture_id?: string;
  run_id?: string;
  sensor_id?: string;
  scheduled_at?: number;
  started_at?: number;
  completed_at?: number | null;
  status?: string;
  api_latency_ms?: number | null;
  auth_latency_ms?: number | null;
  device_count?: number | null;
  active_device_count?: number | null;
  payload_hash?: string | null;
  created_at?: number;
}

interface CollectorDevice {
  pseudonym?: string;
  hostname?: string | null;
  band?: string | null;
  signal_strength?: number | null;
  signal_level?: number | null;
  noise?: number | null;
  operating_standard?: string | null;
  tx_rate_kbps?: number | null;
  rx_rate_kbps?: number | null;
  status?: string | null;
  bssid_pseudonym?: string | null;
  identity?: any | null;
}

async function handleCollectorSync(
  request: Request,
  env: Env
): Promise<Response> {
  const origin = request.headers.get("Origin") || undefined;
  const sensorId = request.headers.get("X-Detectic-Sensor") || "";
  const signature = request.headers.get("X-Detectic-Signature") || "";
  const timestamp = request.headers.get("X-Detectic-Timestamp");
  const bodyText = await request.text();

  if (bodyText.length > 4 * 1024 * 1024) {
    return jsonResponse(400, { error: "body too large" }, origin);
  }
  if (!(await verifyAuth(env, sensorId, signature, bodyText, timestamp))) {
    return jsonResponse(401, { error: "unauthorized" }, origin);
  }

  let payload: any;
  try {
    payload = JSON.parse(bodyText);
  } catch {
    return jsonResponse(400, { error: "invalid json" }, origin);
  }

  const results: { captures: number; devices: number; runs: number } = {
    captures: 0,
    devices: 0,
    runs: 0,
  };

  const sensor = payload.captures?.[0]?.sensor_id || sensorId;
  const now = Math.floor(Date.now() / 1000);

  // --- captures (idempotent upsert) ---
  if (payload.captures && Array.isArray(payload.captures)) {
    const stmts = payload.captures.map((c: CollectorCapture) =>
      env.DB.prepare(
        `INSERT OR REPLACE INTO collector_captures
         (capture_id, run_id, sensor_id, scheduled_at, started_at, completed_at,
          status, api_latency_ms, auth_latency_ms, device_count, active_device_count,
          payload_hash, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`
      ).bind(
        c.capture_id, c.run_id ?? "", c.sensor_id ?? sensor,
        c.scheduled_at ?? 0, c.started_at ?? 0, c.completed_at ?? null,
        c.status ?? "UNKNOWN", c.api_latency_ms ?? null, c.auth_latency_ms ?? null,
        c.device_count ?? null, c.active_device_count ?? null,
        c.payload_hash ?? null, c.created_at ?? now
      )
    );
    await env.DB.batch(stmts);
    results.captures = payload.captures.length;
  }

  // --- devices (idempotent: replace per capture) ---
  if (payload.devices) {
    for (const [captureId, devs] of Object.entries(payload.devices)) {
      if (!Array.isArray(devs) || devs.length === 0) continue;
      const dlist = devs as CollectorDevice[];
      const idStmts: any[] = [];
      // Clear previous rows for this capture (deterministic capture_id).
      idStmts.push(
        env.DB.prepare(`DELETE FROM collector_devices WHERE capture_id = ?`).bind(captureId)
      );
      for (const d of dlist) {
        idStmts.push(
          env.DB.prepare(
            `INSERT INTO collector_devices
             (capture_id, pseudonym, hostname, band, signal_strength, signal_level,
              noise, operating_standard, tx_rate_kbps, rx_rate_kbps, status,
              bssid_pseudonym, identity_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`
          ).bind(
            captureId, d.pseudonym ?? "", d.hostname ?? null, d.band ?? null,
            d.signal_strength ?? null, d.signal_level ?? null, d.noise ?? null,
            d.operating_standard ?? null, d.tx_rate_kbps ?? null, d.rx_rate_kbps ?? null,
            d.status ?? null, d.bssid_pseudonym ?? null,
            d.identity ? JSON.stringify(d.identity) : null
          )
        );
      }
      await env.DB.batch(idStmts);
      results.devices += dlist.length;

      // --- derived identity / network fingerprint upserts ---
      const fStmts: any[] = [];
      for (const d of dlist) {
        if (!d.identity) continue;
        const id = d.identity;
        fStmts.push(
          env.DB.prepare(
            `INSERT INTO device_identity
             (pseudonym, sensor_id, manufacturer, brand, model_guess, device_class,
              mac_type, confidence, confidence_label, bssid_manufacturer, identity_json, last_seen)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(pseudonym, sensor_id) DO UPDATE SET
               manufacturer=excluded.manufacturer, brand=excluded.brand,
               model_guess=excluded.model_guess, device_class=excluded.device_class,
               mac_type=excluded.mac_type, confidence=excluded.confidence,
               confidence_label=excluded.confidence_label,
               bssid_manufacturer=excluded.bssid_manufacturer,
               identity_json=excluded.identity_json, last_seen=excluded.last_seen`
          ).bind(
            d.pseudonym, sensor, id.manufacturer ?? null, id.brand ?? null,
            id.model_guess ?? null, id.device_class ?? null, id.mac_type ?? null,
            id.confidence ?? null, id.confidence_label ?? null,
            id.bssid_manufacturer ?? null, JSON.stringify(id), now
          )
        );
        if (Array.isArray(id.evidence)) {
          for (const ev of id.evidence) {
            fStmts.push(
              env.DB.prepare(
                `INSERT INTO identity_evidence
                 (pseudonym, sensor_id, evidence_type, description, weight, captured_at)
                 VALUES (?, ?, ?, ?, ?, ?)`
              ).bind(
                d.pseudonym, sensor, ev?.type ?? null, ev?.description ?? null,
                ev?.weight ?? null, now
              )
            );
          }
        }
        if (Array.isArray(id.candidates)) {
          for (const c of id.candidates) {
            fStmts.push(
              env.DB.prepare(
                `INSERT INTO device_fingerprint
                 (pseudonym, model, confidence, evidence)
                 VALUES (?, ?, ?, ?)
                 ON CONFLICT(pseudonym, model) DO UPDATE SET
                   confidence=excluded.confidence, evidence=excluded.evidence`
              ).bind(d.pseudonym, c?.model ?? c?.name ?? null, c?.confidence ?? null,
                     JSON.stringify(c))
            );
          }
        }
        if (d.bssid_pseudonym && id.bssid_manufacturer) {
          fStmts.push(
            env.DB.prepare(
              `INSERT INTO wifi_network_observation
               (bssid_pseudonym, ssid, manufacturer, band, first_seen, last_seen, observation_count, sensor_id)
               VALUES (?, ?, ?, ?, ?, ?, 1, ?)
               ON CONFLICT(bssid_pseudonym, sensor_id) DO UPDATE SET
                 manufacturer=excluded.manufacturer, band=excluded.band,
                 last_seen=excluded.last_seen, observation_count=observation_count+1`
            ).bind(
              d.bssid_pseudonym, null, id.bssid_manufacturer, d.band ?? null,
              now, now, sensor
            )
          );
        }
      }
      if (fStmts.length) await env.DB.batch(fStmts);
    }
  }

  // --- runs ---
  if (payload.runs && Array.isArray(payload.runs)) {
    const stmts = payload.runs.map((r: any) =>
      env.DB.prepare(
        `INSERT OR REPLACE INTO collector_runs
         (run_id, scheduled_at, started_at, completed_at, status, duration_ms)
         VALUES (?, ?, ?, ?, ?, ?)`
      ).bind(
        r.run_id ?? "", r.scheduled_at ?? 0, r.started_at ?? 0,
        r.completed_at ?? null, r.status ?? "", r.duration_ms ?? null
      )
    );
    await env.DB.batch(stmts);
    results.runs = payload.runs.length;
  }

  return jsonResponse(200, { synced: true, ...results }, origin);
}

async function fetchRealtimeSummary(env: Env, hours: number): Promise<any> {
  const id = env.REALTIME_HUB.idFromName('hub');
  const stub = env.REALTIME_HUB.get(id);
  const r = await stub.fetch(new Request(`https://internal/summary?hours=${hours}`, { method: 'GET' }));
  if (!r.ok) return { devices: [] };
  return r.json();
}

async function fetchRealtimeNetworks(env: Env, hours: number): Promise<any> {
  const id = env.REALTIME_HUB.idFromName('hub');
  const stub = env.REALTIME_HUB.get(id);
  const r = await stub.fetch(new Request(`https://internal/networks?hours=${hours}`, { method: 'GET' }));
  if (!r.ok) return { networks: [] };
  return r.json();
}

async function handleReportsDevices(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  const hours = Math.min(parseInt(url.searchParams.get('hours') || '24'), 720);
  const origin = request.headers.get('Origin') || undefined;
  const data = await fetchRealtimeSummary(env, hours);
  return jsonResponse(200, { ...data, hours }, origin);
}

function msToDuration(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ${m % 60}m`;
  const d = Math.floor(h / 24);
  return `${d}d ${h % 24}h`;
}

function fmtDateTime(ts: number): string {
  return new Date(ts).toLocaleString('pt-BR', {
    timeZone: 'America/Sao_Paulo',
    day: '2-digit', month: '2-digit', year: 'numeric',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
    timeZoneName: 'short',
  });
}

function signalLevel(rssi?: number | null): number {
  if (rssi == null) return 0;
  if (rssi >= -50) return 4;
  if (rssi >= -60) return 3;
  if (rssi >= -70) return 2;
  if (rssi >= -80) return 1;
  return 0;
}

function signalLabel(level: number): string {
  const labels = ['Sin señal', 'Débil', 'Regular', 'Buena', 'Excelente'];
  return labels[level] || 'Sin señal';
}

function signalBars(level: number): string {
  return '🟢'.repeat(level) + '⚪'.repeat(4 - level);
}

function distanceLabel(rssi?: number | null): string {
  if (rssi == null) return '~desconocido';
  if (rssi >= -50) return '~muy cerca';
  if (rssi >= -60) return '~cerca';
  if (rssi >= -70) return '~a cierta distancia';
  if (rssi >= -80) return '~lejos';
  return '~muy lejos';
}

function deviceNameFrom(row: any, fallback: string): string {
  if (row?.hostname) return row.hostname;
  const parts = [row?.manufacturer, row?.brand, row?.model_guess, row?.device_class, row?.operating_standard].filter(Boolean);
  if (parts.length) return parts.join(' ');
  return fallback;
}

function deviceDetails(row: any): string {
  const parts: string[] = [];
  if (row?.hostname) parts.push(`host: ${row.hostname}`);
  if (row?.manufacturer) parts.push(row.manufacturer);
  if (row?.brand) parts.push(row.brand);
  if (row?.model_guess) parts.push(row.model_guess);
  if (row?.device_class) parts.push(row.device_class);
  if (row?.operating_standard) parts.push(row.operating_standard);
  return parts.join(' · ') || 'sin datos adicionales';
}

async function handleEmailReport(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  const hours = Math.min(parseInt(url.searchParams.get('hours') || '24'), 720);
  const origin = request.headers.get('Origin') || undefined;
  const startMs = Date.now();

  const data = await fetchRealtimeSummary(env, hours);
  const now = new Date();
  const scheduled = new Date(Math.floor(now.getTime() / 300000) * 300000); // round to 5 min
  const captureStart = new Date(data.generated_at || now.getTime());
  const captureEnd = now;

  const [idRows, devRows, networkData, apRows] = await Promise.all([
    env.DB.prepare('SELECT pseudonym, manufacturer, brand, model_guess, device_class, last_seen FROM device_identity').all(),
    env.DB.prepare(`SELECT d.pseudonym, d.hostname, d.operating_standard, d.identity_json, c.started_at
                     FROM collector_devices d
                     JOIN collector_captures c ON d.capture_id = c.capture_id
                     ORDER BY c.started_at DESC`).all(),
    fetchRealtimeNetworks(env, hours),
    env.DB.prepare(`SELECT ssid, band, current_signal, status, sensor_id, first_seen, last_seen, online_since,
                           observation_count, w_mode, security
                    FROM ap_state
                    WHERE last_seen >= ?
                    ORDER BY last_seen DESC LIMIT 50`)
      .bind(Math.floor(Date.now() / 1000) - 24 * 3600)
      .all(),
  ]);

  const apiMs = Date.now() - startMs;
  const identityMap = new Map<string, any>();
  for (const r of idRows.results as any[]) identityMap.set(r.pseudonym, r);
  for (const r of devRows.results as any[]) {
    let identity: any = {};
    if (r.identity_json) {
      try { identity = JSON.parse(r.identity_json); } catch { /* ignore */ }
    }
    if (!identityMap.has(r.pseudonym)) identityMap.set(r.pseudonym, { ...r, ...identity });
    else {
      const existing = identityMap.get(r.pseudonym);
      if (r.hostname) existing.hostname = r.hostname;
      if (r.operating_standard && !existing.operating_standard) existing.operating_standard = r.operating_standard;
      for (const k of ['manufacturer', 'brand', 'model_guess', 'device_class']) {
        if (identity[k] && !existing[k]) existing[k] = identity[k];
      }
    }
  }

  const connected = (data.devices || []).filter((d: any) => d.connected);
  const outOfRange = (data.devices || []).filter((d: any) => !d.connected);
  const detectedCount = (data.devices || []).length;
  const connectedCount = connected.length;
  const offCount = outOfRange.length;

  connected.sort((a: any, b: any) => {
    const nameA = deviceNameFrom({ ...(identityMap.get(a.device_id) || {}), ...a }, a.device_id.slice(0, 16));
    const nameB = deviceNameFrom({ ...(identityMap.get(b.device_id) || {}), ...b }, b.device_id.slice(0, 16));
    return nameA.localeCompare(nameB, undefined, { sensitivity: 'base' });
  });
  outOfRange.sort((a: any, b: any) => {
    const nameA = deviceNameFrom({ ...(identityMap.get(a.device_id) || {}), ...a }, a.device_id.slice(0, 16));
    const nameB = deviceNameFrom({ ...(identityMap.get(b.device_id) || {}), ...b }, b.device_id.slice(0, 16));
    return nameA.localeCompare(nameB, undefined, { sensitivity: 'base' });
  });

  const connectedRows = connected.map((d: any) => {
    const id = { ...(identityMap.get(d.device_id) || {}), ...d };
    const name = deviceNameFrom(id, d.device_id.slice(0, 16));
    const details = deviceDetails(id);
    const level = signalLevel(d.last_signal);
    return `<div style="margin-bottom:14px;padding:10px;border:1px solid #d0d7de;border-radius:8px;background:#fff">
      <b>${escHtml(name)}</b>
      <div style="font-size:11px;color:#57606a">${escHtml(details)}</div>
      <div style="font-size:12px;margin:4px 0">📶 ${signalBars(level)} ${signalLabel(level)} (nivel ${level}/4)</div>
      <div style="font-size:12px;color:#57606a">📡 ${d.band || '—'}</div>
      <div style="font-size:12px;color:#57606a">📍 ${distanceLabel(d.last_signal)} · última ${fmtDateTime(d.last_seen)} · total ${msToDuration(d.last_seen - d.first_seen)}</div>
    </div>`;
  }).join('') || '<p style="color:#57606a">Ningún dispositivo conectado en el período.</p>';

  const outRows = outOfRange.map((d: any) => {
    const id = { ...(identityMap.get(d.device_id) || {}), ...d };
    const name = deviceNameFrom(id, d.device_id.slice(0, 16));
    const details = deviceDetails(id);
    return `<div style="margin-bottom:10px;padding:8px;border:1px solid #d0d7de;border-radius:6px;background:#fff">
      <b>${escHtml(name)}</b>
      <div style="font-size:11px;color:#57606a">${escHtml(details)}</div>
      <span style="font-size:12px;color:#57606a"> 📡 ${d.band || '—'} · 💤 desconectado · última ${fmtDateTime(d.last_seen)}</span>
    </div>`;
  }).join('') || '<p style="color:#57606a">Ningún dispositivo fuera de rango.</p>';

  const nowMs = Date.now();
  const toMs = (ts: number) => (ts < 1e12 ? ts * 1000 : ts);
  const rawNetworks = (networkData.networks?.length ? networkData.networks : apRows.results) as any[];
  const networks = rawNetworks.length ? rawNetworks : (apRows.results as any[]);

  const sensorId = (networks[0]?.sensor_id as string) ||
    (data.devices?.[0]?.sensor_id as string) ||
    'desconocido';

  networks.sort((a: any, b: any) => {
    const labelA = String(a.ssid || a.ap_id || '').toLowerCase();
    const labelB = String(b.ssid || b.ap_id || '').toLowerCase();
    return labelA.localeCompare(labelB, undefined, { sensitivity: 'base' });
  });

  const STALE_MS = 10 * 60 * 1000;

  const networkStatus = (n: any) => {
    const firstMs = toMs(n.first_seen ?? n.last_seen);
    const lastMs = toMs(n.last_seen ?? n.first_seen);
    const duration = lastMs - firstMs;
    const sinceLast = nowMs - lastMs;
    const stale = sinceLast > STALE_MS && n.status !== 'OFFLINE';
    const dot = n.status === 'OFFLINE' ? '🔴' : (stale ? '🟠' : '🟢');
    const statusText = n.status === 'OFFLINE' ? 'OFFLINE' : (stale ? 'SIN SEÑAL RECIENTE' : 'ONLINE');
    const active = n.online_since ? (lastMs - toMs(n.online_since)) : 0;
    return `<div style="margin-bottom:10px;padding:10px;border:1px solid #d0d7de;border-radius:8px;background:#fff">
      <b>${escHtml(n.ssid || n.ap_id || '—')}</b>
      <div style="font-size:12px;color:#57606a">${dot} ${statusText} · 📡 ${n.band || '—'} · ${n.w_mode || '—'} · obs: ${n.event_count ?? n.observation_count ?? 0}</div>
      <div style="font-size:11px;color:#57606a">primera detección: ${fmtDateTime(firstMs)} · última: ${fmtDateTime(lastMs)} · detectada: ${msToDuration(duration)} · desde última: ${msToDuration(sinceLast)}${n.online_since ? ` · activa: ${msToDuration(active)}` : ''}</div>
    </div>`;
  };

  const networkRows = networks.map(networkStatus).join('') || '<p style="color:#57606a">No se detectaron redes.</p>';
  const offlineRows = networks.filter((n: any) => n.status === 'OFFLINE' || (nowMs - toMs(n.last_seen) > STALE_MS && n.status !== 'OFFLINE')).map(networkStatus).join('') || '<p style="color:#57606a">Sin caídas de red registradas.</p>';

  const bands = new Set<string>();
  const standards = new Set<string>();
  for (const n of networks) {
    if (n.band) bands.add(n.band);
    if (n.w_mode) standards.add(n.w_mode);
  }

  const reportId = `detectic-${sensorId}-${scheduled.toISOString().replace(/[-:]/g, '').slice(0,15)}`;

  const html = `<!DOCTYPE html>
<html lang="es">
<head><meta charset="utf-8"><title>🛰️ DETECTIC — Informe de Observación Autónoma</title>
<style>
body{font-family:system-ui,-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;background:#f6f8fa;color:#24292f;padding:24px;line-height:1.5}
h2{color:#0969da;margin-top:28px;margin-bottom:10px;font-size:20px}
h3{font-size:15px;color:#24292f;margin:18px 0 8px}
.card{background:#fff;border:1px solid #d0d7de;border-radius:12px;padding:18px;margin-bottom:16px;box-shadow:0 1px 2px rgba(0,0,0,0.04)}
.meta{color:#57606a;font-size:13px}
.badge{display:inline-block;padding:2px 8px;border-radius:999px;font-size:12px;font-weight:600;background:#dafbe1;color:#1a7f37;margin-right:6px}
</style>
</head>
<body>
<div class="card">
  <h2>🛰️ DETECTIC — Informe de Observación Autónoma</h2>
  <div class="meta"><b>Sensor:</b> ${escHtml(sensorId)}</div>
  <div class="meta"><b>Programado:</b> ${fmtDateTime(scheduled.getTime())}</div>
  <div class="meta"><b>Captura:</b> ${fmtDateTime(captureStart.getTime())} → ${fmtDateTime(captureEnd.getTime())}</div>
  <br>
  <div>📊 <b>Resumen:</b> ${detectedCount} dispositivos detectados, <span class="badge">${connectedCount} conectados</span> · <span style="color:#cf222e">😴 ${offCount} fuera de rango</span></div>
  <div class="meta">⚡ Estado: <b>PERSISTIDO</b> · API: ${apiMs} ms · Reporte: ${reportId}</div>
</div>

<div class="card">
  <h3>📱 Dispositivos Conectados</h3>
  ${connectedRows}
</div>

<div class="card">
  <h3>😴 Dispositivos Fuera de Rango</h3>
  ${outRows}
</div>

<div class="card">
  <h3>📶 Leyenda de Señal</h3>
  <div style="font-size:13px">
    <div>🟢🟢🟢🟢 Excelente (nivel 4 — dispositivo muy cerca del sensor)</div>
    <div>🟢🟢🟢⚪ Buena (nivel 3 — dispositivo cerca)</div>
    <div>🟢🟢⚪⚪ Regular (nivel 2 — señal aceptable)</div>
    <div>🟢⚪⚪⚪ Débil (nivel 1 — puede perderse)</div>
    <div>⚪⚪⚪⚪ Sin señal (nivel 0 — sin conexión estable)</div>
  </div>
  <p style="font-size:12px;color:#57606a">La distancia es una estimación basada en la señal RF. Varía según paredes, muebles, personas y obstáculos.</p>
</div>

<div class="card">
  <h3>🌐 Redes Wi-Fi Detectadas</h3>
  ${networkRows}
</div>

<div class="card">
  <h3>🔴 Caídas / Desconexiones de red</h3>
  ${offlineRows}
</div>

<div class="card">
  <h3>🖥️ Redes Observadas</h3>
  <div class="meta">📡 Bandas detectadas: ${Array.from(bands).join(', ') || '—'}</div>
  <div class="meta">📟 Protocolos: ${Array.from(standards).join(', ') || '—'}</div>
  <div class="meta">🔌 Sensor: ${escHtml(sensorId)}</div>
  <div class="meta">🔒 Privacidad: identificadores pseudónimos HMAC-SHA256. Sin direcciones MAC reales. Router sin modificaciones.</div>
  <div class="meta">ID: ${reportId}</div>
</div>
</body>
</html>`;

  return new Response(html, {
    status: 200,
    headers: { 'Content-Type': 'text/html;charset=utf-8', ...corsHeaders(origin) },
  });
}

function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

// ---------------------------------------------------------------------------
// Main router
// ---------------------------------------------------------------------------

export default {
  async fetch(
    request: Request,
    env: Env,
    ctx: ExecutionContext
  ): Promise<Response> {
    // Ensure schema is ready before handling any request
    await ensureSchema(env.DB);

    const url = new URL(request.url);
    const path = url.pathname;
    const origin = request.headers.get("Origin") || undefined;

    // CORS preflight
    if (request.method === "OPTIONS") {
      return new Response(null, {
        status: 204,
        headers: corsHeaders(origin),
      });
    }

    let response: Response | undefined;
    try {
      // POST endpoints
      if (request.method === "POST") {
        if (path === "/api/v1/events") {
          response = await handleIngest(request, env, ctx);
        } else if (path === "/api/v1/events/batch") {
          response = await handleIngest(request, env, ctx);
        } else if (path === "/api/v1/captures/sync") {
          response = await handleCollectorSync(request, env);
        }
      }

      // GET endpoints
      else if (request.method === "GET") {
        // Realtime WebSocket (Durable Object)
        if (path === "/ws") {
          const id = env.REALTIME_HUB.idFromName("hub");
          const stub = env.REALTIME_HUB.get(id);
          return stub.fetch(request);
        }
        // Dashboard UI
        if (path === "/" || path === "/dashboard" || path === "/index.html") {
          response = new Response(dashboardHtml, {
            headers: { "Content-Type": "text/html;charset=utf-8", "Cache-Control": "public, max-age=60" },
          });
        } else if (path === "/api/v1/healthz") response = await handleHealthz(request, env);
        else if (path === "/api/v1/readyz") response = await handleReadyz(request, env);
        else if (path === "/api/v1/devices") response = await handleDevices(request, env);
        else if (path === "/api/v1/presence") response = await handlePresence(request, env);
        else if (path === "/api/v1/sensors") response = await handleSensors(request, env);
        else if (path === "/api/v1/stats") response = await handleStats(request, env);
        else if (path === "/api/v1/timeline") response = await handleTimeline(request, env);
        else if (path === "/api/v1/networks") response = await handleNetworks(request, env);
        else if (path === "/api/v1/fusion") response = await handleFusion(request, env);
        else if (path === "/api/v1/state") response = await handleDeviceState(request, env);
        else if (path === "/api/v1/sessions") response = await handleSessions(request, env);
        else if (path === "/api/v1/reports/devices") response = await handleReportsDevices(request, env);
        else if (path === "/api/v1/reports/email") response = await handleEmailReport(request, env);
        else if (path === "/api/v1/events") response = await handleEvents(request, env);
        else if (/^\/api\/v1\/devices\/[^/]+\/events$/.test(path)) response = await handleDeviceEvents(request, env);
        else if (/^\/api\/v1\/devices\/[^/]+\/sessions$/.test(path)) response = await handleDeviceSessions(request, env);
        else if (/^\/api\/v1\/devices\/[^/]+\/signals$/.test(path)) response = await handleDeviceSignals(request, env);
      }

      if (!response) response = jsonResponse(404, { error: "not found" });
    } catch (e: any) {
      // Surface the real error so misconfigured queries / schema drift are diagnosable.
      console.error("REQUEST_ERROR", path, e?.message, e?.stack);
      response = jsonResponse(500, {
        error: e?.message ?? String(e),
        path,
        stack: String(e?.stack ?? "").split("\n").slice(0, 5),
      }, origin);
    }

    return response;
  },
};

export { RealtimeHub };
