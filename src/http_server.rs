//! Minimal HTTP health/control server for the Detectic sensor.
//!
//! Exposes the canonical TCP/8787 endpoints:
//!   GET /         -> small status page
//!   GET /health   -> health snapshot (JSON)
//!   GET /ready    -> readiness probe
//!   GET /version  -> version string
//!   GET /devices  -> current device snapshot
//!   GET /events   -> recent events (limited)
//!   GET /metrics  -> basic metrics
//!
//! Implemented with `std::net` only, to keep the on-router musl build free of
//! extra C dependencies.

use crate::cwmp_handler;
use crate::snapshot::SensorSnapshot;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Payload accepted by `POST /probes` from an external RF (monitor-mode) sensor.
/// `device_id` is already an HMAC pseudonym (raw MAC never leaves the external
/// sensor). Accepts a single object or an array.
#[derive(Debug, Deserialize)]
struct IngestProbe {
    device_id: String,
    #[serde(default)]
    rssi: Option<i64>,
    #[serde(default)]
    rssi_dbm: Option<f64>,
    #[serde(default)]
    band: Option<String>,
    #[serde(default)]
    channel: Option<u8>,
    #[serde(default)]
    ssid: Option<String>,
    #[serde(default)]
    per_chain_rssi: Vec<i64>,
    #[serde(default)]
    randomized: Option<bool>,
}

/// Shared state between the sensor service and the HTTP control server.
#[derive(Clone, Default, Debug)]
pub struct SensorState {
    pub sensor_id: String,
    pub version: String,
    /// Per-sensor HMAC secret. Used to pseudonymize MAC addresses before they
    /// are exposed over the LAN HTTP control plane — raw MACs must never leave
    /// the sensor through `/devices`.
    pub secret: String,
    pub started_at: Option<Instant>,
    pub last_poll: Option<Instant>,
    pub last_upload: Option<Instant>,
    pub last_gtpr_success: Option<Instant>,
    pub last_gtpr_failure: Option<Instant>,
    pub last_backend_success: Option<Instant>,
    pub last_backend_failure: Option<Instant>,
    pub gtpr_status: String,
    pub backend_status: String,
    pub mdns_status: String,
    pub healthy: bool,
    pub ready: bool,
    pub snapshot: Arc<Mutex<Option<SensorSnapshot>>>,
    pub recent_events: Arc<Mutex<Vec<String>>>,
    pub device_count: usize,
    /// Devices detected by an external RF probe sensor (monitor-mode). Keyed by
    /// the probe's HMAC pseudonym; updated immediately by `POST /probes` so the
    /// dashboard can show movement of non-associated devices without waiting for
    /// the GTPR poll. `source: "probe"`.
    pub probe_devices: HashMap<String, serde_json::Value>,
    /// Probe observations queued by `POST /probes` for the next poll cycle, which
    /// feeds them into `process_probes` so the temporal/proximity engine and the
    /// backend event stream also reflect RF-detected devices.
    pub pending_probes: Vec<crate::temporal::ProbeObservation>,
}

impl SensorState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started_at
            .map(|s| Instant::now().duration_since(s).as_secs())
            .unwrap_or(0)
    }

    pub fn set_healthy(&mut self, healthy: bool) {
        self.healthy = healthy;
    }

    pub fn set_ready(&mut self, ready: bool) {
        self.ready = ready;
    }
}

#[derive(Clone)]
pub struct HttpServer {
    state: Arc<Mutex<SensorState>>,
    port: u16,
}

