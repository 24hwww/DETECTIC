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
  const limit = Math.min(parseInt(url.searchParams.get("limit") || "500"), 1000);
  const origin = request.headers.get("Origin") || undefined;

  const { results } = await env.DB.prepare(
    `SELECT d.pseudonym,
            MIN(COALESCE(s.captured_at, s.received_at)) AS first_seen,
            MAX(COALESCE(s.captured_at, s.received_at)) AS last_seen,
            COUNT(*) AS observations,
            CAST(ROUND(AVG(d.rssi)) AS INTEGER) AS avg_rssi,
            MIN(d.rssi) AS min_rssi,
            MAX(d.rssi) AS max_rssi,
            d.source, d.standard
     FROM detections d JOIN snapshots s ON d.snapshot_id = s.id
     GROUP BY d.pseudonym ORDER BY last_seen DESC LIMIT ?`
  )
    .bind(limit)
    .all();

  return jsonResponse(200, { devices: results }, origin);
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
    `SELECT d.pseudonym,
            MIN(COALESCE(s.captured_at, s.received_at)) AS first_seen,
            MAX(COALESCE(s.captured_at, s.received_at)) AS last_seen,
            COUNT(DISTINCT s.id) AS observations,
            CAST(ROUND(AVG(d.rssi)) AS INTEGER) AS avg_rssi,
            MIN(d.rssi) AS min_rssi,
            MAX(d.rssi) AS max_rssi,
            COUNT(DISTINCT date(COALESCE(s.captured_at, s.received_at), 'unixepoch')) AS distinct_days,
            d.source, d.standard
     FROM detections d JOIN snapshots s ON d.snapshot_id = s.id
     WHERE COALESCE(s.captured_at, s.received_at) >= ?
     GROUP BY d.pseudonym ORDER BY last_seen DESC LIMIT 500`
  )
    .bind(cutoff)
    .all();

  return jsonResponse(200, { hours, devices: results });
}

async function handleSensors(
  _request: Request,
  env: Env
): Promise<Response> {
  const { results } = await env.DB.prepare(
    `SELECT s.id, s.name, s.location, s.created_at, s.last_seen,
            (SELECT COUNT(*) FROM detections d WHERE d.sensor_id = s.id) AS total_observations,
            (SELECT COUNT(DISTINCT d.pseudonym) FROM detections d WHERE d.sensor_id = s.id) AS distinct_devices
     FROM sensors s ORDER BY s.last_seen DESC`
  ).all();

  return jsonResponse(200, { sensors: results });
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
      (SELECT COUNT(*) FROM snapshots) AS total_snapshots,
      (SELECT COUNT(*) FROM detections) AS total_detections,
      (SELECT COUNT(DISTINCT pseudonym) FROM detections) AS distinct_devices,
      (SELECT COUNT(*) FROM events) AS total_events,
      (SELECT COUNT(*) FROM sensors) AS total_sensors,
      (SELECT COUNT(*) FROM snapshots WHERE received_at >= ?) AS snapshots_last_hour,
      (SELECT COUNT(*) FROM snapshots WHERE received_at >= ?) AS snapshots_last_day`
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

async function handleTimeline(
  request: Request,
  env: Env
): Promise<Response> {
  const url = new URL(request.url);
  const hours = Math.min(parseInt(url.searchParams.get("hours") || "24"), 168);
  const cutoff = Math.floor(Date.now() / 1000) - hours * 3600;
  const origin = request.headers.get("Origin") || undefined;

  // Get detections with timestamps for charting
  const { results } = await env.DB.prepare(
    `SELECT d.pseudonym, d.rssi, d.source, d.standard,
            COALESCE(s.captured_at, s.received_at) AS ts
     FROM detections d JOIN snapshots s ON d.snapshot_id = s.id
     WHERE COALESCE(s.captured_at, s.received_at) >= ?
     ORDER BY ts ASC LIMIT 2000`
  )
    .bind(cutoff)
    .all();

  return jsonResponse(200, { hours, points: results }, origin);
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
    }

    return jsonResponse(404, { error: "not found" });
  },
};
