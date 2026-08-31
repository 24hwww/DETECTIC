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
 *   GET  /api/v1/devices/aliases — stable fingerprint_id (huella) -> MAC aliases
 *   GET  /api/v1/healthz      — health check
 *   GET  /                  — real-time dashboard UI
 */

import { RealtimeHub } from './realtime';
import {
  buildAckBody,
  buildOpaqueError,
  constantTimeEqual,
  parseAllowedOrigins,
  resolveCorsOrigin,
  selectAcceptedEvents,
  type AckOutcome,
} from './protocol.ts';

const MANIFEST_JSON = JSON.stringify({
  name: "Detectic",
  short_name: "Detectic",
  description: "Identidad y huella Wi-Fi en tiempo real",
  start_url: "/",
  display: "standalone",
  background_color: "#0a0a0f",
  theme_color: "#0a0a0f",
  orientation: "portrait",
  icons: [
    { src: "/icon.svg", sizes: "any", type: "image/svg+xml" },
    { src: "/icon.svg", sizes: "192x192", type: "image/svg+xml" },
    { src: "/icon.svg", sizes: "512x512", type: "image/svg+xml" },
  ],
});

const ICON_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 192 192"><rect width="192" height="192" rx="24" fill="#0a0a0f"/><circle cx="96" cy="96" r="28" fill="#58a6ff"/><path d="M96 48c-26.5 0-48 21.5-48 48 0 26.5 21.5 48 48 48" stroke="#58a6ff" stroke-width="8" fill="none" stroke-linecap="round"/><path d="M96 32c-35.3 0-64 28.7-64 64 0 35.3 28.7 64 64 64" stroke="#3fb950" stroke-width="8" fill="none" stroke-linecap="round"/><path d="M96 16c-44.2 0-80 35.8-80 80 0 44.2 35.8 80 80 80" stroke="#d29922" stroke-width="8" fill="none" stroke-linecap="round"/></svg>`;

const SW_JS = `self.addEventListener('install', event => {
  self.skipWaiting();
});

self.addEventListener('activate', event => {
  event.waitUntil(self.clients.claim());
});

self.addEventListener('fetch', event => {
  if (event.request.method !== 'GET' || !event.request.url.startsWith(self.location.origin)) return;
  event.respondWith(
    fetch(event.request)
      .then(networkResponse => {
        if (networkResponse && networkResponse.status === 200) {
          const clone = networkResponse.clone();
          caches.open('detectic-v1').then(cache => cache.put(event.request, clone));
        }
        return networkResponse;
      })
      .catch(() => caches.match(event.request).then(cached => cached || new Response('Offline', { status: 503 })))
  );
});

self.addEventListener('push', event => {
  let data = { title: 'Detectic', body: 'Nuevo evento', tag: 'detectic', url: '/' };
  if (event.data) {
    try { data = event.data.json(); } catch (e) {}
  }
  event.waitUntil(
    self.registration.showNotification(data.title, {
      body: data.body,
      icon: '/icon.svg',
      badge: '/icon.svg',
      tag: data.tag,
      requireInteraction: true,
      data: { url: data.url || '/' },
    })
  );
});

self.addEventListener('notificationclick', event => {
  event.notification.close();
  const url = event.notification.data?.url || '/';
  event.waitUntil(self.clients.openWindow(url));
});
`;

const HUB_NAME = "hub";
function hubStub(env: Env) {
  return env.REALTIME_HUB.get(env.REALTIME_HUB.idFromName(HUB_NAME));
}

interface Env {
  DB: D1Database;
  DETECTIC_SENSORS: string;  // JSON: {"sensor_id": "secret", ...}
  DETECTIC_MASTER_SECRET: string;
  /** Comma-separated list of sensor ids that may bypass HMAC auth in emergencies (do not use in production). */
  DETECTIC_BYPASS_HMAC?: string;
  /** Comma-separated list of allowed dashboard origins (optional). */
  DETECTIC_ALLOWED_ORIGINS?: string;
  REALTIME_HUB: DurableObjectNamespace<RealtimeHub>;
  ASSETS: Fetcher;
}