impl HttpServer {
    pub fn spawn(state: Arc<Mutex<SensorState>>, port: u16) -> Result<(), String> {
        // Bind to `[::]:port` for dual-stack (IPv4 + IPv6) support.
        // We explicitly clear IPV6_V6ONLY (set it to 0) via setsockopt instead
        // of relying on the system default (`net.ipv6.bindv6only`), because the
        // EX520V kernel may ship with bindv6only=1, which would make the
        // `::` socket IPv6-only and reject IPv4 connections to 192.168.0.1:8787
        // with "connection refused".  This is essential on the EX520V where the
        // host may reach the router via either IPv4 LAN or IPv6 link-local.
        let listener = bind_dualstack(port)
            .map_err(|e| format!("http_server bind error on [::]:{port}: {e}"))?;

        let server = HttpServer { state, port };

        thread::Builder::new()
            .name("http-server".into())
            .spawn(move || server.run(listener))
            .map_err(|e| format!("http_server thread spawn error: {e}"))?;

        Ok(())
    }

    fn run(self, listener: TcpListener) {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    // Handle each connection on its own thread so a long-lived
                    // SSE (`/stream`) client never blocks the other routes
                    // (e.g. the dashboard polls /health, /devices).
                    let server = self.clone();
                    let _ = thread::Builder::new()
                        .name("http-conn".into())
                        .spawn(move || server.handle_connection(stream));
                }
                Err(_) => continue,
            }
        }
    }

    fn handle_connection(&self, mut stream: TcpStream) {
        let mut reader = BufReader::new(&mut stream);
        let mut first_line = String::new();
        if reader.read_line(&mut first_line).is_err() {
            return;
        }

        let parts: Vec<&str> = first_line.split_whitespace().collect();
        if parts.len() < 2 {
            return;
        }

        let method = parts[0];
        let path = parts[1];

        // Parse headers (including Content-Length for POST bodies).
        let mut content_length: usize = 0;
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).is_err() {
                break;
            }
            if line == "\r\n" || line.is_empty() {
                break;
            }
            if let Some(val) = line.strip_prefix("Content-Length:") {
                content_length = val.trim().parse().unwrap_or(0);
            }
        }

        // Handle POST /cwmp (CWMP SOAP endpoint).
        if method == "POST" && path.starts_with("/cwmp") {
            let mut body = vec![0u8; content_length];
            if content_length > 0 {
                if reader.read_exact(&mut body).is_err() {
                    respond(&mut stream, 400, "text/plain", b"Bad Request\n");
                    return;
                }
            }
            let body_text = String::from_utf8_lossy(&body).to_string();
            let resp = cwmp_handler::handle_cwmp_request(&body_text);
            respond_raw(&mut stream, resp.status, resp.content_type, &resp.body);
            return;
        }

        // Handle POST /probes — external RF probe observations (motion detector).
        if method == "POST" && path == "/probes" {
            let mut body = vec![0u8; content_length];
            if content_length > 0 {
                if reader.read_exact(&mut body).is_err() {
                    respond(&mut stream, 400, "text/plain", b"Bad Request\n");
                    return;
                }
            }
            let (status, content_type, resp_body) = self.ingest_probes(&body);
            respond_raw(&mut stream, status, content_type, resp_body.as_bytes());
            return;
        }

        // No longer need the buffered reader; drop its borrow of `stream` so we
        // can move `stream` into the SSE handler (which keeps it open).
        drop(reader);

        if method == "GET" && path == "/stream" {
            handle_sse(self.state.clone(), stream);
            return;
        }

        if method != "GET" {
            respond(&mut stream, 405, "text/plain", b"Method Not Allowed\n");
            return;
        }

        let (status, content_type, body) = self.route(path);
        respond(&mut stream, status, content_type, body.as_bytes());
    }

    fn route(&self, path: &str) -> (u16, &'static str, String) {
        let path = path.split('?').next().unwrap_or(path);

        match path {
            "/" => self.root_page(),
            "/health" => self.health_json(),
            "/ready" => self.ready_json(),
            "/version" => self.version_text(),
            "/devices" => self.devices_json(),
            "/events" => self.events_json(),
            "/metrics" => self.metrics_json(),
            _ => {
                if let Some(id) = path.strip_prefix("/devices/") {
                    self.device_detail_json(id)
                } else {
                    (404, "text/plain", "Not Found\n".into())
                }
            }
        }
    }

    fn root_page(&self) -> (u16, &'static str, String) {
        let state = self.state.lock().unwrap();
        let devices_json = {
            let guard = state.snapshot.lock().unwrap();
            let stations = guard
                .as_ref()
                .map(|s| mask_stations(state.secret.as_bytes(), &s.stations))
                .unwrap_or_default();
            serde_json::to_string(&stations).unwrap_or_else(|_| "[]".into())
        };
        let events_json = {
            let guard = state.recent_events.lock().unwrap();
            serde_json::to_string(&guard.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_else(|_| "[]".into())
        };
        let body = build_dashboard(&state, &devices_json, &events_json);
        (200, "text/html", body)
    }

    fn health_json(&self) -> (u16, &'static str, String) {
        let state = self.state.lock().unwrap();
        let mut map: HashMap<&str, String> = HashMap::new();
        map.insert(
            "status",
            if state.healthy {
                "healthy".into()
            } else {
                "unhealthy".into()
            },
        );
        map.insert("sensor_id", state.sensor_id.clone());
        map.insert("version", state.version.clone());
        map.insert("uptime", state.uptime_secs().to_string());
        map.insert("gtpr", state.gtpr_status.clone());
        map.insert("backend", state.backend_status.clone());
        map.insert("mdns", state.mdns_status.clone());
        map.insert("devices", state.device_count.to_string());
        map.insert("ready", state.ready.to_string());
        map.insert("port", self.port.to_string());
        (200, "application/json", json_obj(map))
    }

    fn ready_json(&self) -> (u16, &'static str, String) {
        let state = self.state.lock().unwrap();
        let mut map: HashMap<&str, String> = HashMap::new();
        map.insert("ready", state.ready.to_string());
        map.insert("gtpr", state.gtpr_status.clone());
        (200, "application/json", json_obj(map))
    }

    fn version_text(&self) -> (u16, &'static str, String) {
        let state = self.state.lock().unwrap();
        (200, "text/plain", format!("{}\n", state.version))
    }

    fn devices_json(&self) -> (u16, &'static str, String) {
        let state = self.state.lock().unwrap();
        let guard = state.snapshot.lock().unwrap();
        let body = if let Some(snap) = guard.as_ref() {
            let secret = state.secret.as_bytes();
            let mut out: Vec<serde_json::Value> = Vec::with_capacity(snap.stations.len());
            for d in &snap.stations {
                let masked = mask_station(secret, d);
                let mut value = serde_json::to_value(&masked).unwrap_or_default();
                if let Some(p) = snap.station_proximity.get(&d.identity()) {
                    if let Some(obj) = value.as_object_mut() {
                        obj.insert("proximity_zone".into(), p.zone.as_str().into());
                        obj.insert("proximity_trend".into(), p.trend.as_str().into());
                        obj.insert("proximity_zone_label".into(), p.zone_label().into());
                        obj.insert("proximity_trend_label".into(), p.trend_label().into());
                        obj.insert("proximity_label".into(), p.label().into());
                        obj.insert("heat".into(), p.heat.into());
                        obj.insert("heat_color".into(), p.color_class().into());
                        obj.insert("trend_arrow".into(), p.trend.arrow().into());
                        obj.insert("rssi_dbm".into(), serde_json::json!(p.rssi_dbm));
                        obj.insert("distance_m".into(), serde_json::json!(p.distance_m));
                        obj.insert(
                            "proximity_confidence".into(),
                            serde_json::json!(p.confidence),
                        );
                        obj.insert("proximity_samples".into(), serde_json::json!(p.samples));
                        obj.insert("proximity_reliable".into(), serde_json::json!(proximity_reliable(d)));
                    }
                }
                out.push(value);
            }
            // Append RF probe-detected devices (from POST /probes) so the
            // dashboard shows motion-detected (possibly non-associated) clients.
            for probe in state.probe_devices.values() {
                out.push(probe.clone());
            }
            let out = dedup_devices(out);
            serde_json::to_string(&out).unwrap_or_default()
        } else {
            "[]".into()
        };
        (200, "application/json", body)
    }

    fn device_detail_json(&self, id: &str) -> (u16, &'static str, String) {
        let state = self.state.lock().unwrap();
        let guard = state.snapshot.lock().unwrap();
        if let Some(snap) = guard.as_ref() {
            for d in &snap.stations {
                // Match by either the raw identity (e.g. hostname/IP) or the
                // pseudonymized MAC, so the masked view stays navigable.
                let pseudo = match &d.mac {
                    Some(mac) => Some(crate::crypto::pseudonymize(state.secret.as_bytes(), mac)),
                    None => None,
                };
                if d.identity() == id || pseudo.as_deref() == Some(id) {
                    let masked = mask_station(state.secret.as_bytes(), d);
                    let mut value = serde_json::to_value(&masked).unwrap_or_default();
                    if let Some(p) = snap.station_proximity.get(&d.identity()) {
                        if let Some(obj) = value.as_object_mut() {
                            obj.insert("proximity_zone".into(), p.zone.as_str().into());
                            obj.insert("proximity_trend".into(), p.trend.as_str().into());
                            obj.insert("proximity_zone_label".into(), p.zone_label().into());
                            obj.insert("proximity_trend_label".into(), p.trend_label().into());
                            obj.insert("proximity_label".into(), p.label().into());
                            obj.insert("heat".into(), p.heat.into());
                            obj.insert("heat_color".into(), p.color_class().into());
                            obj.insert("trend_arrow".into(), p.trend.arrow().into());
                            obj.insert("rssi_dbm".into(), serde_json::json!(p.rssi_dbm));
                            obj.insert("distance_m".into(), serde_json::json!(p.distance_m));
                            obj.insert(
                                "proximity_confidence".into(),
                                serde_json::json!(p.confidence),
                            );
                            obj.insert("proximity_samples".into(), serde_json::json!(p.samples));
                            obj.insert("proximity_reliable".into(), serde_json::json!(proximity_reliable(d)));
                        }
                    }
                    return (
                        200,
                        "application/json",
                        serde_json::to_string(&value).unwrap_or_default(),
                    );
                }
            }
        }
        (
            404,
            "application/json",
            r#"{"error":"device not found"}"#.into(),
        )
    }

    /// Ingest a probe observation from an external RF (monitor-mode) sensor.
    /// Updates the dashboard-visible probe device immediately and queues the
    /// observation for `process_probes` on the next poll so the temporal /
    /// proximity engine and the backend event stream also see it.
    fn ingest_probes(&self, body: &[u8]) -> (u16, &'static str, String) {
        let value: serde_json::Value = match serde_json::from_slice(body) {
            Ok(v) => v,
            Err(e) => return (400, "application/json", format!(r#"{{"error":"bad probes: {e}"}}"#)),
        };
        let probes: Vec<IngestProbe> = match value {
            serde_json::Value::Array(a) => {
                match serde_json::from_value(serde_json::Value::Array(a)) {
                    Ok(p) => p,
                    Err(e) => return (400, "application/json", format!(r#"{{"error":"bad probes: {e}"}}"#)),
                }
            }
            other => match serde_json::from_value::<IngestProbe>(other) {
                Ok(one) => vec![one],
                Err(e) => return (400, "application/json", format!(r#"{{"error":"bad probes: {e}"}}"#)),
            },
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let mut state = self.state.lock().unwrap();
        let sensor_id = state.sensor_id.clone();
        for p in &probes {
            let dbm = p.rssi_dbm.or_else(|| {
                p.rssi
                    .filter(|&r| (-120..=0).contains(&r))
                    .map(|r| r as f64)
            });
            let zone = dbm
                .map(|d| crate::proximity::zone_from_dbm(d, crate::calibrate::Band::Unknown))
                .unwrap_or(crate::proximity::ProximityZone::Unknown);
            let host = p
                .ssid
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "(probe)".into());
            let entry = serde_json::json!({
                "mac": p.device_id, // already a pseudonym from the external sensor
                "source": "probe",
                "active": "1",
                "hostname": host,
                "band": p.band,
                "channel": p.channel,
                "rssi": p.rssi,
                "rssi_dbm": dbm,
                "per_chain_rssi": p.per_chain_rssi,
                "ssid": p.ssid,
                "randomized": p.randomized,
                "proximity_zone": zone.as_str(),
                "proximity_zone_label": zone.es_label(),
                "proximity_trend": "unknown",
                "proximity_trend_label": "desconocido",
                "trend_arrow": "?",
                "last_seen": now,
            });
            state.probe_devices.insert(p.device_id.clone(), entry);
            state
                .pending_probes
                .push(crate::temporal::ProbeObservation {
                    device_id: p.device_id.clone(),
                    timestamp: now,
                    sensor_id: sensor_id.clone(),
                    band: p.band.clone(),
                    channel: p.channel,
                    rssi: p.rssi,
                    per_chain_rssi: p.per_chain_rssi.clone(),
                    ..Default::default()
                });
        }
        (200, "application/json", format!(r#"{{"ok":true,"count":{}}}"#, probes.len()))
    }


    fn events_json(&self) -> (u16, &'static str, String) {
        let state = self.state.lock().unwrap();
        let guard = state.recent_events.lock().unwrap();
        let events: Vec<String> = guard.iter().cloned().collect();
        (
            200,
            "application/json",
            serde_json::to_string(&events).unwrap_or_default(),
        )
    }

    fn metrics_json(&self) -> (u16, &'static str, String) {
        let state = self.state.lock().unwrap();
        let mut map: HashMap<&str, String> = HashMap::new();
        map.insert("uptime_seconds", state.uptime_secs().to_string());
        map.insert("device_count", state.device_count.to_string());
        map.insert("last_poll_ago", fmt_since(state.last_poll));
        map.insert("last_upload_ago", fmt_since(state.last_upload));
        map.insert("gtpr_status", state.gtpr_status.clone());
        map.insert("backend_status", state.backend_status.clone());
        map.insert("mdns_status", state.mdns_status.clone());
        // Phase 4: expose per-radio per-chain RSSI samples (read-only
        // `iwpriv stat`) so the time series is retrievable without a new table.
        if let Ok(snap_lock) = state.snapshot.lock() {
            if let Some(snap) = snap_lock.as_ref() {
                if !snap.radio_stats.is_empty() {
                    map.insert(
                        "radio_stats",
                        serde_json::to_string(&snap.radio_stats).unwrap_or_default(),
                    );
                }
            }
        }
        (200, "application/json", json_obj(map))
    }
}

fn fmt_since(since: Option<Instant>) -> String {
    since
        .map(|s| format!("{}s", Instant::now().duration_since(s).as_secs()))
        .unwrap_or_else(|| "never".into())
}

/// Replace a station's raw MAC with a stable HMAC pseudonym (secret-keyed) for
/// LAN-facing HTTP responses. Raw MACs must never leave the sensor.
fn mask_station(secret: &[u8], d: &crate::model::Device) -> crate::model::Device {
    let mut m = d.clone();
    if let Some(mac) = &d.mac {
        m.mac = Some(crate::crypto::pseudonymize(secret, mac));
    }
    m
}

/// Mask every station in a slice (see [`mask_station`]).
fn mask_stations(secret: &[u8], devs: &[crate::model::Device]) -> Vec<crate::model::Device> {
    devs.iter().map(|d| mask_station(secret, d)).collect()
}

/// Uplink (RX) rate in kbps below which a client is considered idle / power-save.
/// Such clients transmit almost nothing, so the AP's `signalStrength` is a stale
/// estimate and the derived proximity should not be trusted as "cerca".
const UPLINK_ACTIVE_KBPS: u64 = 4000;

/// Whether a client's RSSI-derived proximity is likely reliable: the client must
/// show meaningful uplink activity. Idle clients → `false` ("señal estimada").
fn proximity_reliable(d: &crate::model::Device) -> bool {
    d.rx_rate.map(|r| r >= UPLINK_ACTIVE_KBPS).unwrap_or(false)
}

/// Deduplicate `/devices` so the same logical device is not replicated across
/// radios/sources. Key = hostname (lowercased) when present, else MAC, else IP.
/// When two rows collide, the entry with `active="1"` wins (a stale/inactive
/// duplicate is dropped). Presence order of first occurrence is preserved.
fn dedup_devices(items: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut result: Vec<serde_json::Value> = Vec::new();
    for item in items {
        let key = device_key(&item);
        if let Some(&pos) = index.get(&key) {
            let is_active = item.get("active").and_then(|a| a.as_str()) == Some("1");
            let cur_active = result[pos].get("active").and_then(|a| a.as_str()) == Some("1");
            if is_active && !cur_active {
                result[pos] = item;
            }
        } else {
            index.insert(key, result.len());
            result.push(item);
        }
    }
    result
}

/// Stable key to detect the same logical device across rows.
fn device_key(v: &serde_json::Value) -> String {
    if let Some(h) = v
        .get("hostname")
        .and_then(|h| h.as_str())
        .map(|s| s.to_lowercase())
        .filter(|s| !s.is_empty())
    {
        return format!("h:{h}");
    }
    if let Some(m) = v.get("mac").and_then(|m| m.as_str()) {
        return format!("m:{m}");
    }
    let ip = v.get("ip").and_then(|i| i.as_str()).unwrap_or("");
    format!("i:{ip}")
}

/// Build the rich HTML dashboard.  The page bootstraps with the current state
/// then polls /health, /devices and /events every 2s for a live, zero-install
/// developer view of the sensor.
fn build_dashboard(state: &SensorState, devices_json: &str, events_json: &str) -> String {
    let mut page = include_str!("http_dashboard.html").to_string();
    page = page.replacen("__SENSOR_ID__", &state.sensor_id, 1);
    page = page.replacen("__VERSION__", &state.version, 1);
    page = page.replacen("__UPTIME__", &state.uptime_secs().to_string(), 1);
    page = page.replacen("__DEVICE_COUNT__", &state.device_count.to_string(), 1);
    page = page.replacen("__GTPR_STATUS__", &state.gtpr_status, 1);
    page = page.replacen("__LAST_POLL__", &fmt_since(state.last_poll), 1);
    page = page.replacen("__LAST_UPLOAD__", &fmt_since(state.last_upload), 1);
    // Los dos tokens JSON aparecen DOS veces en el template (dentro de la guarda
    // `typeof __X__ !== 'undefined' ? __X__ : []`), así que hay que reemplazar
    // TODAS las ocurrencias; `replacen(...,1)` dejaría la 2ª ref como literal.
    page = page.replace("__DEVICES_JSON__", devices_json);
    page = page.replace("__EVENTS_JSON__", events_json);
    page
}

fn respond(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Unknown",
    };
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n",
        status = status,
        reason = reason,
        content_type = content_type,
        len = body.len()
    );
    let _ = stream.write_all(headers.as_bytes());
    let _ = stream.write_all(body);
}

fn respond_raw(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) {
    respond(stream, status, content_type, body);
}

/// Server-Sent Events (SSE) channel (`GET /stream`).
///
/// Emits an `event: change` whenever the sensor's `last_poll` advances, i.e. the
/// router actually produced a new snapshot. This lets the dashboard refresh only
/// when there is something new (instead of polling on a fixed 2s timer). An
/// initial `change` is sent immediately on connect so the client can catch up.
/// The stream stays open; it ends when the client disconnects (write errors).
fn handle_sse(state: Arc<Mutex<SensorState>>, mut stream: TcpStream) {
    let _ = stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nX-Accel-Buffering: no\r\n\r\n",
    );
    let _ = stream.flush();
    // Detect a dropped client quickly instead of blocking forever.
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

    let _ = stream.write_all(b"event: change\ndata: open\n\n");
    let _ = stream.flush();

    let mut last_poll: Option<Instant> = None;
    loop {
        thread::sleep(Duration::from_millis(250));
        let current = state.lock().unwrap().last_poll;
        if current != last_poll {
            last_poll = current;
            if stream
                .write_all(b"event: change\ndata: poll\n\n")
                .is_err()
            {
                break;
            }
            let _ = stream.flush();
        }
    }
}

/// Build a simple JSON object from string key/value pairs.  Values are escaped.
fn json_obj(map: HashMap<&str, String>) -> String {
    let mut parts = Vec::new();
    for (k, v) in map {
        parts.push(format!("\"{}\":\"{}\"", escape_json(k), escape_json(&v)));
    }
    format!("{{ {}}}", parts.join(", "))
}

fn escape_json(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '"' => "\\\"".to_string(),
            '\\' => "\\\\".to_string(),
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            c if (c as u32) < 0x20 => format!("\\u{:04x}", c as u32),
            c => c.to_string(),
        })
        .collect()
}

/// Bind a dual-stack IPv6 socket on `[::]:port` with IPV6_V6ONLY explicitly
/// cleared, so it accepts both IPv4 and IPv6 connections regardless of the
/// system `net.ipv6.bindv6only` sysctl.  Falls back to `TcpListener::bind`
/// (`[::]:port`) if the low-level path is unavailable on a non-Linux target.
#[cfg(target_os = "linux")]
fn bind_dualstack(port: u16) -> std::io::Result<TcpListener> {
    use std::os::unix::io::FromRawFd;

    // IPV6_V6ONLY is 26 on Linux.
    const IPV6_V6ONLY: libc::c_int = 26;

    unsafe {
        let fd = libc::socket(libc::AF_INET6, libc::SOCK_STREAM, 0);
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }

        // Allow rapid rebinding after a restart (SO_REUSEADDR).
        let one: libc::c_int = 1;
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            &one as *const libc::c_int as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        );

        // Clear IPV6_V6ONLY -> accept IPv4-mapped connections too.
        let v6only: libc::c_int = 0;
        if libc::setsockopt(
            fd,
            libc::IPPROTO_IPV6,
            IPV6_V6ONLY,
            &v6only as *const libc::c_int as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        ) < 0
        {
            let e = std::io::Error::last_os_error();
            libc::close(fd);
            return Err(e);
        }

        // Bind to [::]:port (the IPv6 wildcard).
        let addr = libc::sockaddr_in6 {
            sin6_family: libc::AF_INET6 as libc::sa_family_t,
            sin6_port: port.to_be(),
            sin6_flowinfo: 0,
            sin6_addr: libc::in6_addr { s6_addr: [0; 16] },
            sin6_scope_id: 0,
        };
        if libc::bind(
            fd,
            &addr as *const libc::sockaddr_in6 as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t,
        ) < 0
        {
            let e = std::io::Error::last_os_error();
            libc::close(fd);
            return Err(e);
        }

        // Listen with a reasonable backlog.
        if libc::listen(fd, 16) < 0 {
            let e = std::io::Error::last_os_error();
            libc::close(fd);
            return Err(e);
        }

        Ok(TcpListener::from_raw_fd(fd))
    }
}

#[cfg(not(target_os = "linux"))]
fn bind_dualstack(port: u16) -> std::io::Result<TcpListener> {
    let addr = format!("[::]:{port}");
    TcpListener::bind(&addr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Device;

    fn station(mac: &str, hostname: &str, ip: &str) -> Device {
        Device {
            hostname: Some(hostname.into()),
            ip: Some(ip.into()),
            mac: Some(mac.into()),
            rssi: Some(50),
            standard: Some("ax".into()),
            source: Some("wifi".into()),
            active: Some("1".into()),
            ..Default::default()
        }
    }

    #[test]
    fn masked_device_replaces_raw_mac_and_is_stable() {
        let dev = station("AA:BB:CC:11:22:33", "phone", "10.0.0.5");
        let a = mask_station(b"secret", &dev);
        let b = mask_station(b"secret", &dev);
        // Raw MAC never leaks; pseudonym is present, stable, 64-hex HMAC.
        assert_ne!(a.mac.as_deref(), Some("AA:BB:CC:11:22:33"));
        let pseudo = a.mac.as_deref().unwrap_or_default();
        assert_eq!(pseudo.len(), 64);
        assert!(pseudo.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(a.mac, b.mac, "pseudonym stable for the same device+secret");
    }

    #[test]
    fn masked_device_varies_with_secret() {
        let dev = station("AA:BB:CC:11:22:33", "phone", "10.0.0.5");
        let a = mask_station(b"secret-1", &dev);
        let b = mask_station(b"secret-2", &dev);
        assert_ne!(a.mac, b.mac);
    }

    #[test]
    fn masked_device_keeps_non_mac_fields() {
        let dev = station("AA:BB:CC:11:22:33", "phone", "10.0.0.5");
        let a = mask_station(b"secret", &dev);
        assert_eq!(a.hostname.as_deref(), Some("phone"));
        assert_eq!(a.ip.as_deref(), Some("10.0.0.5"));
        assert_eq!(a.rssi, Some(50));
        assert_eq!(a.standard.as_deref(), Some("ax"));
    }

    #[test]
    fn mask_stations_preserves_order_and_len() {
        let devs = vec![
            station("AA:BB:CC:00:00:01", "a", "10.0.0.1"),
            station("AA:BB:CC:00:00:02", "b", "10.0.0.2"),
        ];
        let masked = mask_stations(b"secret", &devs);
        assert_eq!(masked.len(), 2);
        assert_eq!(masked[0].hostname.as_deref(), Some("a"));
        assert_eq!(masked[1].hostname.as_deref(), Some("b"));
        // JSON output must not contain any raw MAC.
        let json = serde_json::to_string(&masked).unwrap();
        assert!(!json.contains("AA:BB:CC"));
        assert!(!json.contains("00:00:01"));
    }

    #[test]
    fn dedup_keeps_active_and_removes_duplicate_hostname() {
        let rows = vec![
            serde_json::json!({"hostname":"moto-g42","ip":"192.168.0.21","mac":"m1","active":"0","radio_mac":"C1"}),
            serde_json::json!({"hostname":"moto-g42","ip":"192.168.0.20","mac":"m2","active":"1","radio_mac":"C3"}),
            serde_json::json!({"hostname":"realme-9i","ip":"192.168.0.22","mac":"m3","active":"1","radio_mac":"C1"}),
            serde_json::json!({"hostname":"","ip":"","mac":"m4","active":"1"}),
        ];
        let out = dedup_devices(rows);
        assert_eq!(out.len(), 3, "moto-g42 must collapse to one row");
        let moto = out
            .iter()
            .find(|x| x["hostname"] == "moto-g42")
            .expect("moto row");
        assert_eq!(moto["active"], "1");
        assert_eq!(moto["ip"], "192.168.0.20");
        // distinct MAC without hostname stays
        assert!(out.iter().any(|x| x["mac"] == "m4"));
    }
}
