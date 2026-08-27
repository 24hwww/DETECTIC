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

use crate::snapshot::SensorSnapshot;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

/// Shared state between the sensor service and the HTTP control server.
#[derive(Clone, Default, Debug)]
pub struct SensorState {
    pub sensor_id: String,
    pub version: String,
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

pub struct HttpServer {
    state: Arc<Mutex<SensorState>>,
    port: u16,
}

impl HttpServer {
    pub fn spawn(state: Arc<Mutex<SensorState>>, port: u16) -> Result<(), String> {
        // Bind to `[::]:port` for dual-stack (IPv4 + IPv6) support.
        // On Linux this accepts both IPv4 and IPv6 connections by default
        // (IPV6_V6ONLY=0).  This is essential on the EX520V where the host
        // can only reach the router via IPv6 link-local.
        let addr = format!("[::]:{port}");
        let listener = TcpListener::bind(&addr)
            .map_err(|e| format!("http_server bind error on {addr}: {e}"))?;

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
                Ok(stream) => self.handle_connection(stream),
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

        // Drain headers.
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).is_err() {
                break;
            }
            if line == "\r\n" || line.is_empty() {
                break;
            }
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
        let body = format!(
            "<!DOCTYPE html><html><head><title>DETECTIC</title></head><body>\
|<h1>DETECTIC</h1>\
|<p>sensor_id: {}</p>\
|<p>version: {}</p>\
|<p>uptime: {}s</p>\
|<p>healthy: {}</p>\
|<p>ready: {}</p>\
|<p>devices: {}</p>\
|<p>gtpr: {}</p>\
|<p>backend: {}</p>\
|<p>mdns: {}</p>\
|<p>last_poll: {}</p>\
|<p>last_upload: {}</p>\
|</body></html>\n",
            state.sensor_id,
            state.version,
            state.uptime_secs(),
            state.healthy,
            state.ready,
            state.device_count,
            state.gtpr_status,
            state.backend_status,
            state.mdns_status,
            fmt_since(state.last_poll),
            fmt_since(state.last_upload),
        );
        (200, "text/html", body)
    }

    fn health_json(&self) -> (u16, &'static str, String) {
        let state = self.state.lock().unwrap();
        let mut map: HashMap<&str, String> = HashMap::new();
        map.insert("status", if state.healthy { "healthy".into() } else { "unhealthy".into() });
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
        let devices = guard
            .as_ref()
            .map(|s| s.stations.clone())
            .unwrap_or_default();
        (200, "application/json", serde_json::to_string(&devices).unwrap_or_default())
    }

    fn device_detail_json(&self, id: &str) -> (u16, &'static str, String) {
        let state = self.state.lock().unwrap();
        let guard = state.snapshot.lock().unwrap();
        if let Some(snap) = guard.as_ref() {
            if let Some(dev) = snap.stations.iter().find(|d| d.identity() == id) {
                return (
                    200,
                    "application/json",
                    serde_json::to_string(dev).unwrap_or_default(),
                );
            }
        }
        (404, "application/json", r#"{"error":"device not found"}"#.into())
    }

    fn events_json(&self) -> (u16, &'static str, String) {
        let state = self.state.lock().unwrap();
        let guard = state.recent_events.lock().unwrap();
        let events: Vec<String> = guard.iter().cloned().collect();
        (200, "application/json", serde_json::to_string(&events).unwrap_or_default())
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
        (200, "application/json", json_obj(map))
    }
}

fn fmt_since(since: Option<Instant>) -> String {
    since
        .map(|s| format!("{}s", Instant::now().duration_since(s).as_secs()))
        .unwrap_or_else(|| "never".into())
}

fn respond(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) {
    let reason = match status {
        200 => "OK",
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
