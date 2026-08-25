//! Backend transport abstraction (M5-E).
//!
//! The sensor runtime sends snapshots and events to a backend through a
//! `BackendTransport` trait. This decouples the sensor from any specific
//! backend protocol.
//!
//! Implementations:
//! - `NullBackend` — discards everything (for testing / local-only mode)
//! - `HttpBackend` — HTTP POST with HMAC auth, retry, bounded spool
//! - `SpoolBackend` — wraps another backend with a bounded local file spool
//!
//! The on-router build (`--no-default-features`) uses only pure-Rust HTTP
//! (ureq without TLS). TLS for the backend is a future enhancement that would
//! require the `persist` feature (rustls).

use crate::config::SensorConfig;
use crate::events::Event;
use crate::model::NetworkMap;
use crate::publisher::{append_bounded, drain_buffer, UploadPayload};
use crate::snapshot::SensorSnapshot;
use std::thread::sleep;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Backend transport contract. The sensor runtime calls these methods;
/// implementations handle delivery, retry, and offline buffering.
pub trait BackendTransport {
    /// Send a full snapshot (with events) to the backend.
    /// Returns `true` on success, `false` if the snapshot should be spooled.
    fn send_snapshot(&mut self, snapshot: &SensorSnapshot, events: &[Event], secret: &[u8])
        -> bool;

    /// Drain the offline spool (re-send buffered entries).
    /// Called at the start of each poll cycle.
    fn drain_spool(&mut self) {}

    /// Human-readable backend name for status/logging.
    fn name(&self) -> &str;

    /// Whether the backend is connected/reachable. Best-effort.
    fn is_connected(&self) -> bool {
        true
    }

    /// Current spool size in bytes (0 if no spool or empty).
    fn spool_size(&self) -> u64 {
        0
    }
}

// ---------------------------------------------------------------------------
// NullBackend — discards everything (testing / local-only)
// ---------------------------------------------------------------------------

pub struct NullBackend;

impl NullBackend {
    pub fn new() -> Self {
        Self
    }
}

impl BackendTransport for NullBackend {
    fn send_snapshot(
        &mut self,
        _snapshot: &SensorSnapshot,
        _events: &[Event],
        _secret: &[u8],
    ) -> bool {
        true // always "succeeds" (data is discarded)
    }

    fn name(&self) -> &str {
        "null"
    }

