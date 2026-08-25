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
 *   GET  /api/v1/healthz      — health check
 *   GET  /                  — real-time dashboard UI
 */

import dashboardHtml from './dashboard.html';

interface Env {
  DB: D1Database;
  DETECTIC_SENSORS: string;  // JSON: {"sensor_id": "secret", ...}
  DETECTIC_MASTER_SECRET: string;
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
  body: string
): Promise<boolean> {
  const sensors = JSON.parse(env.DETECTIC_SENSORS || "{}");
  const secret = sensors[sensorId];
  if (!secret || !signature) return false;
  const expected = await hmacSha256(secret, new TextEncoder().encode(body));
  return expected === signature;
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
    ]);
    schemaReady = true;
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
  const bodyText = await request.text();

  if (bodyText.length > 4 * 1024 * 1024) {
    return jsonResponse(400, { error: "body too large" }, origin);
  }

  if (!(await verifyAuth(env, sensorId, signature, bodyText))) {
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
    return handleEventBatch(env, sensorId, payload, origin);
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
  sensorId: string,
  payload: SensorPayload,
  origin?: string
): Promise<Response> {
  const now = Math.floor(Date.now() / 1000);
  const events = (payload.events || []).slice(0, 100); // max 100 per batch

  let accepted = 0;
  let duplicates = 0;

  for (const evt of events) {
    const eventId = evt.event_id || "";
    if (!eventId) continue;

    try {
      await env.DB.prepare(
        "INSERT INTO events (sensor_id, event_id, event_type, event_timestamp, device_id, snapshot_json, schema_version, received_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
      )
        .bind(
          sensorId,
          eventId,
          evt.event_type || "",
          evt.event_timestamp || 0,
          evt.device_id || null,
          evt.snapshot ? JSON.stringify(evt.snapshot) : null,
          evt.schema_version || "2.0",
          now
        )
        .run();
      accepted++;
    } catch (e: any) {
      if (e.message?.includes("UNIQUE")) {
        duplicates++;
      }
    }
  }

  return jsonResponse(202, { accepted, duplicates }, origin);
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

  const devices = (idRows.results as any[]).map((i: any) => {
    const l = latest.get(i.pseudonym) || {};
    const f = fp.get(i.pseudonym);
    const n = rn.get(i.pseudonym) || 0;
    return {
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
    };
  });

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

  const { results } = await env.DB.prepare(
    `SELECT i.pseudonym, i.device_class, i.manufacturer, i.mac_type,
            COUNT(DISTINCT d.capture_id) AS observations,
            MAX(c.started_at) AS last_seen,
            MIN(c.started_at) AS first_seen
     FROM device_identity i
     JOIN collector_devices d ON d.pseudonym = i.pseudonym
     JOIN collector_captures c ON d.capture_id = c.capture_id
     WHERE c.started_at >= ?
     GROUP BY i.pseudonym ORDER BY last_seen DESC LIMIT 500`
  )
    .bind(cutoff)
    .all();

  return jsonResponse(200, { hours, devices: results }, origin);
}

async function handleSensors(
  _request: Request,
  env: Env
): Promise<Response> {
  const cap = await env.DB.prepare(
    `SELECT sensor_id AS id, MAX(started_at) AS last_seen,
            COUNT(*) AS total_captures,
            SUM(device_count) AS total_detections
     FROM collector_captures GROUP BY sensor_id ORDER BY last_seen DESC`
  ).all();

  const dev = await env.DB.prepare(
    `SELECT d.pseudonym, c.sensor_id
     FROM collector_devices d JOIN collector_captures c ON d.capture_id = c.capture_id`
  ).all();
  const perSensor = new Map<string, Set<string>>();
  for (const r of dev.results as any[]) {
    if (!perSensor.has(r.sensor_id)) perSensor.set(r.sensor_id, new Set());
    perSensor.get(r.sensor_id)!.add(r.pseudonym);
  }

  const sensors = (cap.results as any[]).map((s: any) => ({
    id: s.id,
    last_seen: s.last_seen,
    total_captures: s.total_captures,
    total_detections: s.total_detections,
    distinct_devices: perSensor.get(s.id)?.size || 0,
  }));

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
      (SELECT COUNT(*) FROM device_identity) AS distinct_devices,
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
  _request: Request,
  env: Env
): Promise<Response> {
  const { results } = await env.DB.prepare(
    `SELECT bssid_pseudonym, ssid, manufacturer, band, first_seen, last_seen,
            observation_count
     FROM wifi_network_observation ORDER BY observation_count DESC LIMIT 100`
  ).all();
  return jsonResponse(200, { networks: results });
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
  const bodyText = await request.text();

  if (bodyText.length > 4 * 1024 * 1024) {
    return jsonResponse(400, { error: "body too large" }, origin);
  }
  if (!(await verifyAuth(env, sensorId, signature, bodyText))) {
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

    // CORS preflight
    if (request.method === "OPTIONS") {
      return new Response(null, {
        status: 204,
        headers: corsHeaders(request.headers.get("Origin") || undefined),
      });
    }

    // POST endpoints
    if (request.method === "POST") {
      if (path === "/api/v1/events") {
        return handleIngest(request, env, ctx);
      }
      if (path === "/api/v1/events/batch") {
        return handleIngest(request, env, ctx);
      }
      if (path === "/api/v1/captures/sync") {
        return handleCollectorSync(request, env);
      }
    }

    // GET endpoints
    if (request.method === "GET") {
      // Dashboard UI
      if (path === "/" || path === "/dashboard" || path === "/index.html") {
        return new Response(dashboardHtml, {
          headers: { "Content-Type": "text/html;charset=utf-8", "Cache-Control": "public, max-age=60" },
        });
      }
      if (path === "/api/v1/healthz") return handleHealthz(request, env);
      if (path === "/api/v1/readyz") return handleReadyz(request, env);
      if (path === "/api/v1/devices") return handleDevices(request, env);
      if (path === "/api/v1/presence") return handlePresence(request, env);
      if (path === "/api/v1/sensors") return handleSensors(request, env);
      if (path === "/api/v1/stats") return handleStats(request, env);
      if (path === "/api/v1/timeline") return handleTimeline(request, env);
      if (path === "/api/v1/networks") return handleNetworks(request, env);
    }

    return jsonResponse(404, { error: "not found" });
  },
};