interface SensorPayload {
  sensor_id?: string;
  id?: string;
  run_id?: string;
  captured_at?: number;
  devices?: Array<{
    pseudonym?: string;
    rssi?: number;
    rssi_dbm?: number;
    source?: string;
    standard?: string;
    radio_mac?: string;
    mac?: string;
    ip?: string;
    hostname?: string;
    band?: string;
    signal_level?: number;
    signal_strength?: number;
    noise?: number;
    tx_rate_kbps?: number;
    rx_rate_kbps?: number;
    tx_rate?: number;
    rx_rate?: number;
    max_link_rate?: number;
    status?: string;
    interface?: string;
    fingerprint_id?: string;
    fingerprint_method?: string;
    proximity_zone?: string;
    proximity_trend?: string;
    proximity_zone_label?: string;
    proximity_trend_label?: string;
    heat?: number;
    distance_m?: number;
    proximity_confidence?: number;
    proximity_samples?: number;
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
  let keyBytes = new TextEncoder().encode(secret);
  // Match Rust `hmac` crate behaviour: keys longer than the SHA-256 block
  // size (64 bytes) are hashed to 32 bytes before HMAC use.
  if (keyBytes.length > 64) {
    keyBytes = new Uint8Array(await crypto.subtle.digest("SHA-256", keyBytes));
  }
  const key = await crypto.subtle.importKey(
    "raw",
    keyBytes,
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"]
  );
  const sig = await crypto.subtle.sign("HMAC", key, data);
  return Array.from(new Uint8Array(sig))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function verifyMasterAuth(request: Request, env: Env): boolean {
  const master = env.DETECTIC_MASTER_SECRET;
  if (!master) return false;
  const auth = request.headers.get("Authorization") || "";
  if (auth.startsWith("Bearer ")) {
    return auth.slice(7).trim() === master;
  }
  const header = request.headers.get("X-Detectic-Master-Secret") || "";
  if (header) return header === master;
  const url = new URL(request.url);
  return url.searchParams.get("master_secret") === master;
}

type AuthVerdict = { ok: boolean; reason?: string };

async function verifyAuth(
  env: Env,
  sensorId: string,
  signature: string,
  body: string,
  timestamp?: string | null
): Promise<AuthVerdict> {
  const sensors = JSON.parse(env.DETECTIC_SENSORS || "{}");
  const secret = sensors[sensorId];
  if (!secret || !signature) return { ok: false, reason: "missing_secret" };

  if (timestamp) {
    const tsNum = parseInt(timestamp, 10);
    if (isNaN(tsNum)) return { ok: false, reason: "invalid_timestamp" };
    const now = Math.floor(Date.now() / 1000);
    if (Math.abs(now - tsNum) > 300) return { ok: false, reason: "timestamp_out_of_window" };
    const signed = new TextEncoder().encode(timestamp + "\n" + body);
    const expected = await hmacSha256(secret, signed);
    if (expected === signature) return { ok: true };
    return { ok: false, reason: "signature_mismatch" };
  }

  const expectedLegacy = await hmacSha256(secret, new TextEncoder().encode(body));
  if (expectedLegacy === signature) return { ok: true };
  return { ok: false, reason: "legacy_signature_mismatch" };
}

function verifyBearerToken(
  env: Env,
  sensorId: string,
  request: Request
): boolean {
  const auth = request.headers.get("Authorization") || "";
  if (!auth.startsWith("Bearer ")) return false;
  const token = auth.slice(7).trim();
  const sensors = JSON.parse(env.DETECTIC_SENSORS || "{}");
  const secret = sensors[sensorId];
  if (!secret || !token) return false;
  return constantTimeEqual(secret, token);
}

/**
 * Fallback authentication for snapshot payloads using the deterministic
 * UploadPayload.id as an HMAC over sensor_id|captured_at|sorted pseudonyms.
 * This survives JSON re-encoding by proxies because it recomputes the id from
 * the parsed payload fields, not from the raw request bytes.
 */
async function sha256Hex(data: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", data);
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

async function verifySnapshotId(
  env: Env,
  sensorId: string,
  bodyText: string,
  timestamp?: string | null
): Promise<{ ok: boolean; reason?: string }> {
  let payload: any;
  try {
    payload = JSON.parse(bodyText);
  } catch {
    return { ok: false, reason: "parse_failed" };
  }
  if (!payload || typeof payload.id !== "string") return { ok: false, reason: "missing_id" };
  if (typeof payload.captured_at !== "number") return { ok: false, reason: "missing_captured_at" };
  if (!Array.isArray(payload.devices)) return { ok: false, reason: "missing_devices" };
  const sensors = JSON.parse(env.DETECTIC_SENSORS || "{}");
  const secret = sensors[sensorId];
  if (!secret) return { ok: false, reason: "missing_secret" };
  const devicePseudos = (payload.devices as any[])
    .map((d) => d && typeof d.pseudonym === "string" ? d.pseudonym : "")
    .filter((p) => p.length > 0)
    .sort();
  const eventPseudos = Array.isArray(payload.events)
    ? (payload.events as any[])
        .map((e) => e && typeof e.pseudonym === "string" ? e.pseudonym : "")
        .filter((p) => p.length > 0)
        .sort()
    : [];
  const allPseudos = Array.from(new Set([...devicePseudos, ...eventPseudos])).sort();
  const pseudos = allPseudos.length > 0 ? allPseudos : devicePseudos;
  const rawDevicePseudos = (payload.devices as any[])
    .map((d) => d && typeof d.pseudonym === "string" ? d.pseudonym : "")
    .filter((p) => p.length > 0);
  const rawEventPseudos = Array.isArray(payload.events)
    ? (payload.events as any[])
        .map((e) => e && typeof e.pseudonym === "string" ? e.pseudonym : "")
        .filter((p) => p.length > 0)
    : [];

  const candidates: number[] = [payload.captured_at];
  const now = Math.floor(Date.now() / 1000);
  candidates.push(now);
  for (const base of [payload.captured_at, now]) {
    candidates.push(base * 1000);
    candidates.push(base * 1000000);
    if (base > 1000000000000) {
      candidates.push(Math.floor(base / 1000));
      candidates.push(Math.floor(base / 1000000));
    }
  }
  if (timestamp) {
    const tsNum = parseInt(timestamp, 10);
    if (!isNaN(tsNum)) {
      candidates.push(tsNum);
      for (let delta = -2; delta <= 2; delta++) candidates.push(tsNum + delta);
      candidates.push(tsNum * 1000);
      candidates.push(tsNum * 1000000);
    }
  }

  const base = payload.sensor_id || sensorId;
  const baseCandidates = [base, base.replace(/-/g, ""), base.replace(/-/g, "_")];
  const pseudoLists = [
    pseudos,
    devicePseudos,
    eventPseudos,
    rawDevicePseudos,
    rawEventPseudos,
    Array.from(new Set([...rawDevicePseudos, ...rawEventPseudos])),
    [...rawDevicePseudos, ...rawEventPseudos],
  ];
  for (const list of pseudoLists) {
    const joinedPseudos = list.join(",");
    for (const capturedAt of candidates) {
      for (const capturedStr of [String(capturedAt), String(capturedAt) + ".0"]) {
        for (const baseStr of baseCandidates) {
          const signed = new TextEncoder().encode(
            [baseStr, capturedStr, joinedPseudos].join("|")
          );
          const expected = await hmacSha256(secret, signed);
          if (constantTimeEqual(expected, payload.id)) return { ok: true };
        }
      }
    }
  }

  const bestExpected = await hmacSha256(secret, new TextEncoder().encode(
    [base, String(payload.captured_at), pseudos.join(",")].join("|")
  ));

  // Persist debug metadata to D1 to diagnose idempotency-key mismatches.
  try {
    const bodyBytes = new TextEncoder().encode(bodyText);
    const bodySha256 = await sha256Hex(bodyBytes);
    await env.DB.prepare(
      `INSERT INTO debug_ingest_log (sensor_id, captured_at, received_at, got_id, expected_id, pseudos_json, body_sha256, body_text, reason)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`
    ).bind(
      sensorId,
      payload.captured_at ?? null,
      now,
      payload.id,
      bestExpected,
      JSON.stringify({ devices: devicePseudos, events: eventPseudos }),
      bodySha256,
      bodyText.slice(0, 100000),
      `id_mismatch pseudos=${pseudos.length} device_pseudos=${devicePseudos.length} event_pseudos=${eventPseudos.length} candidates=${candidates.length}`
    ).run();
  } catch (e: any) {
    console.warn(`[verifySnapshotId] debug log insert failed: ${e.message || e}`);
  }

  return { ok: false, reason: `id_mismatch expected=${bestExpected} got=${payload.id} pseudos=${pseudos.length}` };
}

// Pseudonymize using SubtleCrypto (async) — the only pseudonymization path.
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
    `ALTER TABLE collector_devices ADD COLUMN fingerprint_id TEXT`,
    `ALTER TABLE collector_devices ADD COLUMN fingerprint_method TEXT`,
    `ALTER TABLE collector_captures ADD COLUMN payload_hash TEXT`,
    `ALTER TABLE device_identity ADD COLUMN bssid_manufacturer TEXT`,
    `ALTER TABLE device_identity ADD COLUMN identity_json TEXT`,
    `ALTER TABLE device_identity ADD COLUMN fingerprint_id TEXT`,
    `ALTER TABLE device_label ADD COLUMN alias TEXT`,
    `ALTER TABLE device_label ADD COLUMN owner TEXT`,
    `ALTER TABLE device_label ADD COLUMN room TEXT`,
    `ALTER TABLE device_label ADD COLUMN tags TEXT`,
    `ALTER TABLE device_label ADD COLUMN notes TEXT`,
    `ALTER TABLE device_state ADD COLUMN fingerprint_id TEXT`,
    `ALTER TABLE device_sessions ADD COLUMN fingerprint_id TEXT`,
    `ALTER TABLE events ADD COLUMN payload_json TEXT`,
    `ALTER TABLE events ADD COLUMN sequence INTEGER`,
    `ALTER TABLE events ADD COLUMN acked INTEGER NOT NULL DEFAULT 0`,
    `ALTER TABLE ap_state ADD COLUMN proximity TEXT`,
    `ALTER TABLE ap_state ADD COLUMN proximity_detail TEXT`,
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
      db.prepare(`CREATE TABLE IF NOT EXISTS device_identity (pseudonym TEXT NOT NULL, sensor_id TEXT NOT NULL, manufacturer TEXT, brand TEXT, model_guess TEXT, device_class TEXT, mac_type TEXT, confidence REAL, confidence_label TEXT, bssid_manufacturer TEXT, identity_json TEXT, fingerprint_id TEXT, last_seen INTEGER, PRIMARY KEY (pseudonym, sensor_id))`),
      db.prepare(`CREATE TABLE IF NOT EXISTS device_label (pseudonym TEXT PRIMARY KEY, alias TEXT, owner TEXT, room TEXT, tags TEXT, notes TEXT, updated_at INTEGER NOT NULL)`),
      db.prepare(`CREATE TABLE IF NOT EXISTS report_config (id INTEGER PRIMARY KEY CHECK (id = 1), enabled INTEGER NOT NULL DEFAULT 0, frequency_hours INTEGER NOT NULL DEFAULT 24, changes_only INTEGER NOT NULL DEFAULT 0, top_devices INTEGER NOT NULL DEFAULT 5, new_detections INTEGER NOT NULL DEFAULT 1, nearby_aps INTEGER NOT NULL DEFAULT 1, email_to TEXT, email_subject TEXT, updated_at INTEGER NOT NULL DEFAULT 0)`),
      db.prepare(`CREATE TABLE IF NOT EXISTS email_queue (id INTEGER PRIMARY KEY AUTOINCREMENT, report_id TEXT NOT NULL, scheduled_at INTEGER NOT NULL, generated_at INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'pending', html TEXT, text TEXT, config_json TEXT, attempts INTEGER NOT NULL DEFAULT 0, last_attempt_at INTEGER, sent_at INTEGER, error TEXT)`),
      db.prepare(`CREATE TABLE IF NOT EXISTS device_trust (pseudonym TEXT PRIMARY KEY, sensor_id TEXT, status TEXT NOT NULL DEFAULT 'unknown', first_seen INTEGER, last_seen INTEGER, alert_count INTEGER NOT NULL DEFAULT 0, acknowledged_at INTEGER, updated_at INTEGER NOT NULL DEFAULT 0)`),
      db.prepare(`CREATE TABLE IF NOT EXISTS device_ip (id INTEGER PRIMARY KEY AUTOINCREMENT, pseudonym TEXT NOT NULL, ip TEXT NOT NULL, mac TEXT, source TEXT NOT NULL DEFAULT 'arp', sensor_id TEXT, first_seen INTEGER, last_seen INTEGER, confidence REAL NOT NULL DEFAULT 1.0, UNIQUE (pseudonym, ip, source))`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_device_trust_status ON device_trust(status)`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_device_ip_pseudo ON device_ip(pseudonym)`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_device_ip_ip ON device_ip(ip)`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_device_ip_mac ON device_ip(mac)`),
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
      db.prepare(`CREATE TABLE IF NOT EXISTS device_aliases (
        fingerprint_id TEXT NOT NULL, pseudonym TEXT NOT NULL, sensor_id TEXT NOT NULL,
        hostname TEXT, band TEXT, first_seen INTEGER, last_seen INTEGER,
        PRIMARY KEY (fingerprint_id, pseudonym, sensor_id)
      )`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_da_fp ON device_aliases(fingerprint_id)`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_da_pseudo ON device_aliases(pseudonym)`),
      db.prepare(`CREATE TABLE IF NOT EXISTS ap_state (
        sensor_id TEXT NOT NULL, ap_id TEXT NOT NULL, status TEXT NOT NULL,
        ssid TEXT, band TEXT, channel INTEGER, current_signal INTEGER,
        average_signal REAL, min_signal INTEGER, max_signal INTEGER, rssi_variance REAL,
        observation_count INTEGER NOT NULL DEFAULT 0, session_count INTEGER NOT NULL DEFAULT 0,
        channel_history TEXT, first_seen INTEGER, last_seen INTEGER, online_since INTEGER,
        security TEXT, w_mode TEXT, extch TEXT, proximity TEXT, proximity_detail TEXT,
        updated_at INTEGER NOT NULL, PRIMARY KEY (sensor_id, ap_id)
      )`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_ap_sensor_status ON ap_state(sensor_id, status)`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_ap_last_seen ON ap_state(sensor_id, last_seen)`),
      db.prepare(`CREATE TABLE IF NOT EXISTS ap_sessions (
        session_id TEXT PRIMARY KEY, sensor_id TEXT NOT NULL, ap_id TEXT NOT NULL,
        started_at INTEGER NOT NULL, ended_at INTEGER, duration_seconds INTEGER,
        observation_count INTEGER NOT NULL DEFAULT 0, rssi_average REAL, rssi_min INTEGER,
        rssi_max INTEGER, channel_history TEXT, received_at INTEGER NOT NULL
      )`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_aps_ap ON ap_sessions(sensor_id, ap_id)`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_aps_start ON ap_sessions(started_at)`),
      db.prepare(`CREATE TABLE IF NOT EXISTS rf_environment_snapshots (
        event_id TEXT PRIMARY KEY, sensor_id TEXT NOT NULL, event_timestamp INTEGER NOT NULL,
        ap_count INTEGER NOT NULL DEFAULT 0, ap_count_2_4 INTEGER NOT NULL DEFAULT 0,
        ap_count_5 INTEGER NOT NULL DEFAULT 0, strongest_signal INTEGER, weakest_signal INTEGER,
        average_signal INTEGER, rssi_variance REAL, channel_distribution TEXT, top_aps TEXT,
        received_at INTEGER NOT NULL
      )`),
      db.prepare(`CREATE INDEX IF NOT EXISTS idx_rf_sensor_ts ON rf_environment_snapshots(sensor_id, event_timestamp)`),
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
      `CREATE TABLE IF NOT EXISTS device_identity (pseudonym TEXT NOT NULL, sensor_id TEXT NOT NULL, manufacturer TEXT, brand TEXT, model_guess TEXT, device_class TEXT, mac_type TEXT, confidence REAL, confidence_label TEXT, bssid_manufacturer TEXT, identity_json TEXT, fingerprint_id TEXT, last_seen INTEGER, PRIMARY KEY (pseudonym, sensor_id))`,
      `CREATE TABLE IF NOT EXISTS device_label (pseudonym TEXT PRIMARY KEY, alias TEXT, owner TEXT, room TEXT, tags TEXT, notes TEXT, updated_at INTEGER NOT NULL)`,
      `CREATE TABLE IF NOT EXISTS report_config (id INTEGER PRIMARY KEY CHECK (id = 1), enabled INTEGER NOT NULL DEFAULT 0, frequency_hours INTEGER NOT NULL DEFAULT 24, changes_only INTEGER NOT NULL DEFAULT 0, top_devices INTEGER NOT NULL DEFAULT 5, new_detections INTEGER NOT NULL DEFAULT 1, nearby_aps INTEGER NOT NULL DEFAULT 1, email_to TEXT, email_subject TEXT, updated_at INTEGER NOT NULL DEFAULT 0)`,
      `CREATE TABLE IF NOT EXISTS email_queue (id INTEGER PRIMARY KEY AUTOINCREMENT, report_id TEXT NOT NULL, scheduled_at INTEGER NOT NULL, generated_at INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'pending', html TEXT, text TEXT, config_json TEXT, attempts INTEGER NOT NULL DEFAULT 0, last_attempt_at INTEGER, sent_at INTEGER, error TEXT)`,
      `CREATE TABLE IF NOT EXISTS device_trust (pseudonym TEXT PRIMARY KEY, sensor_id TEXT, status TEXT NOT NULL DEFAULT 'unknown', first_seen INTEGER, last_seen INTEGER, alert_count INTEGER NOT NULL DEFAULT 0, acknowledged_at INTEGER, updated_at INTEGER NOT NULL DEFAULT 0)`,
      `CREATE TABLE IF NOT EXISTS device_ip (id INTEGER PRIMARY KEY AUTOINCREMENT, pseudonym TEXT NOT NULL, ip TEXT NOT NULL, mac TEXT, source TEXT NOT NULL DEFAULT 'arp', sensor_id TEXT, first_seen INTEGER, last_seen INTEGER, confidence REAL NOT NULL DEFAULT 1.0, UNIQUE (pseudonym, ip, source))`,
      `CREATE INDEX IF NOT EXISTS idx_device_trust_status ON device_trust(status)`,
      `CREATE INDEX IF NOT EXISTS idx_device_ip_pseudo ON device_ip(pseudonym)`,
      `CREATE INDEX IF NOT EXISTS idx_device_ip_ip ON device_ip(ip)`,
      `CREATE INDEX IF NOT EXISTS idx_device_ip_mac ON device_ip(mac)`,
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
      `CREATE TABLE IF NOT EXISTS device_aliases (fingerprint_id TEXT NOT NULL, pseudonym TEXT NOT NULL, sensor_id TEXT NOT NULL, hostname TEXT, band TEXT, first_seen INTEGER, last_seen INTEGER, PRIMARY KEY (fingerprint_id, pseudonym, sensor_id))`,
      `CREATE INDEX IF NOT EXISTS idx_da_fp ON device_aliases(fingerprint_id)`,
      `CREATE INDEX IF NOT EXISTS idx_da_pseudo ON device_aliases(pseudonym)`,
      `CREATE TABLE IF NOT EXISTS ap_state (sensor_id TEXT NOT NULL, ap_id TEXT NOT NULL, status TEXT NOT NULL, ssid TEXT, band TEXT, channel INTEGER, current_signal INTEGER, average_signal REAL, min_signal INTEGER, max_signal INTEGER, rssi_variance REAL, observation_count INTEGER NOT NULL DEFAULT 0, session_count INTEGER NOT NULL DEFAULT 0, channel_history TEXT, first_seen INTEGER, last_seen INTEGER, online_since INTEGER, security TEXT, w_mode TEXT, extch TEXT, proximity TEXT, proximity_detail TEXT, updated_at INTEGER NOT NULL, PRIMARY KEY (sensor_id, ap_id))`,
      `CREATE INDEX IF NOT EXISTS idx_ap_sensor_status ON ap_state(sensor_id, status)`,
      `CREATE INDEX IF NOT EXISTS idx_ap_last_seen ON ap_state(sensor_id, last_seen)`,
      `CREATE TABLE IF NOT EXISTS ap_sessions (session_id TEXT PRIMARY KEY, sensor_id TEXT NOT NULL, ap_id TEXT NOT NULL, started_at INTEGER NOT NULL, ended_at INTEGER, duration_seconds INTEGER, observation_count INTEGER NOT NULL DEFAULT 0, rssi_average REAL, rssi_min INTEGER, rssi_max INTEGER, channel_history TEXT, received_at INTEGER NOT NULL)`,
      `CREATE INDEX IF NOT EXISTS idx_aps_ap ON ap_sessions(sensor_id, ap_id)`,
      `CREATE INDEX IF NOT EXISTS idx_aps_start ON ap_sessions(started_at)`,
      `CREATE TABLE IF NOT EXISTS rf_environment_snapshots (event_id TEXT PRIMARY KEY, sensor_id TEXT NOT NULL, event_timestamp INTEGER NOT NULL, ap_count INTEGER NOT NULL DEFAULT 0, ap_count_2_4 INTEGER NOT NULL DEFAULT 0, ap_count_5 INTEGER NOT NULL DEFAULT 0, strongest_signal INTEGER, weakest_signal INTEGER, average_signal INTEGER, rssi_variance REAL, channel_distribution TEXT, top_aps TEXT, received_at INTEGER NOT NULL)`,
      `CREATE INDEX IF NOT EXISTS idx_rf_sensor_ts ON rf_environment_snapshots(sensor_id, event_timestamp)`,
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

/**
 * Resolve the Origin to reflect in Access-Control-Allow-Origin for `request`.
 *
 * Returns the request Origin ONLY if it is an explicit allowed origin
 * (DETECTIC_ALLOWED_ORIGINS) or it equals the worker's own origin (same-origin
 * dashboard). Returns undefined for absent Origin, disallowed origins, or any
 * non-dashboard client — in which case corsHeaders() emits no ACAO header, so
 * browsers block cross-origin reads. It never returns "*".
 */
function requestCorsOrigin(env: Env, request: Request): string | undefined {
  const allowed = parseAllowedOrigins(env.DETECTIC_ALLOWED_ORIGINS);
  const host = request.headers.get("Host");
  const selfOrigin = host ? (host.includes("localhost") || host.includes("127.0.0.1") ? `http://${host}` : `https://${host}`) : undefined;
  const origin = request.headers.get("Origin");
  return resolveCorsOrigin(origin, allowed, selfOrigin) ?? undefined;
}

function corsHeaders(origin?: string): Record<string, string> {
  const headers: Record<string, string> = {
    "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, OPTIONS",
    "Access-Control-Allow-Headers": "Content-Type, X-Detectic-Sensor, X-Detectic-Signature, Authorization",
    "Access-Control-Max-Age": "86400",
  };
  // Only emit a concrete allowed origin — never "*".
  if (origin) headers["Access-Control-Allow-Origin"] = origin;
  return headers;
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

function isHtmlResponse(res: Response): boolean {
  return String(res.headers.get("Content-Type") || "").includes("text/html");
}

/** Re-serves an HTML document with no-store so clients always pick up the
 *  latest hashed asset references (fixes stale bundle after redeploys). */
function noCacheHtml(res: Response): Response {
  const headers = new Headers(res.headers);
  headers.set("Cache-Control", "no-store");
  headers.set("Pragma", "no-cache");
  return new Response(res.body, { status: res.status, statusText: res.statusText, headers });
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

interface DiffEvent {
  captured_at?: number;
  kind?: string;
  pseudonym?: string;
  identity?: string;
  changed_fields?: string[];
}

type CanonicalEvent = NonNullable<SensorPayload["events"]> extends (infer E)[] ? E : never;

function isCanonicalEvent(ev: any): boolean {
  return ev && typeof ev === "object" &&
    (typeof ev.event_id === "string" && ev.event_id.length > 0) &&
    (typeof ev.type === "string" || typeof ev.event_type === "string");
}

function isDiffEvent(ev: any): boolean {
  return ev && typeof ev === "object" && typeof ev.kind === "string";
}

async function normalizeDiffEvent(
  sensorId: string,
  ev: DiffEvent,
  now: number
): Promise<CanonicalEvent | null> {
  if (!ev.pseudonym || !ev.kind) return null;
  const ts = ev.captured_at ?? now;
  const typeMap: Record<string, string> = {
    DeviceJoined: "device.connected",
    DeviceLeft: "device.disconnected",
    DeviceUpdated: "device.signal_changed",
  };
  const type = typeMap[ev.kind];
  if (!type) return null;
  const eventId = await shortDigest([sensorId, type, ev.pseudonym, String(ts)]);
  const payload: Record<string, any> = { pseudonym: ev.pseudonym };
  if (ev.changed_fields && ev.changed_fields.length > 0) {
    payload.changed_fields = ev.changed_fields;
  }
  return {
    event_id: eventId,
    event_type: type,
    type,
    timestamp: ts,
    device_id: ev.pseudonym,
    payload,
    sequence: 0,
  };
}

async function shortDigest(parts: string[]): Promise<string> {
  const joined = parts.join("|");
  const encoder = new TextEncoder();
  const buf = await crypto.subtle.digest("SHA-256", encoder.encode(joined));
  return Array.from(new Uint8Array(buf))
    .slice(0, 16)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

async function handleIngest(
  request: Request,
  env: Env,
  ctx: ExecutionContext
): Promise<Response> {
  const origin = requestCorsOrigin(env, request);

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

  const bearer = request.headers.get("Authorization") || "";
  const contentLength = request.headers.get("Content-Length") || "";
  const bodyBytes = new TextEncoder().encode(bodyText).length;
  let auth = await verifyAuth(env, sensorId, signature, bodyText, timestamp);
  const bearerOk = verifyBearerToken(env, sensorId, request);
  const idCheck = auth.ok ? { ok: false, reason: "" } : await verifySnapshotId(env, sensorId, bodyText, timestamp);
  const idOk = idCheck.ok;
  let payloadCapturedAt: number | null = null;
  let payloadId: string | null = null;
  try { const p = JSON.parse(bodyText); payloadCapturedAt = typeof p?.captured_at === "number" ? p.captured_at : null; payloadId = typeof p?.id === "string" ? p.id : null; } catch {}
  const bypass = (env.DETECTIC_BYPASS_HMAC || "").split(",").map((s) => s.trim()).includes(sensorId) || env.DETECTIC_BYPASS_HMAC === "*";
  const trustFallback = !auth.ok && bypass && typeof payloadId === "string" && payloadId.length === 64 && typeof payloadCapturedAt === "number";
  console.log(`[handleIngest] auth debug sensor=${sensorId} content_length=${contentLength} body_chars=${bodyText.length} body_bytes=${bodyBytes} signature_len=${signature.length} timestamp=${timestamp} captured_at=${payloadCapturedAt} bypass=${bypass} bearer_present=${bearer.length > 0} bearer_ok=${bearerOk} id_ok=${idOk} id_reason=${idCheck.reason || ""} trust_fallback=${trustFallback}`);
  if (!auth.ok && bearerOk) {
    auth = { ok: true, reason: "bearer_token" };
  }
  if (!auth.ok && idOk) {
    auth = { ok: true, reason: "snapshot_id" };
  }
  if (!auth.ok && trustFallback) {
    auth = { ok: true, reason: "trusted_sensor_bypass" };
  }
  if (!auth.ok) {
    console.warn(`[handleIngest] auth failed sensor=${sensorId} reason=${auth.reason}`);
    return jsonResponse(401, { error: "unauthorized", reason: auth.reason }, origin);
  }

  let payload: SensorPayload;
  try {
    payload = JSON.parse(bodyText);
  } catch {
    return jsonResponse(400, { error: "invalid json" }, origin);
  }

  // Extract sensor network / geolocation metadata from Cloudflare
  const cf = (request as any).cf || {};
  const publicIp = request.headers.get("cf-connecting-ip") || request.headers.get("x-forwarded-for")?.split(",")[0].trim() || cf.clientIp || null;
  const geoip: Location | undefined = (typeof cf.latitude === "number" && typeof cf.longitude === "number")
    ? { latitude: cf.latitude, longitude: cf.longitude, accuracy_m: 10000, source: "ip_geolocation", confidence: null, timestamp: Date.now() }
    : undefined;

  const isUploadPayload = typeof payload.captured_at === "number" && Array.isArray(payload.devices);

  if (isUploadPayload) {
    // Sensor upload: snapshot devices + optional diff events.
    // Always persist the snapshot. Canonical events (if any) are processed
    // for temporal side effects; legacy diff events are normalized so the
    // dashboard can still derive device presence/ap state from the same payload.
    const now = Math.floor(Date.now() / 1000);
    const allEvents = payload.events || [];
    const canonical = allEvents.filter(isCanonicalEvent);
    const diff = allEvents.filter(isDiffEvent) as DiffEvent[];
    if (diff.length > 0) {
      for (const d of diff) {
        const normalized = await normalizeDiffEvent(sensorId, d, now);
        if (normalized) canonical.push(normalized);
      }
    }

    const snapshotResp = await handleSnapshot(env, sensorId, payload, origin, publicIp, geoip);

    if (canonical.length > 0) {
      const batchPayload: SensorPayload = { events: canonical };
      const batchResp = await handleEventBatch(env, ctx, sensorId, batchPayload, origin, publicIp, geoip);
      // Return a combined ack/snapshot response; the sensor's HttpBackend only
      // cares about HTTP success. The EventTransport (canonical batch) uses the
      // 202 ack for retries.
      const snapBody = await snapshotResp.json() as any;
      const batchBody = await batchResp.json() as any;
      return jsonResponse(200, {
        snapshot: snapBody,
        batch: batchBody,
      }, origin);
    }

    return snapshotResp;
  }

  // Handle event batch
  if (payload.events && Array.isArray(payload.events)) {
    return handleEventBatch(env, ctx, sensorId, payload, origin, publicIp, geoip);
  }

  // Handle snapshot
  return handleSnapshot(env, sensorId, payload, origin, publicIp, geoip);
}

async function handleSnapshot(
  env: Env,
  sensorId: string,
  payload: SensorPayload,
  origin?: string,
  publicIp?: string | null,
  geoip?: Location
): Promise<Response> {
  const now = Math.floor(Date.now() / 1000);
  const capturedAt = payload.captured_at || now;
  const devices = payload.devices || [];

  // Sanitize: keep pseudonym + radio metadata and any extra fields the sensor
  // may have included (proximity, rates, hostname, band, etc.).
  const sanitizedDevices = devices
    .filter((d) => d.pseudonym)
    .map((d) => ({
      pseudonym: d.pseudonym!,
      rssi: d.rssi ?? null,
      rssi_dbm: d.rssi_dbm ?? null,
      source: d.source ?? null,
      standard: d.standard ?? null,
      radio_mac: d.radio_mac ?? null,
      mac: d.mac ?? null,
      ip: d.ip ?? null,
      hostname: d.hostname ?? null,
      band: d.band ?? null,
      signal_level: d.signal_level ?? null,
      signal_strength: d.signal_strength ?? null,
      noise: d.noise ?? null,
      tx_rate_kbps: d.tx_rate_kbps ?? d.tx_rate ?? null,
      rx_rate_kbps: d.rx_rate_kbps ?? d.rx_rate ?? null,
      max_link_rate: d.max_link_rate ?? null,
      status: d.status ?? null,
      interface: d.interface ?? null,
      fingerprint_id: d.fingerprint_id ?? null,
      fingerprint_method: d.fingerprint_method ?? null,
      proximity_zone: d.proximity_zone ?? null,
      proximity_trend: d.proximity_trend ?? null,
      proximity_zone_label: d.proximity_zone_label ?? null,
      proximity_trend_label: d.proximity_trend_label ?? null,
      heat: d.heat ?? null,
      distance_m: d.distance_m ?? null,
      proximity_confidence: d.proximity_confidence ?? null,
      proximity_samples: d.proximity_samples ?? null,
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

  // Also populate the collector_* tables so the existing dashboard queries
  // (handleDevices, handlePresence, handleTimeline, handleStats) stay in sync
  // when the sensor uploads via the HTTP snapshot path (UploadPayload).
  const captureId = payload.id || `snap-${sensorId}-${now}`;
  await env.DB.prepare(
    `INSERT OR REPLACE INTO collector_captures
     (capture_id, run_id, sensor_id, scheduled_at, started_at, completed_at, status,
      device_count, active_device_count, created_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`
  ).bind(
    captureId,
    payload.run_id || captureId,
    sensorId,
    capturedAt,
    capturedAt,
    null,
    "OK",
    sanitizedDevices.length,
    sanitizedDevices.filter((d) => d.rssi != null).length,
    now
  ).run();

  if (sanitizedDevices.length > 0) {
    const devStmts = sanitizedDevices.map((d) =>
      env.DB.prepare(
        `INSERT OR REPLACE INTO collector_devices
         (capture_id, pseudonym, hostname, band, signal_strength, signal_level, noise,
          operating_standard, tx_rate_kbps, rx_rate_kbps, status, bssid_pseudonym,
          identity_json, fingerprint_id, fingerprint_method)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`
      ).bind(
        captureId,
        d.pseudonym,
        d.hostname ?? null,
        d.band ?? null,
        d.rssi ?? null,
        d.signal_level ?? null,
        d.noise ?? null,
        d.standard ?? null,
        d.tx_rate_kbps ?? null,
        d.rx_rate_kbps ?? null,
        d.status ?? null,
        d.radio_mac ?? null,
        null,
        d.fingerprint_id ?? null,
        d.fingerprint_method ?? null
      )
    );
    await env.DB.batch(devStmts);
  }

  // Update sensor last_seen, public IP and geoip
  const existing = await env.DB.prepare("SELECT location FROM sensors WHERE id = ?").bind(sensorId).first() as { location?: string } | null;
  const merged = mergeSensorLocation(existing?.location ?? null, publicIp ?? null, geoip ?? null);
  await env.DB.prepare(
    `INSERT INTO sensors (id, created_at, last_seen, location) VALUES (?, ?, ?, ?)
     ON CONFLICT(id) DO UPDATE SET
       last_seen = excluded.last_seen,
       location = excluded.location,
       created_at = coalesce(sensors.created_at, excluded.created_at)`
  )
    .bind(sensorId, now, now, JSON.stringify(merged))
    .run();

  // Upsert current device presence from the snapshot itself. The snapshot is an
  // authoritative observation: every device listed is present. This keeps
  // device_state in sync even when the WSS event stream is not used or the
  // legacy diff events only contain DeviceUpdated (which should not change
  // presence).
  await upsertDeviceStateFromSnapshot(env, sensorId, sanitizedDevices, capturedAt, now);

  // Mark any device not seen in this snapshot for a while as ABSENT. A device
  // that truly disappeared will not appear in subsequent snapshots; this is the
  // explicit absence policy for the HTTP snapshot path. A WSS `device.disconnected`
  // event will also set ABSENT (likely earlier), so we only act when the HTTP
  // snapshot is the only source of truth.
  const ABSENCE_THRESHOLD_SECONDS = 60;
  await env.DB.prepare(
    `UPDATE device_state
     SET state = 'ABSENT'
     WHERE sensor_id = ? AND state IN ('PRESENT', 'CONNECTED')
       AND last_seen < ?`
  ).bind(sensorId, capturedAt - ABSENCE_THRESHOLD_SECONDS).run();

  return jsonResponse(
    200,
    { snapshot: snapId, devices_stored: sanitizedDevices.length },
    origin
  );
}

async function upsertDeviceStateFromSnapshot(
  env: Env,
  sensorId: string,
  devices: Array<{
    pseudonym: string;
    rssi?: number | null;
    band?: string | null;
    interface?: string | null;
    fingerprint_id?: string | null;
  }>,
  capturedAt: number,
  now: number
): Promise<void> {
  if (devices.length === 0) return;
  const presentPseudos = new Set<string>();
  const stmts: D1PreparedStatement[] = [];
  for (const d of devices) {
    presentPseudos.add(d.pseudonym);
    const rssi = typeof d.rssi === "number" ? d.rssi : null;
    const band = typeof d.band === "string" && d.band.length > 0 ? d.band : null;
    const iface = typeof d.interface === "string" && d.interface.length > 0 ? d.interface : null;
    const fingerprint = typeof d.fingerprint_id === "string" && d.fingerprint_id.length > 0
      ? d.fingerprint_id
      : d.pseudonym;
    stmts.push(
      env.DB.prepare(
        `INSERT INTO device_state
         (sensor_id, device_id, state, last_signal, noise, band, interface,
          current_session_id, first_seen, last_seen, total_connected_time,
          connection_count, updated_at, fingerprint_id)
         VALUES (?, ?, 'PRESENT', ?, NULL, ?, ?, NULL, ?, ?, 0, 0, ?, ?)
         ON CONFLICT(sensor_id, device_id) DO UPDATE SET
           state = 'PRESENT',
           last_signal = COALESCE(excluded.last_signal, device_state.last_signal),
           band = COALESCE(excluded.band, device_state.band),
           interface = COALESCE(excluded.interface, device_state.interface),
           first_seen = COALESCE(device_state.first_seen, excluded.first_seen),
           last_seen = excluded.last_seen,
           updated_at = excluded.updated_at,
           fingerprint_id = COALESCE(excluded.fingerprint_id, device_state.fingerprint_id)`
      ).bind(
        sensorId,
        d.pseudonym,
        rssi,
        band,
        iface,
        capturedAt,
        capturedAt,
        now,
        fingerprint
      )
    );
  }
  await batchInChunks(env.DB, stmts, 50);
}

async function batchInChunks(db: D1Database, stmts: D1PreparedStatement[], chunkSize = 50): Promise<void> {
  for (let i = 0; i < stmts.length; i += chunkSize) {
    const chunk = stmts.slice(i, i + chunkSize);
    await db.batch(chunk);
  }
}

/**
 * Apply the D1 side effects (device_state, ap_state, rf_environment_snapshots,
 * device_aliases) for a single canonical event. This is the shared path used by
 * both HTTP batch ingest and WSS ingest.
 */
export async function applyCanonicalEventToD1(
  env: Env,
  sensorId: string,
  evt: CanonicalEvent,
  now: number
): Promise<void> {
  const type = String(evt.type || evt.event_type || "");
  const ts = (evt.timestamp ?? evt.event_timestamp ?? now) as number;
  const deviceId = evt.device_id ?? null;
  const eventId = evt.event_id || "";
  if (!eventId || !type) return;

  const sideEffects: D1PreparedStatement[] = [];

  if (deviceId) {
    if (type.startsWith("device.")) {
      applyTemporalSideEffects(env, sideEffects, sensorId, type, ts, now, deviceId, evt.payload);
      applyAliasSideEffects(env, sideEffects, sensorId, ts, now, deviceId, evt as Record<string, any>);
    } else if (type.startsWith("arp.") || type.startsWith("ipv6.") || type.startsWith("ndp.")) {
      applyIpSideEffects(env, sideEffects, sensorId, type, ts, now, deviceId, evt.payload);
    } else if (type === "rf.probe_detected") {
      // A probe from an (external) RF sensor marks the device as RF_PRESENT in
      // device_state, reusing the presence side-effect contract.
      applyTemporalSideEffects(env, sideEffects, sensorId, "device.presence_changed", ts, now, deviceId, {
        to_state: "RF_PRESENT",
        ...(evt.payload || {}),
      });
    } else if (type.startsWith("network.")) {
      applyApSideEffects(env, sideEffects, sensorId, type, ts, now, deviceId, evt.payload);
    }
  } else if (type === "rf.environment_snapshot") {
    applyRfSnapshot(env, sideEffects, sensorId, eventId, ts, now, evt.payload);
  }

  if (sideEffects.length > 0) {
    await batchInChunks(env.DB, sideEffects, 50);
  }
}

async function handleEventBatch(
  env: Env,
  ctx: ExecutionContext,
  sensorId: string,
  payload: SensorPayload,
  origin?: string,
  publicIp?: string | null,
  geoip?: Location
): Promise<Response> {
  const now = Math.floor(Date.now() / 1000);
  const events = (payload.events || []).slice(0, 100); // max 100 per batch

  // Explicit, ID-keyed classification (never positional).
  const acceptedIds: string[] = [];
  const duplicateIds: string[] = [];
  const rejectedIds: string[] = [];
  let maxSeq: number | null = null;
  const sideEffects: D1PreparedStatement[] = [];

  for (const evt of events) {
    const eventId = evt.event_id || "";
    if (!eventId) {
      rejectedIds.push("");
      continue;
    }

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
      acceptedIds.push(eventId);

      // Append side-effect statements for the accepted event.
      await applyCanonicalEventToD1(env, sensorId, evt, now);
    } catch (e: any) {
      if (e?.message?.includes("UNIQUE")) {
        duplicateIds.push(eventId);
      } else {
        rejectedIds.push(eventId);
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
    await batchInChunks(env.DB, sideEffects, 50);
  }

  // Fan newly accepted events out to subscribed frontends via the realtime
  // hub. Events are selected by stable event ID (never by array position), so
  // duplicates and rejections cannot be misattributed as accepted.
  if (acceptedIds.length > 0 && env.REALTIME_HUB) {
    const acceptedById = new Set(acceptedIds);
    const acceptedEventsByMain = selectAcceptedEvents(events, acceptedById);
    ctx.waitUntil(
      env.REALTIME_HUB.get(env.REALTIME_HUB.idFromName("hub")).notify(acceptedEventsByMain, sensorId)
    );
  }

  // Update sensor last_seen, public IP and geoip
  const existing = await env.DB.prepare("SELECT location FROM sensors WHERE id = ?").bind(sensorId).first() as { location?: string } | null;
  const merged = mergeSensorLocation(existing?.location ?? null, publicIp ?? null, geoip ?? null);
  await env.DB.prepare(
    `INSERT INTO sensors (id, created_at, last_seen, location) VALUES (?, ?, ?, ?)
     ON CONFLICT(id) DO UPDATE SET
       last_seen = excluded.last_seen,
       location = excluded.location,
       created_at = coalesce(sensors.created_at, excluded.created_at)`
  )
    .bind(sensorId, now, now, JSON.stringify(merged))
    .run();

  const ackBody: AckOutcome = buildAckBody(acceptedIds, duplicateIds, rejectedIds);
  return jsonResponse(202, ackBody, origin);
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

function applyIpSideEffects(
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
  const ip = strOrNull(p.ip ?? p.ipv4 ?? p.ipv6);
  if (!ip) return;
  const mac = strOrNull(p.mac ?? p.lladdr);
  const source = type.startsWith("ipv6.") || type.startsWith("ndp.") ? "ndp" : "arp";
  const confidence = typeof p.confidence === "number" ? p.confidence : 1.0;

  stmts.push(
    env.DB.prepare(
      `INSERT INTO device_ip (pseudonym, ip, mac, source, sensor_id, first_seen, last_seen, confidence)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?)
       ON CONFLICT(pseudonym, ip, source) DO UPDATE SET
         mac = COALESCE(excluded.mac, device_ip.mac),
         last_seen = excluded.last_seen,
         confidence = excluded.confidence`
    ).bind(deviceId, ip, mac, source, sensorId, ts, ts, confidence)
  );
}

function applyAliasSideEffects(
  env: Env,
  stmts: D1PreparedStatement[],
  sensorId: string,
  ts: number,
  now: number,
  deviceId: string,
  evt: Record<string, any>
): void {
  // deviceId is the stable fingerprint_id (huella). Register every MAC
  // pseudonym observed for it. The sensor carries the alias set at the event
  // top level (mac_pseudonym, aliases) and/or inside payload.
  const p = (evt.payload && typeof evt.payload === "object" ? evt.payload : {}) as Record<string, any>;
  const fingerprintId = strOrNull(evt.fingerprint_id ?? p.fingerprint_id) || deviceId;
  const macPseudonym = strOrNull(evt.mac_pseudonym ?? p.mac_pseudonym);
  const aliases: unknown = evt.aliases ?? p.aliases;
  const band = strOrNull(evt.band ?? p.band);
  const hostname = strOrNull(evt.hostname ?? p.hostname);

  const toRegister: { pseudo: string; band: string | null }[] = [];
  if (macPseudonym) toRegister.push({ pseudo: macPseudonym, band });
  if (Array.isArray(aliases)) {
    for (const a of aliases) {
      const s = strOrNull(a);
      if (s && !toRegister.some((t) => t.pseudo === s)) toRegister.push({ pseudo: s, band: null });
    }
  }

  for (const t of toRegister) {
    stmts.push(
      env.DB.prepare(
        `INSERT INTO device_aliases (fingerprint_id, pseudonym, sensor_id, hostname, band, first_seen, last_seen)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(fingerprint_id, pseudonym, sensor_id) DO UPDATE SET
           hostname = COALESCE(excluded.hostname, device_aliases.hostname),
           band = COALESCE(excluded.band, device_aliases.band),
           last_seen = excluded.last_seen`
      ).bind(fingerprintId, t.pseudo, sensorId, hostname, t.band, ts, ts)
    );
  }
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
          connection_count, updated_at, fingerprint_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(sensor_id, device_id) DO UPDATE SET
           state = COALESCE(NULLIF(excluded.state, ''), device_state.state, 'PRESENT'),
           last_signal = COALESCE(excluded.last_signal, device_state.last_signal),
           noise = COALESCE(excluded.noise, device_state.noise),
           band = COALESCE(excluded.band, device_state.band),
           interface = COALESCE(excluded.interface, device_state.interface),
           current_session_id = COALESCE(excluded.current_session_id, device_state.current_session_id),
           first_seen = COALESCE(device_state.first_seen, excluded.first_seen),
           last_seen = excluded.last_seen,
           total_connected_time = device_state.total_connected_time + excluded.total_connected_time,
           connection_count = device_state.connection_count + excluded.connection_count,
           updated_at = excluded.updated_at,
           fingerprint_id = COALESCE(excluded.fingerprint_id, device_state.fingerprint_id)`
      ).bind(
        sensorId,
        deviceId,
        state || "PRESENT",
        extra.last_signal ?? null,
        extra.noise ?? null,
        extra.band ?? null,
        extra.interface ?? null,
        extra.current_session_id ?? null,
        ts,
        ts,
        extra.total_connected_time ?? 0,
        extra.connection_count ?? 0,
        now,
        deviceId
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
             (session_id, sensor_id, device_id, started_at, ended_at, duration_seconds, band, last_signal, last_noise, received_at, fingerprint_id)
             VALUES (?, ?, ?, ?, NULL, NULL, ?, ?, ?, ?, ?)`
          ).bind(sess, sensorId, deviceId, ts, strOrNull(p.band), numOrNull(p.rssi ?? p.signal), numOrNull(p.noise), now, deviceId)
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
             (session_id, sensor_id, device_id, started_at, ended_at, duration_seconds, band, last_signal, last_noise, received_at, fingerprint_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(session_id) DO UPDATE SET
               ended_at = excluded.ended_at,
               duration_seconds = excluded.duration_seconds,
               last_signal = COALESCE(excluded.last_signal, device_sessions.last_signal),
               last_noise = COALESCE(excluded.last_noise, device_sessions.last_noise),
               received_at = excluded.received_at,
               fingerprint_id = COALESCE(excluded.fingerprint_id, device_sessions.fingerprint_id)`
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
            now,
            deviceId
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
        stateUpdate(p.to_state, {
          last_signal: numOrNull(p.rssi ?? p.rssi_dbm),
          band: strOrNull(p.band),
        });
      }
      break;
    }
    case "device.proximity_changed": {
      stateUpdate("", {
        last_signal: numOrNull(p.rssi_dbm ?? p.new_signal),
        band: strOrNull(p.band),
      });
      break;
    }
    default:
      break;
  }
}

function numOrNull(v: unknown): number | null {
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}

type Location = {
  latitude?: number;
  longitude?: number;
  source?: string;
  accuracy_m?: number;
  confidence?: number | null;
  timestamp?: number;
  method?: string;
};

const LOCATION_SOURCE_PRIORITY = ["gps", "manual", "sensor_known_location", "ip_geolocation", "rf_estimation", "estimated", "unknown"];

function resolveLocation(...locs: (Location | undefined)[]): Location {
  const valid = locs.filter((l): l is Location => !!l && typeof l.latitude === "number" && typeof l.longitude === "number");
  const sorted = valid.sort((a, b) => {
    const ra = LOCATION_SOURCE_PRIORITY.indexOf(a.source || "unknown");
    const rb = LOCATION_SOURCE_PRIORITY.indexOf(b.source || "unknown");
    if (ra !== rb) return ra - rb;
    return (a.accuracy_m ?? Infinity) - (b.accuracy_m ?? Infinity);
  });
  return sorted[0] || { source: "unknown" };
}

function mergeSensorLocation(existingJson: string | null, publicIp: string | null, geoip: Location | null): {
  public_ip: string | null;
  geoip?: Location;
  manual?: Location;
  gps?: Location;
  current: Location;
} {
  const base: any = {};
  if (existingJson) {
    try { Object.assign(base, JSON.parse(existingJson)); } catch {}
  }
  if (publicIp) base.public_ip = publicIp;
  if (geoip) base.geoip = geoip;
  base.current = resolveLocation(base.gps, base.manual, base.known, base.geoip);
  return base;
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
    const proximity = strOrNull(newOrValue(p.proximity));
    const proximityDetail = p.proximity_detail && typeof p.proximity_detail === "object"
      ? JSON.stringify(p.proximity_detail)
      : null;
    stmts.push(
      env.DB.prepare(
        `INSERT INTO ap_state
         (sensor_id, ap_id, status, ssid, band, channel, current_signal, security,
          w_mode, extch, observation_count, first_seen, last_seen, online_since,
          updated_at, average_signal, min_signal, max_signal, rssi_variance,
          proximity, proximity_detail)
         VALUES (?, ?, 'ONLINE', ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
           END,
           proximity = COALESCE(excluded.proximity, ap_state.proximity),
           proximity_detail = COALESCE(excluded.proximity_detail, ap_state.proximity_detail)`
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
        null,
        proximity,
        proximityDetail
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
  const origin = requestCorsOrigin(env, request);

  const trustRows = await env.DB.prepare('SELECT pseudonym, status FROM device_trust').all();
  const trustMap = new Map<string, string>();
  for (const r of trustRows.results as any[]) trustMap.set(r.pseudonym, r.status);

  const idRows = await env.DB.prepare(
    `SELECT i.pseudonym, i.manufacturer, i.brand, i.model_guess, i.device_class,
            i.mac_type, i.confidence, i.confidence_label, i.bssid_manufacturer, i.last_seen,
            l.alias, l.owner, l.room, l.tags, l.notes
     FROM device_identity i
     LEFT JOIN device_label l ON i.pseudonym = l.pseudonym
     ORDER BY i.last_seen DESC LIMIT ?`
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
      alias: i.alias ?? null,
      owner: i.owner ?? null,
      room: i.room ?? null,
      tags: i.tags ?? null,
      trust_status: trustMap.get(i.pseudonym) || 'unknown',
      notes: i.notes ?? null,
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
      alias: null,
      owner: null,
      room: null,
      tags: null,
      trust_status: trustMap.get(pseudonym) || 'unknown',
      notes: null,
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
  const origin = requestCorsOrigin(env, request);

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

async function handleUpdateSensorLocation(
  request: Request,
  env: Env
): Promise<Response> {
  const origin = requestCorsOrigin(env, request);
  const path = new URL(request.url).pathname;
  const match = path.match(/^\/api\/v1\/sensors\/([^/]+)\/location$/);
  if (!match) return jsonResponse(400, { error: "invalid path" }, origin);
  const sensorId = decodeURIComponent(match[1]);
  let body: any;
  try { body = await request.json(); } catch { return jsonResponse(400, { error: "invalid json" }, origin); }
  if (!body || typeof body.latitude !== "number" || typeof body.longitude !== "number") {
    return jsonResponse(400, { error: "latitude and longitude required" }, origin);
  }
  const manual: Location = {
    latitude: body.latitude,
    longitude: body.longitude,
    source: "manual",
    accuracy_m: typeof body.accuracy_m === "number" ? body.accuracy_m : 10,
    confidence: 1.0,
    timestamp: Date.now()
  };
  const existing = await env.DB.prepare("SELECT location FROM sensors WHERE id = ?").bind(sensorId).first() as { location?: string } | null;
  const base: any = {};
  if (existing?.location) {
    try { Object.assign(base, JSON.parse(existing.location)); } catch {}
  }
  base.manual = manual;
  base.current = resolveLocation(base.gps, base.manual, base.known, base.geoip);
  await env.DB.prepare(
    `INSERT INTO sensors (id, created_at, last_seen, location) VALUES (?, ?, ?, ?)
     ON CONFLICT(id) DO UPDATE SET
       location = excluded.location,
       created_at = coalesce(sensors.created_at, excluded.created_at)`
  )
    .bind(sensorId, Math.floor(Date.now()/1000), Math.floor(Date.now()/1000), JSON.stringify(base))
    .run();
  return jsonResponse(200, { id: sensorId, location: base.current }, origin);
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

  const locRows = await env.DB.prepare(
    `SELECT id, name, location, created_at, last_seen FROM sensors`
  ).all();
  const locBySensor = new Map<string, any>();
  for (const r of (locRows.results as any[])) locBySensor.set(r.id, r);

  const sensors = Array.from(bySensor.entries()).map(([id, s]) => {
    const meta = locBySensor.get(id);
    let location: any = { source: "unknown" };
    let public_ip: string | null = null;
    if (meta?.location) {
      try {
        const parsed = JSON.parse(meta.location);
        location = parsed.current || { source: "unknown" };
        public_ip = parsed.public_ip || null;
      } catch {}
    }
    return {
      id,
      name: meta?.name || id,
      last_seen: Math.max(s.last_seen || 0, meta?.last_seen || 0),
      ap_count: s.ap_count || 0,
      distinct_devices: s.distinct_devices || 0,
      total_devices: s.total_devices || 0,
      location,
      public_ip,
      created_at: meta?.created_at || null
    };
  }).sort((a, b) => (b.last_seen || 0) - (a.last_seen || 0));

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
      (SELECT COUNT(DISTINCT ap_id) FROM ap_state) AS total_networks`
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
  const origin = requestCorsOrigin(env, request);

  const apConds: string[] = ["last_seen >= ?"];
  const apBinds: (string | number)[] = [cutoff];
  if (sensorId && sensorId !== "all") {
    apConds.push("sensor_id = ?");
    apBinds.push(sensorId);
  }
  const { results } = await env.DB.prepare(
    `SELECT sensor_id, ap_id, status, ssid, band, channel, current_signal, average_signal,
            min_signal, max_signal, rssi_variance, observation_count, session_count,
            first_seen, last_seen, online_since, security, w_mode, extch,
            proximity, proximity_detail
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
  const origin = requestCorsOrigin(env, request);

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
            min_signal, max_signal, security, w_mode, extch, first_seen, last_seen,
            proximity, proximity_detail
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
  const origin = requestCorsOrigin(env, request);

  let query = `SELECT device_id, state, last_signal, noise, band, interface,
                      current_session_id, first_seen, last_seen,
                      total_connected_time, connection_count, updated_at,
                      fingerprint_id
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

  // Enrich each device with its MAC aliases (huella -> pseudonyms).
  if (results.length > 0) {
    const fpIds = results.map((r: any) => r.fingerprint_id || r.device_id).filter(Boolean);
    if (fpIds.length > 0) {
      const placeholders = fpIds.map(() => "?").join(",");
      const aliasQ = sensorId
        ? `SELECT fingerprint_id, pseudonym, band, first_seen, last_seen
           FROM device_aliases WHERE fingerprint_id IN (${placeholders}) AND sensor_id = ?`
        : `SELECT fingerprint_id, pseudonym, band, first_seen, last_seen
           FROM device_aliases WHERE fingerprint_id IN (${placeholders})`;
      const aliasBinds = sensorId ? [...fpIds, sensorId] : fpIds;
      const { results: aliasRows } = await env.DB.prepare(aliasQ).bind(...aliasBinds).all();
      const byFp: Record<string, any[]> = {};
      for (const a of aliasRows as any[]) {
        (byFp[a.fingerprint_id] = byFp[a.fingerprint_id] || []).push(a);
      }
      for (const r of results as any[]) {
        const fp = r.fingerprint_id || r.device_id;
        r.aliases = byFp[fp] || [];
      }
    }
  }
  return jsonResponse(200, { devices: results }, origin);
}

async function handleDeviceAliases(
  request: Request,
  env: Env
): Promise<Response> {
  const url = new URL(request.url);
  const sensorId = url.searchParams.get("sensor_id");
  const fingerprintId = url.searchParams.get("fingerprint_id");
  const origin = requestCorsOrigin(env, request);

  const conds: string[] = [];
  const binds: (string | number)[] = [];
  if (sensorId) { conds.push("sensor_id = ?"); binds.push(sensorId); }
  if (fingerprintId) { conds.push("fingerprint_id = ?"); binds.push(fingerprintId); }
  const where = conds.length ? `WHERE ${conds.join(" AND ")}` : "";
  const { results } = await env.DB.prepare(
    `SELECT fingerprint_id, pseudonym, sensor_id, hostname, band, first_seen, last_seen
     FROM device_aliases ${where}
     ORDER BY last_seen DESC LIMIT 1000`
  ).bind(...binds).all();

  // Group by fingerprint_id for a compact view.
  const byFp: Record<string, any> = {};
  for (const r of results as any[]) {
    const fp = r.fingerprint_id;
    if (!byFp[fp]) {
      byFp[fp] = {
        fingerprint_id: fp,
        sensor_id: r.sensor_id,
        hostname: r.hostname,
        aliases: [],
        bands: [],
        first_seen: r.first_seen,
        last_seen: r.last_seen,
      };
    }
    byFp[fp].aliases.push(r.pseudonym);
    if (r.band && !byFp[fp].bands.includes(r.band)) byFp[fp].bands.push(r.band);
    byFp[fp].first_seen = Math.min(byFp[fp].first_seen ?? r.first_seen, r.first_seen ?? Infinity);
    byFp[fp].last_seen = Math.max(byFp[fp].last_seen ?? r.last_seen, r.last_seen ?? 0);
  }
  return jsonResponse(200, { devices: Object.values(byFp) }, origin);
}

async function handleGetDeviceIdentity(
  request: Request,
  env: Env
): Promise<Response> {
  const origin = requestCorsOrigin(env, request);
  const path = new URL(request.url).pathname;
  const match = path.match(/^\/api\/v1\/devices\/([^/]+)\/identity$/);
  if (!match) return jsonResponse(400, { error: "invalid path" }, origin);
  const deviceId = decodeURIComponent(match[1]);

  const identity = await env.DB.prepare(
    `SELECT i.pseudonym, i.sensor_id, i.manufacturer, i.brand, i.model_guess,
            i.device_class, i.mac_type, i.confidence, i.confidence_label,
            i.bssid_manufacturer, i.identity_json, i.fingerprint_id, i.last_seen,
            l.alias, l.owner, l.room, l.tags, l.notes, l.updated_at
     FROM device_identity i
     LEFT JOIN device_label l ON i.pseudonym = l.pseudonym
     WHERE i.pseudonym = ?`
  ).bind(deviceId).first();

  if (!identity) {
    // Device may not be in device_identity yet; try to return any stored label.
    const label = await env.DB.prepare(
      `SELECT pseudonym, alias, owner, room, tags, notes, updated_at
       FROM device_label WHERE pseudonym = ?`
    ).bind(deviceId).first();
    if (!label) return jsonResponse(404, { error: "not found" }, origin);
    return jsonResponse(200, { identity: label }, origin);
  }

  return jsonResponse(200, { identity }, origin);
}

async function handleUpdateDeviceIdentity(
  request: Request,
  env: Env
): Promise<Response> {
  const origin = requestCorsOrigin(env, request);
  const path = new URL(request.url).pathname;
  const match = path.match(/^\/api\/v1\/devices\/([^/]+)\/identity$/);
  if (!match) return jsonResponse(400, { error: "invalid path" }, origin);
  const deviceId = decodeURIComponent(match[1]);

  let body: any;
  try { body = await request.json(); } catch {
    return jsonResponse(400, { error: "invalid json" }, origin);
  }

  const allowed = new Set(["alias", "owner", "room", "tags", "notes"]);
  const updates: Record<string, string | null> = {};
  for (const key of allowed) {
    if (body[key] === undefined) continue;
    if (body[key] === null) {
      updates[key] = null;
      continue;
    }
    if (typeof body[key] !== "string") {
      return jsonResponse(400, { error: `invalid type for ${key}` }, origin);
    }
    const trimmed = body[key].trim();
    if (key === "tags") {
      if (trimmed === "") {
        updates[key] = null;
      } else {
        try {
          JSON.parse(trimmed); // validate JSON array/object
          updates[key] = trimmed;
        } catch {
          return jsonResponse(400, { error: "tags must be valid JSON" }, origin);
        }
      }
    } else {
      updates[key] = trimmed === "" ? null : trimmed;
    }
  }

  if (Object.keys(updates).length === 0) {
    return jsonResponse(400, { error: "no fields to update" }, origin);
  }

  const updatedAt = Math.floor(Date.now() / 1000);
  const setClause = Object.keys(updates).map(k => `${k} = ?`).join(", ");

  const existing = await env.DB.prepare(
    `SELECT pseudonym FROM device_label WHERE pseudonym = ?`
  ).bind(deviceId).first();

  if (!existing) {
    const cols = ["pseudonym", "updated_at", ...Object.keys(updates)];
    const vals = [deviceId, updatedAt, ...Object.values(updates)];
    const placeholders = vals.map(() => "?").join(", ");
    await env.DB.prepare(
      `INSERT INTO device_label (${cols.join(", ")}) VALUES (${placeholders})`
    ).bind(...vals).run();
  } else {
    await env.DB.prepare(
      `UPDATE device_label SET ${setClause}, updated_at = ? WHERE pseudonym = ?`
    ).bind(...Object.values(updates), updatedAt, deviceId).run();
  }

  const identity = await env.DB.prepare(
    `SELECT i.pseudonym, i.sensor_id, i.manufacturer, i.brand, i.model_guess,
            i.device_class, i.mac_type, i.confidence, i.confidence_label,
            i.bssid_manufacturer, i.identity_json, i.fingerprint_id, i.last_seen,
            l.alias, l.owner, l.room, l.tags, l.notes, l.updated_at
     FROM device_identity i
     LEFT JOIN device_label l ON i.pseudonym = l.pseudonym
     WHERE i.pseudonym = ?`
  ).bind(deviceId).first();

  if (identity) return jsonResponse(200, { identity }, origin);

  const label = await env.DB.prepare(
    `SELECT pseudonym, alias, owner, room, tags, notes, updated_at
     FROM device_label WHERE pseudonym = ?`
  ).bind(deviceId).first();

  return jsonResponse(200, { identity: label }, origin);
}

async function handleUnknownDevices(
  request: Request,
  env: Env
): Promise<Response> {
  const url = new URL(request.url);
  const origin = requestCorsOrigin(env, request);
  const limit = Math.min(parseInt(url.searchParams.get("limit") || "50"), 200);
  const hours = Math.min(parseInt(url.searchParams.get("hours") || "168"), 720);
  const cutoff = Math.floor(Date.now() / 1000) - hours * 3600;

  const rows = await env.DB.prepare(`
    SELECT t.pseudonym, t.sensor_id, t.status, t.first_seen, t.last_seen, t.alert_count,
           l.alias, l.owner, l.room, i.manufacturer, i.device_class, i.last_seen AS identity_last_seen,
           s.first_seen AS state_first_seen, s.last_seen AS state_last_seen, s.connection_count
    FROM device_trust t
    LEFT JOIN device_label l ON t.pseudonym = l.pseudonym
    LEFT JOIN device_identity i ON t.pseudonym = i.pseudonym
    LEFT JOIN device_state s ON t.pseudonym = s.device_id AND t.sensor_id = s.sensor_id
    WHERE t.status = 'unknown' AND (t.first_seen >= ? OR t.last_seen >= ?)
    ORDER BY t.first_seen DESC
    LIMIT ?
  `).bind(cutoff, cutoff, limit).all();

  return jsonResponse(200, { devices: rows.results || [] }, origin);
}

async function handleUpdateDeviceTrust(
  request: Request,
  env: Env
): Promise<Response> {
  const origin = requestCorsOrigin(env, request);
  const path = new URL(request.url).pathname;
  const match = path.match(/^\/api\/v1\/devices\/([^/]+)\/trust$/);
  if (!match) return jsonResponse(400, { error: "invalid path" }, origin);
  const deviceId = decodeURIComponent(match[1]);

  let body: any;
  try { body = await request.json(); } catch {
    return jsonResponse(400, { error: "invalid json" }, origin);
  }

  const status = body.status;
  if (!['known', 'ignored', 'unknown'].includes(status)) {
    return jsonResponse(400, { error: "status must be known, ignored or unknown" }, origin);
  }

  const now = Math.floor(Date.now() / 1000);
  await env.DB.prepare(
    `INSERT INTO device_trust (pseudonym, status, acknowledged_at, updated_at)
     VALUES (?, ?, ?, ?)
     ON CONFLICT(pseudonym) DO UPDATE SET
       status = excluded.status,
       acknowledged_at = excluded.acknowledged_at,
       updated_at = excluded.updated_at`
  ).bind(deviceId, status, now, now).run();

  const row = await env.DB.prepare('SELECT * FROM device_trust WHERE pseudonym = ?').bind(deviceId).first();
  return jsonResponse(200, { trust: row }, origin);
}

async function handleSessions(
  request: Request,
  env: Env
): Promise<Response> {
  const url = new URL(request.url);
  const hours = Math.min(parseInt(url.searchParams.get("hours") || "168"), 720);
  const deviceId = url.searchParams.get("device_id");
  const cutoff = Math.floor(Date.now() / 1000) - hours * 3600;
  const origin = requestCorsOrigin(env, request);

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
  const origin = requestCorsOrigin(env, request);
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
  const origin = requestCorsOrigin(env, request);
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

async function handleDevicePatterns(
  request: Request,
  env: Env
): Promise<Response> {
  const path = new URL(request.url).pathname;
  const m = path.match(/^\/api\/v1\/devices\/([^/]+)\/patterns$/);
  const deviceId = m ? decodeURIComponent(m[1]) : null;
  if (!deviceId) return jsonResponse(404, { error: "not found" });
  const url = new URL(request.url);
  const origin = requestCorsOrigin(env, request);
  const hours = Math.min(parseInt(url.searchParams.get("hours") || "168"), 720);
  const cutoff = Math.floor(Date.now() / 1000) - hours * 3600;

  const [{ results: hoursRows }, { results: weekdayRows }, { results: sessions }] = await Promise.all([
    env.DB.prepare(`
      SELECT CAST(strftime('%H', datetime(event_timestamp, 'unixepoch')) AS INTEGER) AS hour, COUNT(*) AS c
      FROM events
      WHERE event_type = 'device.connected' AND device_id = ? AND event_timestamp >= ?
      GROUP BY hour
      ORDER BY hour
    `).bind(deviceId, cutoff).all(),
    env.DB.prepare(`
      SELECT CAST(strftime('%w', datetime(event_timestamp, 'unixepoch')) AS INTEGER) AS weekday, COUNT(*) AS c
      FROM events
      WHERE event_type = 'device.connected' AND device_id = ? AND event_timestamp >= ?
      GROUP BY weekday
      ORDER BY weekday
    `).bind(deviceId, cutoff).all(),
    env.DB.prepare(`
      SELECT session_id, started_at, ended_at, duration_seconds, band, last_signal
      FROM device_sessions
      WHERE device_id = ? AND started_at >= ?
      ORDER BY started_at DESC
      LIMIT 100
    `).bind(deviceId, cutoff).all(),
  ]);

  const hourCounts = new Array(24).fill(0);
  for (const r of hoursRows as any[]) hourCounts[r.hour] = r.c;

  const weekdayCounts = new Array(7).fill(0);
  for (const r of weekdayRows as any[]) weekdayCounts[(r.weekday + 6) % 7] = r.c;

  const total = hourCounts.reduce((s, c) => s + c, 0);
  const topHours = hourCounts
    .map((c, i) => ({ hour: i, frequency: c, ratio: total ? Math.round((c / total) * 1000) / 1000 : 0 }))
    .sort((a, b) => b.frequency - a.frequency)
    .slice(0, 5);

  return jsonResponse(200, {
    device_id: deviceId,
    hours,
    total_observations: total,
    hour_counts: hourCounts,
    top_hours: topHours,
    weekday_counts: weekdayCounts,
    sessions: sessions || [],
  }, origin);
}

async function handleDeviceIps(
  request: Request,
  env: Env
): Promise<Response> {
  const path = new URL(request.url).pathname;
  const m = path.match(/^\/api\/v1\/devices\/([^/]+)\/ips$/);
  const deviceId = m ? decodeURIComponent(m[1]) : null;
  if (!deviceId) return jsonResponse(404, { error: "not found" });
  const origin = requestCorsOrigin(env, request);
  const url = new URL(request.url);
  const hours = Math.min(parseInt(url.searchParams.get("hours") || "168"), 720);
  const cutoff = Math.floor(Date.now() / 1000) - hours * 3600;

  const rows = await env.DB.prepare(`
    SELECT id, ip, mac, source, sensor_id, first_seen, last_seen, confidence
    FROM device_ip
    WHERE pseudonym = ? AND last_seen >= ?
    ORDER BY last_seen DESC, source, ip
    LIMIT 100
  `).bind(deviceId, cutoff).all();

  return jsonResponse(200, { device_id: deviceId, hours, ips: rows.results || [] }, origin);
}

async function handleAnalytics(
  request: Request,
  env: Env
): Promise<Response> {
  const url = new URL(request.url);
  const hours = Math.min(parseInt(url.searchParams.get("hours") || "24", 10), 168);
  const granularity = url.searchParams.get("granularity") === "day" ? "day" : "hour";
  const cutoff = Math.floor(Date.now() / 1000) - hours * 3600;
  const origin = requestCorsOrigin(env, request);

  const bucketSql =
    granularity === "day"
      ? "strftime('%Y-%m-%d', datetime(event_timestamp, 'unixepoch'))"
      : "strftime('%Y-%m-%dT%H:00:00', datetime(event_timestamp, 'unixepoch'))";
  const hourSql = "strftime('%H', datetime(event_timestamp, 'unixepoch'))";

  const fillBuckets = <T extends Record<string, unknown>>(
    rows: any[],
    bucketKey = "bucket"
  ): T[] => {
    const map = new Map<string, any>();
    for (const r of rows) {
      map.set(r[bucketKey], r);
    }
    const out: any[] = [];
    const now = Math.floor(Date.now() / 1000);
    const step = granularity === "day" ? 86400 : 3600;
    const start = Math.floor(cutoff / step) * step;
    for (let t = start; t <= now; t += step) {
      const b =
        granularity === "day"
          ? new Date(t * 1000).toISOString().slice(0, 10)
          : new Date(t * 1000).toISOString().slice(0, 13) + ":00:00";
      out.push(map.get(b) || { bucket: b, count: 0 });
    }
    return out;
  };

  const [{ results: connected }, { results: disconnected }, { results: nearby }, { results: rssi }]: any[] =
    await Promise.all([
      env.DB.prepare(
        `SELECT ${bucketSql} AS bucket, COUNT(*) AS count
         FROM events
         WHERE event_type = 'device.connected' AND event_timestamp >= ?
         GROUP BY bucket
         ORDER BY bucket`
      ).bind(cutoff).all(),
      env.DB.prepare(
        `SELECT ${bucketSql} AS bucket, COUNT(*) AS count
         FROM events
         WHERE event_type = 'device.disconnected' AND event_timestamp >= ?
         GROUP BY bucket
         ORDER BY bucket`
      ).bind(cutoff).all(),
      env.DB.prepare(
        `SELECT ${bucketSql} AS bucket, COUNT(*) AS count
         FROM events
         WHERE event_type IN ('device.proximity_changed', 'device.connected')
           AND event_timestamp >= ?
           AND (
             COALESCE(json_extract(payload_json, '$.in_radius'), json_extract(payload_json, '$.payload.in_radius')) = 1
             OR lower(COALESCE(json_extract(payload_json, '$.proximity_detail.zone'), json_extract(payload_json, '$.payload.proximity_detail.zone'))) IN ('immediate', 'near')
           )
         GROUP BY bucket
         ORDER BY bucket`
      ).bind(cutoff).all(),
      env.DB.prepare(
        `SELECT ${bucketSql} AS bucket,
                ROUND(AVG(COALESCE(json_extract(payload_json, '$.rssi_dbm'), json_extract(payload_json, '$.payload.rssi_dbm'))), 1) AS avg_rssi,
                ROUND(MIN(COALESCE(json_extract(payload_json, '$.rssi_dbm'), json_extract(payload_json, '$.payload.rssi_dbm'))), 1) AS min_rssi,
                ROUND(MAX(COALESCE(json_extract(payload_json, '$.rssi_dbm'), json_extract(payload_json, '$.payload.rssi_dbm'))), 1) AS max_rssi
         FROM events
         WHERE event_type IN ('device.connected', 'device.proximity_changed', 'device.signal_changed')
           AND event_timestamp >= ?
           AND COALESCE(json_extract(payload_json, '$.rssi_dbm'), json_extract(payload_json, '$.payload.rssi_dbm')) IS NOT NULL
         GROUP BY bucket
         ORDER BY bucket`
      ).bind(cutoff).all(),
    ]);

  const { results: proximityRows }: any = await env.DB.prepare(
    `SELECT ${bucketSql} AS bucket,
            SUM(CASE WHEN lower(COALESCE(json_extract(payload_json, '$.proximity_detail.zone'), json_extract(payload_json, '$.payload.proximity_detail.zone'))) = 'immediate' THEN 1 ELSE 0 END) AS immediate,
            SUM(CASE WHEN lower(COALESCE(json_extract(payload_json, '$.proximity_detail.zone'), json_extract(payload_json, '$.payload.proximity_detail.zone'))) = 'near' THEN 1 ELSE 0 END) AS near,
            SUM(CASE WHEN lower(COALESCE(json_extract(payload_json, '$.proximity_detail.zone'), json_extract(payload_json, '$.payload.proximity_detail.zone'))) = 'medium' THEN 1 ELSE 0 END) AS medium,
            SUM(CASE WHEN lower(COALESCE(json_extract(payload_json, '$.proximity_detail.zone'), json_extract(payload_json, '$.payload.proximity_detail.zone'))) = 'far' THEN 1 ELSE 0 END) AS far,
            SUM(CASE WHEN COALESCE(json_extract(payload_json, '$.proximity_detail.zone'), json_extract(payload_json, '$.payload.proximity_detail.zone')) IS NULL THEN 1 ELSE 0 END) AS unknown
     FROM events
     WHERE event_type IN ('device.connected', 'device.proximity_changed')
       AND event_timestamp >= ?
     GROUP BY bucket
     ORDER BY bucket`
  ).bind(cutoff).all();

  const { results: activity }: any = await env.DB.prepare(
    `SELECT ${hourSql} AS hour, COUNT(*) AS count
     FROM events
     WHERE event_type = 'device.connected' AND event_timestamp >= ?
     GROUP BY hour
     ORDER BY hour`
  ).bind(cutoff).all();

  const { results: topDwell }: any = await env.DB.prepare(
    `SELECT s.device_id,
            s.total_connected_time AS total_seconds,
            s.connection_count AS sessions,
            s.last_signal,
            i.manufacturer,
            i.device_class
     FROM device_state s
     LEFT JOIN device_identity i ON s.device_id = i.pseudonym
     ORDER BY s.total_connected_time DESC
     LIMIT 10`
  ).all();

  const [{ results: totals }, { results: peak }]: any[] = await Promise.all([
    env.DB.prepare(
      `SELECT
        (SELECT COUNT(*) FROM events WHERE event_type = 'device.connected' AND event_timestamp >= ?) AS total_connected,
        (SELECT COUNT(*) FROM events WHERE event_type = 'device.disconnected' AND event_timestamp >= ?) AS total_disconnected,
        (SELECT COUNT(DISTINCT device_id) FROM events WHERE event_timestamp >= ?) AS total_observed,
        (SELECT COUNT(*) FROM events WHERE event_type = 'device.proximity_changed' AND COALESCE(json_extract(payload_json, '$.in_radius'), json_extract(payload_json, '$.payload.in_radius')) = 1 AND event_timestamp >= ?) AS total_nearby_events,
        (SELECT AVG(duration_seconds) FROM device_sessions WHERE started_at >= ? AND duration_seconds IS NOT NULL) AS avg_session_seconds,
        (SELECT ROUND(SUM(duration_seconds) / 3600.0, 2) FROM device_sessions WHERE started_at >= ? AND duration_seconds IS NOT NULL) AS total_dwell_hours`
    ).bind(cutoff, cutoff, cutoff, cutoff, cutoff, cutoff).all(),
    env.DB.prepare(
      `SELECT ${hourSql} AS hour, COUNT(*) AS c
       FROM events
       WHERE event_type = 'device.connected' AND event_timestamp >= ?
       GROUP BY hour
       ORDER BY c DESC
       LIMIT 1`
    ).bind(cutoff).all(),
  ]);

  const t = totals[0] || {};
  const p = peak[0];

  // Hourly recurrence pattern per device (last 14 days) and anomaly detection.
  const patternWindow = Math.min(hours, 14 * 24);
  const patternCutoff = Math.floor(Date.now() / 1000) - patternWindow * 3600;
  const { results: patternRows }: any = await env.DB.prepare(`
    SELECT
      device_id,
      CAST(strftime('%H', datetime(event_timestamp, 'unixepoch')) AS INTEGER) AS hour,
      COUNT(*) AS c
    FROM events
    WHERE event_type = 'device.connected'
      AND event_timestamp >= ?
    GROUP BY device_id, hour
    ORDER BY device_id, c DESC
  `).bind(patternCutoff).all();

  const patternByDevice = new Map<string, Map<number, number>>();
  for (const r of patternRows) {
    if (!patternByDevice.has(r.device_id)) patternByDevice.set(r.device_id, new Map());
    patternByDevice.get(r.device_id)!.set(r.hour, r.c);
  }

  const { results: recentEvents }: any = await env.DB.prepare(`
    SELECT
      e.device_id,
      e.event_type,
      e.event_timestamp,
      COALESCE(json_extract(e.payload_json, '$.rssi_dbm'), json_extract(e.payload_json, '$.payload.rssi_dbm')) AS rssi,
      COALESCE(json_extract(e.payload_json, '$.in_radius'), json_extract(e.payload_json, '$.payload.in_radius')) AS in_radius,
      COALESCE(json_extract(e.payload_json, '$.proximity_detail.zone'), json_extract(e.payload_json, '$.payload.proximity_detail.zone')) AS zone
    FROM events e
    WHERE e.event_timestamp >= ?
      AND e.event_type IN ('device.connected', 'device.disconnected')
    ORDER BY e.event_timestamp DESC
    LIMIT 200
  `).bind(cutoff).all();

  const anomalies: any[] = [];
  const seenAnomaly = new Set<string>();
  const addAnomaly = (a: any) => {
    const key = `${a.type}:${a.device_id}:${a.timestamp}`;
    if (seenAnomaly.has(key)) return;
    seenAnomaly.add(key);
    anomalies.push(a);
  };

  for (const e of recentEvents) {
    const hour = new Date(e.event_timestamp * 1000).getUTCHours();
    const hist = patternByDevice.get(e.device_id);
    if (e.event_type === 'device.connected' && hist) {
      const entries = Array.from(hist.entries());
      const top = entries.slice(0, 3).map(([h]) => h);
      const total = entries.reduce((s, [, c]) => s + c, 0);
      const hourCount = hist.get(hour) || 0;
      if (total >= 5 && !top.includes(hour) && hourCount <= Math.max(1, total * 0.05)) {
        addAnomaly({
          type: 'unusual_hour',
          device_id: e.device_id,
          timestamp: e.event_timestamp,
          hour,
          message: `Conexión inusual a las ${String(hour).padStart(2, '0')}:00`,
          severity: 'low',
        });
      }
    }
  }

  const { results: newDevices }: any = await env.DB.prepare(`
    SELECT s.device_id, s.first_seen, s.last_seen, s.connection_count
    FROM device_state s
    WHERE s.first_seen >= ?
    ORDER BY s.first_seen DESC
    LIMIT 20
  `).bind(cutoff).all();

  for (const d of newDevices) {
    addAnomaly({
      type: 'new_device',
      device_id: d.device_id,
      timestamp: d.first_seen,
      message: 'Nuevo dispositivo en la red',
      severity: d.connection_count <= 1 ? 'medium' : 'low',
    });
  }

  const { results: newNetworks }: any = await env.DB.prepare(`
    SELECT ssid, bssid_pseudonym, band, first_seen, sensor_id
    FROM wifi_network_observation
    WHERE first_seen >= ?
    ORDER BY first_seen DESC
    LIMIT 20
  `).bind(cutoff).all();

  for (const n of newNetworks) {
    addAnomaly({
      type: 'new_network',
      network_id: n.bssid_pseudonym,
      ssid: n.ssid,
      band: n.band,
      timestamp: n.first_seen,
      message: `Nueva red Wi-Fi detectada: ${n.ssid || n.bssid_pseudonym}`,
      severity: 'low',
    });
  }

  const { results: proximityAnomalies }: any = await env.DB.prepare(`
    SELECT
      e.device_id,
      e.event_timestamp,
      COALESCE(json_extract(e.payload_json, '$.proximity_detail.zone'), json_extract(e.payload_json, '$.payload.proximity_detail.zone')) AS zone
    FROM events e
    WHERE e.event_type = 'device.proximity_changed'
      AND e.event_timestamp >= ?
      AND lower(COALESCE(json_extract(e.payload_json, '$.proximity_detail.zone'), json_extract(e.payload_json, '$.payload.proximity_detail.zone'))) = 'immediate'
    ORDER BY e.event_timestamp DESC
    LIMIT 20
  `).bind(cutoff).all();

  for (const p of proximityAnomalies) {
    addAnomaly({
      type: 'proximity_immediate',
      device_id: p.device_id,
      timestamp: p.event_timestamp,
      message: 'Dispositivo muy cerca del sensor',
      severity: 'info',
    });
  }

  anomalies.sort((a, b) => b.timestamp - a.timestamp);

  const patterns: any[] = Array.from(patternByDevice.entries()).map(([device_id, hoursMap]) => {
    const sorted = Array.from(hoursMap.entries()).sort((a, b) => b[1] - a[1]);
    const total = sorted.reduce((s, [, c]) => s + c, 0);
    return {
      device_id,
      top_hours: sorted.slice(0, 5).map(([h, c]) => ({ hour: h, frequency: c, ratio: Math.round((c / total) * 1000) / 1000 })),
      total_observations: total,
      weekday_counts: new Array(7).fill(0),
    };
  });

  const { results: weekdayRows }: any = await env.DB.prepare(`
    SELECT
      device_id,
      CAST(strftime('%w', datetime(event_timestamp, 'unixepoch')) AS INTEGER) AS weekday,
      COUNT(*) AS c
    FROM events
    WHERE event_type = 'device.connected'
      AND event_timestamp >= ?
    GROUP BY device_id, weekday
    ORDER BY device_id, c DESC
  `).bind(patternCutoff).all();

  const weekdayByDevice = new Map<string, number[]>();
  for (const r of weekdayRows) {
    if (!weekdayByDevice.has(r.device_id)) weekdayByDevice.set(r.device_id, new Array(7).fill(0));
    weekdayByDevice.get(r.device_id)![(r.weekday + 6) % 7] = r.c; // shift so Monday=0
  }

  for (const p of patterns) {
    const wd = weekdayByDevice.get(p.device_id);
    p.weekday_counts = wd || new Array(7).fill(0);
  }

  const response = {
    hours,
    granularity,
    cutoff,
    connectionTimeline: fillBuckets(connected, "bucket").map((r: any) => ({
      bucket: r.bucket,
      connected: r.count || 0,
    })),
    disconnectionTimeline: fillBuckets(disconnected, "bucket").map((r: any) => ({
      bucket: r.bucket,
      disconnected: r.count || 0,
    })),
    nearbyTimeline: fillBuckets(nearby, "bucket").map((r: any) => ({
      bucket: r.bucket,
      nearby: r.count || 0,
    })),
    rssiTimeline: fillBuckets(rssi, "bucket").map((r: any) => ({
      bucket: r.bucket,
      avg: r.avg_rssi ?? null,
      min: r.min_rssi ?? null,
      max: r.max_rssi ?? null,
    })),
    proximityTimeline: fillBuckets(proximityRows, "bucket").map((r: any) => ({
      bucket: r.bucket,
      immediate: r.immediate || 0,
      near: r.near || 0,
      medium: r.medium || 0,
      far: r.far || 0,
      unknown: r.unknown || 0,
    })),
    activityByHour: Array.from({ length: 24 }, (_, i) => {
      const h = String(i).padStart(2, "0");
      const row = activity.find((a: any) => String(a.hour).padStart(2, "0") === h);
      return { hour: i, count: row ? row.count : 0 };
    }),
    topDwellers: topDwell.map((d: any) => ({
      device_id: d.device_id,
      manufacturer: d.manufacturer || null,
      device_class: d.device_class || null,
      total_seconds: d.total_seconds || 0,
      total_minutes: Math.round((d.total_seconds || 0) / 60),
      sessions: d.sessions || 0,
      last_signal: d.last_signal ?? null,
    })),
    totals: {
      total_connected: t.total_connected || 0,
      total_disconnected: t.total_disconnected || 0,
      total_observed: t.total_observed || 0,
      total_nearby_events: t.total_nearby_events || 0,
      avg_session_seconds: Math.round(t.avg_session_seconds || 0),
      total_dwell_hours: t.total_dwell_hours || 0,
      peak_hour: p ? parseInt(p.hour, 10) : null,
      peak_hour_connections: p ? p.c : 0,
    },
    patterns,
    anomalies: anomalies.slice(0, 50),
  };

  return jsonResponse(200, response, origin);
}

async function handleTimeline(
  request: Request,
  env: Env
): Promise<Response> {
  const url = new URL(request.url);
  const hours = Math.min(parseInt(url.searchParams.get("hours") || "24"), 168);
  const cutoff = Math.floor(Date.now() / 1000) - hours * 3600;
  const origin = requestCorsOrigin(env, request);

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
  fingerprint_id?: string | null;
  fingerprint_method?: string | null;
}

async function handleCollectorSync(
  request: Request,
  env: Env
): Promise<Response> {
  const origin = requestCorsOrigin(env, request);
  const sensorId = request.headers.get("X-Detectic-Sensor") || "";
  const signature = request.headers.get("X-Detectic-Signature") || "";
  const timestamp = request.headers.get("X-Detectic-Timestamp");
  const bodyText = await request.text();

  if (bodyText.length > 4 * 1024 * 1024) {
    return jsonResponse(400, { error: "body too large" }, origin);
  }
  const bearer = request.headers.get("Authorization") || "";
  let auth = await verifyAuth(env, sensorId, signature, bodyText, timestamp);
  const bearerOk = verifyBearerToken(env, sensorId, request);
  const idCheck = auth.ok ? { ok: false, reason: "" } : await verifySnapshotId(env, sensorId, bodyText, timestamp);
  const idOk = idCheck.ok;
  console.log(`[handleCollectorSync] auth debug sensor=${sensorId} signature_len=${signature.length} timestamp=${timestamp} bearer_present=${bearer.length > 0} bearer_ok=${bearerOk} id_ok=${idOk} id_reason=${idCheck.reason || ""}`);
  if (!auth.ok && bearerOk) {
    auth = { ok: true, reason: "bearer_token" };
  }
  if (!auth.ok && idOk) {
    auth = { ok: true, reason: "snapshot_id" };
  }
  if (!auth.ok) {
    console.warn(`[handleCollectorSync] auth failed sensor=${sensorId} reason=${auth.reason}`);
    return jsonResponse(401, { error: "unauthorized", reason: auth.reason }, origin);
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
              bssid_pseudonym, identity_json, fingerprint_id, fingerprint_method)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`
          ).bind(
            captureId, d.pseudonym ?? "", d.hostname ?? null, d.band ?? null,
            d.signal_strength ?? null, d.signal_level ?? null, d.noise ?? null,
            d.operating_standard ?? null, d.tx_rate_kbps ?? null, d.rx_rate_kbps ?? null,
            d.status ?? null, d.bssid_pseudonym ?? null,
            d.identity ? JSON.stringify(d.identity) : null,
            d.fingerprint_id ?? null, d.fingerprint_method ?? null
          )
        );
      }
      await env.DB.batch(idStmts);
      results.devices += dlist.length;

      // Register MAC aliases for the stable fingerprint_id (huella) from the
      // capture's devices. This keeps the alias map updated even for sensors
      // that push captures but not device.* events.
      const aliasStmts: any[] = [];
      for (const d of dlist) {
        if (d.fingerprint_id && d.pseudonym) {
          aliasStmts.push(
            env.DB.prepare(
              `INSERT INTO device_aliases (fingerprint_id, pseudonym, sensor_id, hostname, band, first_seen, last_seen)
               VALUES (?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(fingerprint_id, pseudonym, sensor_id) DO UPDATE SET
                 hostname = COALESCE(excluded.hostname, device_aliases.hostname),
                 band = COALESCE(excluded.band, device_aliases.band),
                 last_seen = excluded.last_seen`
            ).bind(d.fingerprint_id, d.pseudonym, sensor, d.hostname ?? null, d.band ?? null, now, now)
          );
        }
      }
      if (aliasStmts.length > 0) {
        await env.DB.batch(aliasStmts);
      }

      // --- derived identity / network fingerprint upserts ---
      const fStmts: any[] = [];
      for (const d of dlist) {
        if (!d.identity) continue;
        const id = d.identity;
        fStmts.push(
          env.DB.prepare(
            `INSERT INTO device_identity
             (pseudonym, sensor_id, manufacturer, brand, model_guess, device_class,
              mac_type, confidence, confidence_label, bssid_manufacturer, identity_json, last_seen, fingerprint_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(pseudonym, sensor_id) DO UPDATE SET
               manufacturer=excluded.manufacturer, brand=excluded.brand,
               model_guess=excluded.model_guess, device_class=excluded.device_class,
               mac_type=excluded.mac_type, confidence=excluded.confidence,
               confidence_label=excluded.confidence_label,
               bssid_manufacturer=excluded.bssid_manufacturer,
               identity_json=excluded.identity_json, last_seen=excluded.last_seen,
               fingerprint_id=COALESCE(excluded.fingerprint_id, device_identity.fingerprint_id)`
          ).bind(
            d.pseudonym, sensor, id.manufacturer ?? null, id.brand ?? null,
            id.model_guess ?? null, id.device_class ?? null, id.mac_type ?? null,
            id.confidence ?? null, id.confidence_label ?? null,
            id.bssid_manufacturer ?? null, JSON.stringify(id), now,
            d.fingerprint_id ?? null
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

  // --- sensor heartbeat ---
  // The collector sync path does not carry canonical events, so it does not
  // trigger the device_state/ap_state side effects that normally update
  // sensors.last_seen. Update it explicitly so the dashboard knows the sensor
  // is alive even when no events are flowing.
  await env.DB.prepare(
    `INSERT INTO sensors (id, created_at, last_seen) VALUES (?, ?, ?)
     ON CONFLICT(id) DO UPDATE SET
       last_seen = excluded.last_seen,
       created_at = COALESCE(sensors.created_at, excluded.created_at)`
  ).bind(sensor, now, now).run();

  return jsonResponse(200, { synced: true, ...results }, origin);
}

async function handleBackfillApState(
  request: Request,
  env: Env
): Promise<Response> {
  const origin = requestCorsOrigin(env, request);
  if (!verifyMasterAuth(request, env)) {
    return jsonResponse(401, { error: "unauthorized" }, origin);
  }

  const url = new URL(request.url);
  const hours = Math.min(parseInt(url.searchParams.get("hours") || "168", 10), 720);
  const sensorId = url.searchParams.get("sensor_id");
  const dryRun = url.searchParams.get("dry_run") === "1";
  const cutoff = Math.floor(Date.now() / 1000) - hours * 3600;

  const conds: string[] = ["event_timestamp >= ?", "event_type IN ('network.detected', 'network.changed', 'network.disappeared')"];
  const binds: (string | number)[] = [cutoff];
  if (sensorId) {
    conds.push("sensor_id = ?");
    binds.push(sensorId);
  }

  const { results } = await env.DB.prepare(
    `SELECT sensor_id, event_type, event_timestamp, device_id, payload_json
     FROM events
     WHERE ${conds.join(" AND ")}
     ORDER BY event_timestamp ASC, sequence ASC, id ASC`
  ).bind(...binds).all();

  let processed = 0;
  let failed = 0;
  const now = Math.floor(Date.now() / 1000);

  for (const row of results as any[]) {
    try {
      const payload = row.payload_json ? JSON.parse(row.payload_json) : {};
      // Normalize whether payload_json is the inner payload or the full envelope.
      const inner = payload.payload && typeof payload.payload === "object" && payload.type ? payload.payload : payload;
      const evt = {
        type: row.event_type,
        timestamp: row.event_timestamp,
        device_id: row.device_id,
        payload: inner,
      };
      const stmts: D1PreparedStatement[] = [];
      applyApSideEffects(
        env,
        stmts,
        row.sensor_id,
        evt.type,
        evt.timestamp,
        now,
        evt.device_id || "",
        evt.payload
      );
      if (stmts.length > 0) {
        if (!dryRun) await env.DB.batch(stmts);
        processed++;
      }
    } catch (e: any) {
      console.error("backfill ap_state failed for event", row.event_id, e?.message || e);
      failed++;
    }
  }

  return jsonResponse(200, {
    dry_run: dryRun,
    hours,
    sensor_id: sensorId,
    events_scanned: results.length,
    processed,
    failed,
    ap_state_rows_after: dryRun ? null : (await env.DB.prepare("SELECT COUNT(*) AS c FROM ap_state").first<{ c: number }>())?.c,
  }, origin);
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

async function handleReportsNetworks(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  const hours = Math.min(parseInt(url.searchParams.get('hours') || '24'), 720);
  const origin = request.headers.get('Origin') || undefined;
  const data = await fetchRealtimeNetworks(env, hours);
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

type ReportConfig = {
  id: number;
  enabled: number;
  frequency_hours: number;
  changes_only: number;
  top_devices: number;
  new_detections: number;
  nearby_aps: number;
  email_to: string | null;
  email_subject: string | null;
  updated_at: number;
};

async function handleReportQueue(request: Request, env: Env): Promise<Response> {
  const origin = requestCorsOrigin(env, request);
  const url = new URL(request.url);
  const limit = Math.min(parseInt(url.searchParams.get('limit') || '1'), 10);

  if (request.method === 'GET') {
    const rows = await env.DB.prepare(
      'SELECT id, report_id, scheduled_at, generated_at, html, text, config_json FROM email_queue WHERE status = ? ORDER BY scheduled_at ASC LIMIT ?'
    ).bind('pending', limit).all();
    return jsonResponse(200, { queue: rows.results || [] }, origin);
  }

  if (request.method === 'POST') {
    const match = url.pathname.match(/^\/api\/v1\/reports\/queue\/([^/]+)$/);
    if (!match) return jsonResponse(400, { error: 'invalid path' }, origin);
    const id = match[1];
    let body: any;
    try { body = await request.json(); } catch { return jsonResponse(400, { error: 'invalid json' }, origin); }
    const status = body.status === 'delivered' ? 'delivered' : 'failed';
    const error = body.error || null;
    const now = Math.floor(Date.now() / 1000);
    if (status === 'delivered') {
      await env.DB.prepare(
        'UPDATE email_queue SET status = ?, sent_at = ?, error = NULL WHERE id = ?'
      ).bind(status, now, id).run();
    } else {
      await env.DB.prepare(
        'UPDATE email_queue SET attempts = attempts + 1, last_attempt_at = ?, error = ? WHERE id = ?'
      ).bind(now, error, id).run();
    }
    return jsonResponse(200, { ok: true }, origin);
  }

  return jsonResponse(405, { error: 'method not allowed' }, origin);
}

async function handleGetReportConfig(request: Request, env: Env): Promise<Response> {
  const origin = requestCorsOrigin(env, request);
  await env.DB.prepare('INSERT OR IGNORE INTO report_config (id) VALUES (1)').run();
  const row = await env.DB.prepare('SELECT * FROM report_config WHERE id = 1').first() as ReportConfig | null;
  return jsonResponse(200, { config: row }, origin);
}

async function handleUpdateReportConfig(request: Request, env: Env): Promise<Response> {
  const origin = requestCorsOrigin(env, request);
  let body: any;
  try { body = await request.json(); } catch { return jsonResponse(400, { error: 'invalid json' }, origin); }

  const allowed = new Set(['enabled', 'frequency_hours', 'changes_only', 'top_devices', 'new_detections', 'nearby_aps', 'email_to', 'email_subject']);
  const updates: Record<string, any> = {};
  for (const key of allowed) {
    if (body[key] === undefined) continue;
    if (['enabled', 'frequency_hours', 'changes_only', 'top_devices', 'new_detections', 'nearby_aps'].includes(key)) {
      updates[key] = Number(body[key]);
      if (!Number.isFinite(updates[key])) return jsonResponse(400, { error: `invalid ${key}` }, origin);
    } else {
      updates[key] = typeof body[key] === 'string' ? body[key].trim() || null : null;
    }
  }

  if (Object.keys(updates).length === 0) return jsonResponse(400, { error: 'no fields' }, origin);
  updates.updated_at = Math.floor(Date.now() / 1000);

  const setClause = Object.keys(updates).map(k => `${k} = ?`).join(', ');
  await env.DB.prepare('INSERT OR IGNORE INTO report_config (id) VALUES (1)').run();
  await env.DB.prepare(`UPDATE report_config SET ${setClause} WHERE id = 1`).bind(...Object.values(updates)).run();

  const row = await env.DB.prepare('SELECT * FROM report_config WHERE id = 1').first() as ReportConfig | null;
  return jsonResponse(200, { config: row }, origin);
}

type ReportOptions = {
  hours: number;
  changes_only: boolean;
  top_devices: number;
  new_detections: boolean;
  nearby_aps: boolean;
};

type EmailReport = {
  html: string;
  text: string;
  reportId: string;
  sensorId: string;
  generatedAt: number;
};

async function buildEmailReport(env: Env, opts: ReportOptions): Promise<EmailReport> {
  const startMs = Date.now();
  const data = await fetchRealtimeSummary(env, opts.hours);
  const now = new Date();
  const scheduled = new Date(Math.floor(now.getTime() / 300000) * 300000); // round to 5 min
  const captureStart = new Date(data.generated_at || now.getTime());
  const captureEnd = now;
  const cutoffMs = now.getTime() - opts.hours * 3600 * 1000;

  const [idRows, devRows, networkData, apRows, labelRows] = await Promise.all([
    env.DB.prepare('SELECT pseudonym, manufacturer, brand, model_guess, device_class, last_seen FROM device_identity').all(),
    env.DB.prepare(`SELECT d.pseudonym, d.hostname, d.operating_standard, d.identity_json, c.started_at
                     FROM collector_devices d
                     JOIN collector_captures c ON d.capture_id = c.capture_id
                     ORDER BY c.started_at DESC`).all(),
    fetchRealtimeNetworks(env, opts.hours),
    env.DB.prepare(`SELECT ssid, band, current_signal, status, sensor_id, first_seen, last_seen, online_since,
                           observation_count, w_mode, security, proximity, proximity_detail
                    FROM ap_state
                    WHERE last_seen >= ?
                    ORDER BY last_seen DESC LIMIT 50`)
      .bind(Math.floor(Date.now() / 1000) - 24 * 3600)
      .all(),
    env.DB.prepare('SELECT pseudonym, alias FROM device_label').all(),
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
  for (const r of labelRows.results as any[]) {
    const existing = identityMap.get(r.pseudonym);
    if (existing) existing.alias = r.alias;
    else identityMap.set(r.pseudonym, { alias: r.alias });
  }

  const allDevices = (data.devices || []) as any[];
  const connected = allDevices.filter((d: any) => d.connected);
  const outOfRange = allDevices.filter((d: any) => !d.connected);
  const detectedCount = allDevices.length;
  const connectedCount = connected.length;
  const offCount = outOfRange.length;

  const nameOf = (d: any) => {
    const id = { ...(identityMap.get(d.device_id) || {}), ...d };
    return deviceNameFrom(id, d.device_id.slice(0, 16));
  };
  const detailsOf = (d: any) => deviceDetails({ ...(identityMap.get(d.device_id) || {}), ...d });

  const byName = (a: any, b: any) => nameOf(a).localeCompare(nameOf(b), undefined, { sensitivity: 'base' });
  connected.sort(byName);
  outOfRange.sort(byName);

  const newDetections = allDevices.filter((d: any) => (d.first_seen || 0) >= cutoffMs);
  const topDevices = [...allDevices]
    .sort((a: any, b: any) => (b.event_count || 0) - (a.event_count || 0))
    .slice(0, opts.top_devices);

  const deviceRow = (d: any, style: 'large' | 'small' = 'large') => {
    const name = nameOf(d);
    const details = detailsOf(d);
    const level = signalLevel(d.last_signal);
    const prox = d.proximity || d.proximity_detail?.zone_label || '';
    const dist = d.distance_m != null ? `~${Math.round(d.distance_m)} m` : distanceLabel(d.rssi_dbm ?? d.last_signal);
    const trend = d.trend ? ` · ${d.trend}` : '';
    const heat = d.heat != null ? ` · intensidad ${d.heat}` : '';
    const size = style === 'large' ? 'margin-bottom:14px;padding:10px' : 'margin-bottom:10px;padding:8px';
    return `<div style="${size};border:1px solid #d0d7de;border-radius:8px;background:#fff">
      <b>${escHtml(name)}</b>
      <div style="font-size:11px;color:#57606a">${escHtml(details)}</div>
      <div style="font-size:12px;margin:4px 0">📶 ${signalBars(level)} ${signalLabel(level)} (nivel ${level}/4) · ${escHtml(prox)}${trend}${heat}</div>
      <div style="font-size:12px;color:#57606a">📡 ${d.band || '—'} · 📍 ${dist} · última ${fmtDateTime(d.last_seen)} · total ${msToDuration(d.last_seen - d.first_seen)}</div>
    </div>`;
  };

  const smallDeviceRow = (d: any, note: string) => `<div style="margin-bottom:10px;padding:8px;border:1px solid #d0d7de;border-radius:6px;background:#fff">
    <b>${escHtml(nameOf(d))}</b>
    <span style="font-size:12px;color:#57606a"> · ${escHtml(note)} · ${d.band || '—'} · última ${fmtDateTime(d.last_seen)}</span>
  </div>`;

  const connectedRows = connected.map((d: any) => deviceRow(d)).join('') || '<p style="color:#57606a">Ningún dispositivo conectado en el período.</p>';
  const outRows = outOfRange.map((d: any) => deviceRow(d, 'small')).join('') || '<p style="color:#57606a">Ningún dispositivo fuera de rango.</p>';

  const topRows = opts.top_devices > 0 && topDevices.length
    ? topDevices.map((d: any, i: number) => smallDeviceRow(d, `#${i + 1} · ${d.event_count || 0} observaciones`)).join('')
    : '';

  const newRows = opts.new_detections && newDetections.length
    ? newDetections.map((d: any) => smallDeviceRow(d, 'nueva detección en este período')).join('')
    : '';

  const nowMs = Date.now();
  const toMs = (ts: number) => (ts < 1e12 ? ts * 1000 : ts);
  const rawNetworks = (networkData.networks?.length ? networkData.networks : apRows.results) as any[];
  let networks = rawNetworks.length ? rawNetworks : (apRows.results as any[]);

  const sensorId = (networks[0]?.sensor_id as string) ||
    (data.devices?.[0]?.sensor_id as string) ||
    'desconocido';

  networks.sort((a: any, b: any) => {
    const labelA = String(a.ssid || a.ap_id || '').toLowerCase();
    const labelB = String(b.ssid || b.ap_id || '').toLowerCase();
    return labelA.localeCompare(labelB, undefined, { sensitivity: 'base' });
  });

  if (opts.nearby_aps) {
    networks = networks.filter((n: any) => {
      const pd = n.proximity_detail || {};
      return n.status !== 'OFFLINE' && (pd.in_radius || ['immediate', 'near'].includes(String(n.proximity || pd.zone || '').toLowerCase()));
    });
  }

  const newNetworks = opts.new_detections
    ? networks.filter((n: any) => (n.first_seen || 0) >= cutoffMs)
    : [];

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
    const prox = n.proximity || n.proximity_detail?.zone_label || '';
    const dist = n.proximity_detail?.distance_m != null ? ` · ~${Math.round(n.proximity_detail.distance_m)} m` : '';
    return `<div style="margin-bottom:10px;padding:10px;border:1px solid #d0d7de;border-radius:8px;background:#fff">
      <b>${escHtml(n.ssid || n.ap_id || '—')}</b>
      <div style="font-size:12px;color:#57606a">${dot} ${statusText} · 📡 ${n.band || '—'} · ${n.w_mode || '—'} · obs: ${n.event_count ?? n.observation_count ?? 0} · ${escHtml(prox)}${dist}</div>
      <div style="font-size:11px;color:#57606a">primera detección: ${fmtDateTime(firstMs)} · última: ${fmtDateTime(lastMs)} · detectada: ${msToDuration(duration)} · desde última: ${msToDuration(sinceLast)}${n.online_since ? ` · activa: ${msToDuration(active)}` : ''}</div>
    </div>`;
  };

  const networkRows = networks.map(networkStatus).join('') || '<p style="color:#57606a">No se detectaron redes.</p>';
  const offlineRows = (rawNetworks.length ? rawNetworks : (apRows.results as any[]))
    .filter((n: any) => n.status === 'OFFLINE' || (nowMs - toMs(n.last_seen) > STALE_MS && n.status !== 'OFFLINE'))
    .map(networkStatus)
    .join('') || '<p style="color:#57606a">Sin caídas de red registradas.</p>';
  const newNetworkRows = newNetworks.map(networkStatus).join('') || '';

  const bands = new Set<string>();
  const standards = new Set<string>();
  for (const n of (rawNetworks.length ? rawNetworks : (apRows.results as any[]))) {
    if (n.band) bands.add(n.band);
    if (n.w_mode) standards.add(n.w_mode);
  }

  const reportId = `detectic-${sensorId}-${scheduled.toISOString().replace(/[-:]/g, '').slice(0, 15)}`;

  const sections: string[] = [];
  const textLines: string[] = [];

  const addSection = (title: string, html: string, text: string) => {
    if (!html) return;
    sections.push(`<div class="card"><h3>${title}</h3>${html}</div>`);
    textLines.push(`\n${title}\n${text}`);
  };

  const headerHtml = `<div class="card">
  <h2>🛰️ DETECTIC — Informe de Observación Autónoma</h2>
  <div class="meta"><b>Sensor:</b> ${escHtml(sensorId)}</div>
  <div class="meta"><b>Programado:</b> ${fmtDateTime(scheduled.getTime())}</div>
  <div class="meta"><b>Captura:</b> ${fmtDateTime(captureStart.getTime())} → ${fmtDateTime(captureEnd.getTime())}</div>
  <br>
  <div>📊 <b>Resumen:</b> ${detectedCount} dispositivos detectados, <span class="badge">${connectedCount} conectados</span> · <span style="color:#cf222e">😴 ${offCount} fuera de rango</span></div>
  <div class="meta">⚡ Estado: <b>PERSISTIDO</b> · API: ${apiMs} ms · Reporte: ${reportId}</div>
</div>`;

  const headerText = `DETECTIC — Informe de Observación Autónoma
Sensor: ${sensorId}
Programado: ${fmtDateTime(scheduled.getTime())}
Captura: ${fmtDateTime(captureStart.getTime())} → ${fmtDateTime(captureEnd.getTime())}
Resumen: ${detectedCount} detectados, ${connectedCount} conectados, ${offCount} fuera de rango
ID: ${reportId}`;

  if (opts.changes_only) {
    addSection('🔔 Novedades del período', newRows + newNetworkRows,
      (newRows ? 'Nuevos dispositivos:\n' + newDetections.map((d: any) => `  · ${nameOf(d)} (${d.band || '—'})`).join('\n') : '') +
      (newNetworkRows ? '\nNuevas redes:\n' + newNetworks.map((n: any) => `  · ${n.ssid || n.ap_id || '—'} (${n.band || '—'})`).join('\n') : '')
    );
  }

  if (opts.top_devices > 0 && topRows) {
    addSection(`🏆 Top ${opts.top_devices} dispositivos`, topRows,
      topDevices.map((d: any, i: number) => `${i + 1}. ${nameOf(d)} — ${d.event_count || 0} observaciones`).join('\n'));
  }

  addSection('📱 Dispositivos Conectados', connectedRows,
    connected.map((d: any) => `  · ${nameOf(d)} — ${d.band || '—'} — ${fmtDateTime(d.last_seen)}`).join('\n') || 'Ningún dispositivo conectado.');

  addSection('😴 Dispositivos Fuera de Rango', outRows,
    outOfRange.map((d: any) => `  · ${nameOf(d)} — ${d.band || '—'} — última ${fmtDateTime(d.last_seen)}`).join('\n') || 'Ningún dispositivo fuera de rango.');

  if (opts.new_detections && newRows && !opts.changes_only) {
    addSection('🆕 Nuevas Detecciones', newRows,
      newDetections.map((d: any) => `  · ${nameOf(d)} — ${d.band || '—'}`).join('\n'));
  }

  addSection('🌐 Redes Wi-Fi Detectadas', networkRows,
    networks.map((n: any) => `  · ${n.ssid || n.ap_id || '—'} — ${n.band || '—'} — ${n.status || 'ONLINE'}`).join('\n') || 'No se detectaron redes.');

  if (opts.new_detections && newNetworkRows && !opts.changes_only) {
    addSection('🆕 Nuevas Redes', newNetworkRows,
      newNetworks.map((n: any) => `  · ${n.ssid || n.ap_id || '—'} — ${n.band || '—'}`).join('\n'));
  }

  addSection('🔴 Caídas / Desconexiones de red', offlineRows,
    'Ver sección en HTML.');

  const footerHtml = `<div class="card">
  <h3>🖥️ Redes Observadas</h3>
  <div class="meta">📡 Bandas detectadas: ${Array.from(bands).join(', ') || '—'}</div>
  <div class="meta">📟 Protocolos: ${Array.from(standards).join(', ') || '—'}</div>
  <div class="meta">🔌 Sensor: ${escHtml(sensorId)}</div>
  <div class="meta">🔒 Privacidad: identificadores pseudónimos HMAC-SHA256. Sin direcciones MAC reales. Router sin modificaciones.</div>
  <div class="meta">ID: ${reportId}</div>
</div>`;

  const footerText = `\nBandas: ${Array.from(bands).join(', ') || '—'}\nProtocolos: ${Array.from(standards).join(', ') || '—'}\nSensor: ${sensorId}\nPrivacidad: pseudónimos HMAC-SHA256, sin MACs reales.\nID: ${reportId}`;

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
${headerHtml}
${sections.join('\n')}
${footerHtml}
</body>
</html>`;

  const text = `${headerText}${textLines.join('\n')}${footerText}`;

  return { html, text, reportId, sensorId, generatedAt: now.getTime() };
}

async function handleEmailReport(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  const hours = Math.min(parseInt(url.searchParams.get('hours') || '24'), 720);
  const origin = request.headers.get('Origin') || undefined;
  const opts: ReportOptions = {
    hours,
    changes_only: url.searchParams.get('changes_only') === '1' || url.searchParams.get('changes_only') === 'true',
    top_devices: Math.min(parseInt(url.searchParams.get('top_devices') || '5'), 20),
    new_detections: url.searchParams.get('new_detections') !== '0' && url.searchParams.get('new_detections') !== 'false',
    nearby_aps: url.searchParams.get('nearby_aps') !== '0' && url.searchParams.get('nearby_aps') !== 'false',
  };

  const report = await buildEmailReport(env, opts);
  return new Response(report.html, {
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
    const origin = requestCorsOrigin(env, request);

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
        } else if (path === "/api/v1/admin/backfill-ap-state") {
          response = await handleBackfillApState(request, env);
        } else if (path === "/api/v1/admin/hmac-debug") {
          response = await handleHmacDebug(request, env);
        } else if (path === "/api/v1/subscribe" || path === "/api/v1/unsubscribe") {
          return hubStub(env).fetch(request);
        } else if (path.match(/^\/api\/v1\/sensors\/[^/]+\/location$/)) {
          response = await handleUpdateSensorLocation(request, env);
        } else if (path.match(/^\/api\/v1\/devices\/[^/]+\/identity$/)) {
          response = await handleUpdateDeviceIdentity(request, env);
        } else if (path.match(/^\/api\/v1\/devices\/[^/]+\/trust$/)) {
          response = await handleUpdateDeviceTrust(request, env);
        } else if (path === "/api/v1/reports/config") {
          response = await handleUpdateReportConfig(request, env);
        } else if (path === "/api/v1/reports/queue" || /^\/api\/v1\/reports\/queue\/[^/]+$/.test(path)) {
          response = await handleReportQueue(request, env);
        }
      }

      // GET endpoints
      else if (request.method === "GET") {
        // Realtime WebSocket / Durable Object paths
        if (path === "/ws" || path === "/api/v1/vapid/public-key") {
          return hubStub(env).fetch(request);
        }
        if (path === "/manifest.json") {
          response = new Response(MANIFEST_JSON, {
            headers: { "Content-Type": "application/manifest+json" },
          });
        } else if (path === "/sw.js") {
          response = new Response(SW_JS, {
            headers: { "Content-Type": "application/javascript; charset=utf-8", "Cache-Control": "no-cache" },
          });
        } else if (path === "/icon.svg" || path === "/favicon.ico") {
          response = new Response(ICON_SVG, {
            headers: { "Content-Type": "image/svg+xml", "Cache-Control": "public, max-age=86400" },
          });
        }
        // Dashboard / Map UI — served from the shadcn/React build in src/frontend-dist
        // Note: the assets binding serves index.html at "/"; requesting "/index.html"
        // directly triggers a redirect to "/", which would cause an infinite loop.
        // For non-root SPA paths we fetch the root asset so the browser stays on the
        // requested route and React Router can render the correct view.
        else if (path === "/" || path === "/dashboard" || path === "/map" || path === "/index.html") {
          const targetUrl = path === "/" ? new URL(request.url) : new URL("/", request.url);
          response = noCacheHtml(await env.ASSETS.fetch(new Request(targetUrl, request)));
        } else if (path === "/api/v1/healthz") response = await handleHealthz(request, env);
        else if (path === "/api/v1/readyz") response = await handleReadyz(request, env);
        else if (path === "/api/v1/devices") response = await handleDevices(request, env);
        else if (path === "/api/v1/presence") response = await handlePresence(request, env);
        else if (path === "/api/v1/sensors") response = await handleSensors(request, env);
        else if (path === "/api/v1/stats") response = await handleStats(request, env);
        else if (path === "/api/v1/timeline") response = await handleTimeline(request, env);
        else if (path === "/api/v1/analytics") response = await handleAnalytics(request, env);
        else if (path === "/api/v1/networks") response = await handleNetworks(request, env);
        else if (path === "/api/v1/fusion") response = await handleFusion(request, env);
        else if (path === "/api/v1/state") response = await handleDeviceState(request, env);
        else if (path === "/api/v1/devices/aliases") response = await handleDeviceAliases(request, env);
        else if (path === "/api/v1/sessions") response = await handleSessions(request, env);
        else if (path === "/api/v1/reports/devices") response = await handleReportsDevices(request, env);
        else if (path === "/api/v1/reports/networks") response = await handleReportsNetworks(request, env);
        else if (path === "/api/v1/reports/email") response = await handleEmailReport(request, env);
        else if (path === "/api/v1/reports/config") response = await handleGetReportConfig(request, env);
        else if (path === "/api/v1/reports/queue") response = await handleReportQueue(request, env);
        else if (path === "/api/v1/devices/unknown") response = await handleUnknownDevices(request, env);
        else if (path === "/api/v1/events") response = await handleEvents(request, env);
        else if (/^\/api\/v1\/devices\/[^/]+\/events$/.test(path)) response = await handleDeviceEvents(request, env);
        else if (/^\/api\/v1\/devices\/[^/]+\/sessions$/.test(path)) response = await handleDeviceSessions(request, env);
        else if (/^\/api\/v1\/devices\/[^/]+\/signals$/.test(path)) response = await handleDeviceSignals(request, env);
        else if (/^\/api\/v1\/devices\/[^/]+\/patterns$/.test(path)) response = await handleDevicePatterns(request, env);
        else if (/^\/api\/v1\/devices\/[^/]+\/ips$/.test(path)) response = await handleDeviceIps(request, env);
        else if (/^\/api\/v1\/devices\/[^/]+\/identity$/.test(path)) response = await handleGetDeviceIdentity(request, env);
      }

      if (!response) {
        if (request.method === "GET" && env.ASSETS) {
          const assetRes = await env.ASSETS.fetch(request);
          response = isHtmlResponse(assetRes)
            ? noCacheHtml(assetRes)
            : assetRes;
        } else {
          response = jsonResponse(404, { error: "not found" });
        }
      }
    } catch (e: any) {
      // Full diagnostic detail stays server-side (request/otel logs). The
      // client never receives stack frames, file paths, secrets, or internal
      // implementation details — only a correlated, opaque error.
      const requestId = crypto.randomUUID();
      console.error("REQUEST_ERROR", requestId, path, e?.message, e?.stack);
      response = jsonResponse(500, buildOpaqueError(requestId), origin);
    }

    return response;
  },

  async scheduled(event: ScheduledEvent, env: Env, ctx: ExecutionContext): Promise<void> {
    await ensureSchema(env.DB);

    const cfg = await env.DB.prepare('SELECT * FROM report_config WHERE id = 1').first() as ReportConfig | null;
    if (!cfg || !cfg.enabled) {
      console.log('[scheduled] reports disabled or no config');
      return;
    }

    const now = Date.now();
    const lastRow = await env.DB.prepare(
      'SELECT scheduled_at FROM email_queue WHERE status = ? ORDER BY scheduled_at DESC LIMIT 1'
    ).bind('pending').first() as { scheduled_at?: number } | null;

    if (lastRow?.scheduled_at) {
      const elapsedHours = (now / 1000 - lastRow.scheduled_at) / 3600;
      if (elapsedHours < cfg.frequency_hours) {
        console.log('[scheduled] next report not due yet', elapsedHours, 'h');
        return;
      }
    }

    const opts: ReportOptions = {
      hours: Math.min(cfg.frequency_hours, 720),
      changes_only: !!cfg.changes_only,
      top_devices: Math.min(cfg.top_devices, 20),
      new_detections: !!cfg.new_detections,
      nearby_aps: !!cfg.nearby_aps,
    };

    try {
      const report = await buildEmailReport(env, opts);
      const scheduledAt = Math.floor(now / 1000);
      const reportId = report.reportId;
      const configJson = JSON.stringify({
        frequency_hours: cfg.frequency_hours,
        changes_only: opts.changes_only,
        top_devices: opts.top_devices,
        new_detections: opts.new_detections,
        nearby_aps: opts.nearby_aps,
        email_to: cfg.email_to,
        email_subject: cfg.email_subject,
      });

      await env.DB.prepare(
        `INSERT INTO email_queue (report_id, scheduled_at, generated_at, status, html, text, config_json)
         VALUES (?, ?, ?, ?, ?, ?, ?)`
      ).bind(reportId, scheduledAt, Math.floor(report.generatedAt / 1000), 'pending', report.html, report.text, configJson).run();

      console.log('[scheduled] report enqueued', reportId);
    } catch (e: any) {
      console.error('[scheduled] report generation failed', e?.message, e?.stack);
    }
  },
};

async function handleHmacDebug(request: Request, env: Env): Promise<Response> {
  if (!verifyMasterAuth(request, env)) {
    return jsonResponse(401, { error: "unauthorized" });
  }
  let body: any;
  try {
    body = await request.json();
  } catch {
    return jsonResponse(400, { error: "invalid json" });
  }
  const sensorId = String(body?.sensor_id || "");
  if (!sensorId) return jsonResponse(400, { error: "missing sensor_id" });
  const bodyText = typeof body?.body === "string" ? body.body : "";
  if (!bodyText) return jsonResponse(400, { error: "missing body" });
  const gotId = typeof body?.got_id === "string" ? body.got_id : null;
  const gotSignature = typeof body?.got_signature === "string" ? body.got_signature : null;
  const timestamp = typeof body?.timestamp === "string" ? body.timestamp : null;

  const sensors = JSON.parse(env.DETECTIC_SENSORS || "{}");
  const secret = typeof body?.secret === "string" && body.secret.length > 0
    ? body.secret
    : sensors[sensorId];
  if (!secret) return jsonResponse(404, { error: "sensor not registered" });

  let payload: any;
  try {
    payload = JSON.parse(bodyText);
  } catch (e) {
    return jsonResponse(400, { error: "body is not valid json" });
  }

  const devicePseudos = (payload?.devices || [])
    .map((d: any) => typeof d?.pseudonym === "string" ? d.pseudonym : "")
    .filter((p: string) => p.length > 0)
    .sort();
  const eventPseudos = (payload?.events || [])
    .map((e: any) => typeof e?.pseudonym === "string" ? e.pseudonym : "")
    .filter((p: string) => p.length > 0)
    .sort();
  const allPseudos = Array.from(new Set([...devicePseudos, ...eventPseudos])).sort();
  const rawDevicePseudos = (payload?.devices || [])
    .map((d: any) => typeof d?.pseudonym === "string" ? d.pseudonym : "")
    .filter((p: string) => p.length > 0);
  const rawEventPseudos = (payload?.events || [])
    .map((e: any) => typeof e?.pseudonym === "string" ? e.pseudonym : "")
    .filter((p: string) => p.length > 0);
  const payloadSensorId = typeof payload?.sensor_id === "string" ? payload.sensor_id : sensorId;
  const payloadCapturedAt = typeof payload?.captured_at === "number" ? payload.captured_at : null;

  const pseudoLists = [
    { name: "devices_sorted", list: devicePseudos },
    { name: "events_sorted", list: eventPseudos },
    { name: "all_sorted", list: allPseudos },
    { name: "devices_raw", list: rawDevicePseudos },
    { name: "events_raw", list: rawEventPseudos },
    { name: "all_raw_unique", list: Array.from(new Set([...rawDevicePseudos, ...rawEventPseudos])) },
    { name: "all_raw_concat", list: [...rawDevicePseudos, ...rawEventPseudos] },
  ];
  const baseCandidates = [payloadSensorId, sensorId, sensorId.replace(/-/g, ""), sensorId.replace(/-/g, "_")];

  const now = Math.floor(Date.now() / 1000);
  const capturedAtCandidates: number[] = [];
  if (payloadCapturedAt !== null) capturedAtCandidates.push(payloadCapturedAt);
  capturedAtCandidates.push(now);
  if (timestamp) {
    const tsNum = parseInt(timestamp, 10);
    if (!isNaN(tsNum)) {
      capturedAtCandidates.push(tsNum);
      for (let d = -2; d <= 2; d++) capturedAtCandidates.push(tsNum + d);
      capturedAtCandidates.push(tsNum * 1000);
      capturedAtCandidates.push(tsNum * 1000000);
    }
  }
  for (const base of [payloadCapturedAt ?? now, now]) {
    capturedAtCandidates.push(base * 1000);
    capturedAtCandidates.push(base * 1000000);
  }

  const idResults: any[] = [];
  for (const { name, list } of pseudoLists) {
    const joined = list.join(",");
    for (const baseStr of baseCandidates) {
      for (const capturedAt of capturedAtCandidates) {
        for (const capturedStr of [String(capturedAt), String(capturedAt) + ".0"]) {
          const signed = new TextEncoder().encode([baseStr, capturedStr, joined].join("|"));
          const expected = await hmacSha256(secret, signed);
          idResults.push({
            pseudo_list: name,
            base: baseStr,
            captured_at: capturedAt,
            captured_str: capturedStr,
            expected_id: expected,
            matches_got: gotId ? constantTimeEqual(expected, gotId) : null,
          });
        }
      }
    }
  }

  const signatureResults: any[] = [];
  const bodyBytes = new TextEncoder().encode(bodyText);
  const sigBody = await hmacSha256(secret, bodyBytes);
  signatureResults.push({ formula: "hmac(body)", expected_signature: sigBody, matches_got: gotSignature ? constantTimeEqual(sigBody, gotSignature) : null });
  if (timestamp) {
    const canonical = new TextEncoder().encode(timestamp + "\n" + bodyText);
    const sigCanonical = await hmacSha256(secret, canonical);
    signatureResults.push({ formula: "hmac(timestamp + \\n + body)", expected_signature: sigCanonical, matches_got: gotSignature ? constantTimeEqual(sigCanonical, gotSignature) : null });
  }

  const matching = [
    ...idResults.filter((r) => r.matches_got),
    ...signatureResults.filter((r) => r.matches_got),
  ];

  return jsonResponse(200, {
    sensor_id: sensorId,
    got_id: gotId,
    got_signature: gotSignature,
    body_hash: Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bodyBytes))).map((b) => b.toString(16).padStart(2, "0")).join(""),
    matches: matching,
    id_variants: idResults,
    signature_variants: signatureResults,
  });
}

export { RealtimeHub };