    fn is_connected(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// HttpBackend — HTTP POST with HMAC auth and retry
// ---------------------------------------------------------------------------

pub const HTTP_BACKEND_TIMEOUT: Duration = Duration::from_secs(10);
pub const HTTP_MAX_ATTEMPTS: usize = 3;

/// HTTP backend that POSTs pseudonymized payloads to a Detectic server.
/// On failure, snapshots are written to a bounded local spool file.
pub struct HttpBackend {
    url: String,
    sensor_id: String,
    agent: ureq::Agent,
    spool_path: std::path::PathBuf,
    spool_max: u64,
    connected: bool,
    /// Optional Bearer token for backend authentication (M8).
    backend_token: Option<String>,
}

impl HttpBackend {
    pub fn new(cfg: &SensorConfig) -> Self {
        let url = cfg.backend_url.clone().unwrap_or_default();
        let agent = ureq::AgentBuilder::new()
            .timeout(cfg.backend_timeout)
            .build();
        Self {
            url,
            sensor_id: cfg.sensor_id.clone(),
            agent,
            spool_path: cfg.spool_path.clone(),
            spool_max: cfg.spool_max_bytes,
            connected: false,
            backend_token: cfg.backend_token.clone(),
        }
    }

    /// Build an `UploadPayload` from a snapshot + events and serialize it.
    fn build_payload(
        &self,
        snapshot: &SensorSnapshot,
        events: &[Event],
        secret: &[u8],
    ) -> Option<UploadPayload> {
        // Convert SensorSnapshot back to NetworkMap for the payload builder
        let map = NetworkMap {
            captured_at: snapshot.timestamp,
            devices: snapshot.stations.clone(),
            raw: Default::default(),
        };
        Some(UploadPayload::from_map_with_events(
            &map,
            events,
            &self.sensor_id,
            secret,
        ))
    }

    /// Attempt to upload `body` with retry/backoff. Returns true on success.
    fn upload_with_retry(&mut self, body: &[u8], secret: &[u8]) -> bool {
        for attempt in 0..HTTP_MAX_ATTEMPTS {
            if attempt > 0 {
                let delay = backoff(attempt);
                crate::logging::warn(&format!(
                    "backend_retry attempt={}/{} delay={:?}",
                    attempt,
                    HTTP_MAX_ATTEMPTS - 1,
                    delay
                ));
                sleep(delay);
            }
            let sig = crate::hmac_sha256_hex(secret, body);
            let mut req = self
                .agent
                .post(&self.url)
                .set("Content-Type", "application/json")
                .set("X-Detectic-Sensor", &self.sensor_id)
                .set("X-Detectic-Signature", &sig);
            if let Some(token) = &self.backend_token {
                req = req.set("Authorization", &format!("Bearer {}", token));
            }
            match req.send_bytes(body) {
                Ok(resp) => {
                    let code = resp.status();
                    if code < 300 {
                        self.connected = true;
                        return true;
                    }
                    if code / 100 == 4 {
                        crate::logging::warn(&format!(
                            "backend_http_{} permanent not_buffering",
                            code
                        ));
                        self.connected = true;
                        return true; // don't buffer permanent failures
                    }
                    // 5xx: retry
                }
                Err(ureq::Error::Status(code, _)) if code / 100 == 4 => {
                    crate::logging::warn(&format!("backend_http_{} permanent not_buffering", code));
                    self.connected = true;
                    return true;
                }
                Err(_) => {
                    self.connected = false;
                    // network/timeout: retry
                }
            }
        }
        false
    }
}

impl BackendTransport for HttpBackend {
    fn send_snapshot(
        &mut self,
        snapshot: &SensorSnapshot,
        events: &[Event],
        secret: &[u8],
    ) -> bool {
        let payload = match self.build_payload(snapshot, events, secret) {
            Some(p) => p,
            None => return true, // serialization failed; don't crash
        };
        let body = payload.to_json_bytes();
        let sent = self.upload_with_retry(&body, secret);
        if !sent {
            // Spool for later retry
            let line = serde_json::to_string(&payload).unwrap_or_default();
            if !line.is_empty() {
                append_bounded(
                    self.spool_path
                        .to_str()
                        .unwrap_or("/var/run/misc/misc_rw/detectic/spool/detectic_buffer.jsonl"),
                    &line,
                    self.spool_max,
                );
            }
        }
        sent
    }

    fn drain_spool(&mut self) {
        // The HttpBackend's spool drain requires the per-sensor secret for
        // HMAC re-signing. The runtime passes the secret through the
        // SpoolBackend wrapper, which handles proper re-signing. For the
        // standalone HttpBackend, the spool is drained by the runtime via
        // `drain_buffer` directly. This method is a no-op here.
    }

    fn name(&self) -> &str {
        "http"
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn spool_size(&self) -> u64 {
        std::fs::metadata(&self.spool_path)
            .map(|m| m.len())
            .unwrap_or(0)
    }
}

/// Bounded exponential backoff: 0, 1, 2, 4, 8, 8, 8... seconds.
pub fn backoff(attempt: usize) -> Duration {
    if attempt == 0 {
        return Duration::ZERO;
    }
    let secs = (1u64 << (attempt.saturating_sub(1))).min(8);
    Duration::from_secs(secs)
}

// ---------------------------------------------------------------------------
// SpoolBackend — wraps any backend with a bounded JSONL file spool
// ---------------------------------------------------------------------------

/// A wrapper that adds offline buffering to any backend.
/// On send failure, the payload is appended to a bounded JSONL file.
/// On each poll cycle, `drain_spool` attempts to re-send buffered entries.
pub struct SpoolBackend {
    inner: Box<dyn BackendTransport>,
    spool_path: String,
    spool_max: u64,
    secret: Vec<u8>,
}

impl SpoolBackend {
    pub fn new(
        inner: Box<dyn BackendTransport>,
        spool_path: &str,
        spool_max: u64,
        secret: &[u8],
    ) -> Self {
        Self {
            inner,
            spool_path: spool_path.to_string(),
            spool_max,
            secret: secret.to_vec(),
        }
    }

    /// Drain the spool by re-sending each entry through the inner backend.
    /// Each spool line is a serialized `UploadPayload`. We reconstruct a
    /// minimal `SensorSnapshot` from it and call `send_snapshot`.
    pub fn drain(&mut self) {
        let secret = self.secret.clone();
        let inner = &mut self.inner;
        drain_buffer(&self.spool_path, |body: &[u8]| {
            if let Ok(payload) = serde_json::from_slice::<UploadPayload>(body) {
                let snapshot = SensorSnapshot {
                    timestamp: payload.captured_at,
                    stations: payload
                        .devices
                        .iter()
                        .map(|d| crate::model::Device {
                            rssi: d.rssi,
                            standard: d.standard.clone(),
                            source: d.source.clone(),
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                };
                inner.send_snapshot(&snapshot, &payload.events, &secret)
            } else {
                false
            }
        });
    }
}

impl BackendTransport for SpoolBackend {
    fn send_snapshot(
        &mut self,
        snapshot: &SensorSnapshot,
        events: &[Event],
        secret: &[u8],
    ) -> bool {
        let sent = self.inner.send_snapshot(snapshot, events, secret);
        if !sent {
            // Spool the payload
            let map = NetworkMap {
                captured_at: snapshot.timestamp,
                devices: snapshot.stations.clone(),
                raw: Default::default(),
            };
            let sensor_id = "unknown"; // inner backend has the real sensor_id
            let payload =
                UploadPayload::from_map_with_events(&map, events, sensor_id, &self.secret);
            if let Ok(line) = serde_json::to_string(&payload) {
                if !line.is_empty() {
                    append_bounded(&self.spool_path, &line, self.spool_max);
                }
            }
        }
        sent
    }

    fn drain_spool(&mut self) {
        self.drain();
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    fn spool_size(&self) -> u64 {
        std::fs::metadata(&self.spool_path)
            .map(|m| m.len())
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Spool-only backend (no network — just writes to local file)
// ---------------------------------------------------------------------------

/// A backend that only writes to a local JSONL spool file.
/// Useful for local testing and when no backend is configured.
pub struct LocalSpoolBackend {
    spool_path: String,
    spool_max: u64,
    sensor_id: String,
}

impl LocalSpoolBackend {
    pub fn new(cfg: &SensorConfig) -> Self {
        Self {
            spool_path: cfg
                .spool_path
                .to_str()
                .unwrap_or("/var/run/misc/misc_rw/detectic/spool/detectic_buffer.jsonl")
                .to_string(),
            spool_max: cfg.spool_max_bytes,
            sensor_id: cfg.sensor_id.clone(),
        }
    }
}

impl BackendTransport for LocalSpoolBackend {
    fn send_snapshot(
        &mut self,
        snapshot: &SensorSnapshot,
        events: &[Event],
        secret: &[u8],
    ) -> bool {
        let map = NetworkMap {
            captured_at: snapshot.timestamp,
            devices: snapshot.stations.clone(),
            raw: Default::default(),
        };
        let payload = UploadPayload::from_map_with_events(&map, events, &self.sensor_id, secret);
        if let Ok(line) = serde_json::to_string(&payload) {
            if !line.is_empty() {
                append_bounded(&self.spool_path, &line, self.spool_max);
            }
        }
        true // always "succeeds" (data is buffered locally)
    }

    fn name(&self) -> &str {
        "local-spool"
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn spool_size(&self) -> u64 {
        std::fs::metadata(&self.spool_path)
            .map(|m| m.len())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Device;

    fn station(mac: &str, rssi: Option<i64>) -> Device {
        Device {
            hostname: Some("h".into()),
            ip: Some("10.0.0.1".into()),
            mac: Some(mac.into()),
            rssi,
            standard: Some("ax".into()),
            onemesh_stack: None,
            assoc_time: None,
            radio_mac: None,
            source: Some("wifi".into()),
            tx_rate: None,
            rx_rate: None,
            noise: None,
            signal_level: None,
            max_link_rate: None,
            interface: None,
            ipv6: None,
            client_type: None,
            active: None,
        }
    }

    fn sample_snapshot() -> SensorSnapshot {
        SensorSnapshot {
            timestamp: 1000,
            stations: vec![station("AA:BB:CC:00:00:01", Some(50))],
            ..Default::default()
        }
    }

    #[test]
    fn null_backend_always_succeeds() {
        let mut backend = NullBackend::new();
        let snap = sample_snapshot();
        assert!(backend.send_snapshot(&snap, &[], b"secret"));
        assert_eq!(backend.name(), "null");
    }

    #[test]
    fn local_spool_writes_to_file() {
        let dir = std::env::temp_dir();
        let path = dir.join("detectic_backend_spool_test.jsonl");
        let _ = std::fs::remove_file(&path);
        let cfg = SensorConfig {
            spool_path: path.clone(),
            sensor_id: "test-001".into(),
            router_password: "pw".into(),
            secret: "sk".into(),
            ..Default::default()
        };
        let mut backend = LocalSpoolBackend::new(&cfg);
        let snap = sample_snapshot();
        assert!(backend.send_snapshot(&snap, &[], b"secret"));
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("test-001"));
        assert!(!content.contains("AA:BB:CC")); // pseudonymized
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn backoff_grows_then_caps() {
        assert_eq!(backoff(0), Duration::ZERO);
        assert_eq!(backoff(1), Duration::from_secs(1));
        assert_eq!(backoff(2), Duration::from_secs(2));
        assert_eq!(backoff(3), Duration::from_secs(4));
        assert_eq!(backoff(4), Duration::from_secs(8));
        assert_eq!(backoff(7), Duration::from_secs(8));
    }

    #[test]
    fn spool_size_returns_zero_for_missing_file() {
        let backend = NullBackend::new();
        assert_eq!(backend.spool_size(), 0);
    }
}
