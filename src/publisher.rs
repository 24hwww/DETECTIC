//! Publisher layer — pseudonymized backend upload.
//!
//! Owns `UploadPayload` construction, HMAC signing, retry/backoff and the
//! bounded offline buffer. No knowledge of transport internals; it receives a
//! ready `NetworkMap` from the collector.

use crate::events::Event;
use crate::model::NetworkMap;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::thread::sleep;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Payload — privacy-preserving by construction (AGENTS.md §21/§39)
// ---------------------------------------------------------------------------

/// Upload body: devices are identified only by a locally-derived pseudonym.
/// Raw MAC/IP/hostname and the OID `raw` blob are never serialized.
#[derive(Serialize, Deserialize)]
pub struct UploadDevice {
    pub pseudonym: String,
    pub rssi: Option<i64>,
    pub standard: Option<String>,
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radio_mac: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct UploadPayload {
    pub sensor_id: String,
    /// Deterministic idempotency key (HMAC over sensor_id|captured_at|sorted pseudos).
    pub id: String,
    pub captured_at: i64,
    pub devices: Vec<UploadDevice>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub events: Vec<Event>,
}

impl UploadPayload {
    pub fn from_map(map: &NetworkMap, sensor_id: &str, secret: &[u8]) -> Self {
        Self::from_map_with_events(map, &[], sensor_id, secret)
    }

    /// Build a payload that includes both the current snapshot and the privacy-safe
    /// change events that produced it.
    pub fn from_map_with_events(
        map: &NetworkMap,
        events: &[Event],
        sensor_id: &str,
        secret: &[u8],
    ) -> Self {
        let mut pseudos: Vec<String> = map
            .devices
            .iter()
            .map(|d| crate::pseudonymize(secret, &d.identity()))
            .collect();
        pseudos.sort();
        let id = crate::hmac_sha256_hex(
            secret,
            format!("{}|{}|{}", sensor_id, map.captured_at, pseudos.join(",")).as_bytes(),
        );
        UploadPayload {
            sensor_id: sensor_id.to_string(),
            id,
            captured_at: map.captured_at,
            devices: map
                .devices
                .iter()
                .map(|d| UploadDevice {
                    pseudonym: crate::pseudonymize(secret, &d.identity()),
                    rssi: d.rssi,
                    standard: d.standard.clone(),
                    source: d.source.clone(),
                    radio_mac: d
                        .radio_mac
                        .as_deref()
                        .map(|r| crate::pseudonymize(secret, r)),
                })
                .collect(),
            events: events.to_vec(),
        }
    }

    pub fn to_json_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// HTTP + retry/backoff
// ---------------------------------------------------------------------------

pub const UPLOAD_TIMEOUT: Duration = Duration::from_secs(10);
pub const MAX_ATTEMPTS: usize = 3;

/// Backoff delay before the `attempt`-th retry (0 = initial, no wait).
pub fn backoff_delay(attempt: usize) -> Duration {
    if attempt == 0 {
        return Duration::ZERO;
    }
    let secs = (1u64 << (attempt.saturating_sub(1))).min(8);
    Duration::from_secs(secs)
}

/// Authenticated upload with retry/backoff.
/// Returns `true` on success or permanent 4xx (should not be retried/buffered);
/// `false` only when the upload ultimately failed and must be buffered.
pub fn upload_with_retry(
    agent: &ureq::Agent,
    url: &str,
    sensor_id: &str,
    secret: &[u8],
    body: &[u8],
) -> bool {
    for attempt in 0..MAX_ATTEMPTS {
        if attempt > 0 {
            eprintln!(
                "[detectic] upload retry {}/{} after {:?}",
                attempt,
                MAX_ATTEMPTS - 1,
                backoff_delay(attempt)
            );
            sleep(backoff_delay(attempt));
        }
        let sig = crate::hmac_sha256_hex(secret, body);
        match agent
            .post(url)
            .set("Content-Type", "application/json")
            .set("X-Detectic-Sensor", sensor_id)
            .set("X-Detectic-Signature", &sig)
            .send_bytes(body)
        {
            Ok(resp) => {
                let code = resp.status();
                if code < 300 {
                    return true;
                }
                if code / 100 == 4 {
                    eprintln!("[detectic] upload HTTP {} — permanent, not buffering (check DETECTIC_SECRET/UPLOAD_URL)", code);
                    return true;
                }
            }
            Err(ureq::Error::Status(code, _)) if code / 100 == 4 => {
                eprintln!("[detectic] upload HTTP {} — permanent, not buffering (check DETECTIC_SECRET/UPLOAD_URL)", code);
                return true;
            }
            Err(_) => { /* network/timeout/5xx: retry */ }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Offline buffer (JSONL, bounded)
// ---------------------------------------------------------------------------

/// Extract the JSON payload from a buffered line `"<ts> <json>"`.
pub fn parse_buffer_line(line: &str) -> &str {
    match line.find(' ') {
        Some(i) => &line[i + 1..],
        None => "",
    }
}

/// Drain the offline buffer: re-send each entry; drop on success, keep failures.
pub fn drain_buffer<F: FnMut(&[u8]) -> bool>(path: &str, mut uploader: F) {
    let data = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => return,
    };
    let mut remaining: Vec<String> = Vec::new();
    for raw in data.lines() {
        if raw.is_empty() {
            continue;
        }
        let entry = parse_buffer_line(raw);
        if entry.is_empty() || !uploader(entry.as_bytes()) {
            remaining.push(raw.to_string());
        }
    }
    if remaining.is_empty() {
        let _ = std::fs::remove_file(path);
    } else {
        let _ = std::fs::write(path, format!("{}\n", remaining.join("\n")));
    }
}

/// Append one JSONL line, keeping the file bounded by dropping oldest entries.
/// Never splits a line; never fills the filesystem (AGENTS.md §29).
pub fn append_bounded(path: &str, line: &str, max: u64) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let entry = format!("{} {}\n", ts, line);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(entry.as_bytes());
    }
    if let Ok(data) = std::fs::read(path) {
        if (data.len() as u64) > max {
            let mut keep_from = data.len().saturating_sub(max as usize);
            if let Some(nl) = data[keep_from..].iter().position(|&b| b == b'\n') {
                keep_from += nl + 1;
            }
            let _ = std::fs::write(path, &data[keep_from..]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Device, NetworkMap};

    fn sample_device(mac: &str, rssi: Option<i64>) -> Device {
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

    #[test]
    fn backoff_grows_then_caps() {
        assert_eq!(backoff_delay(0), Duration::ZERO);
        assert_eq!(backoff_delay(1), Duration::from_secs(1));
        assert_eq!(backoff_delay(2), Duration::from_secs(2));
        assert_eq!(backoff_delay(3), Duration::from_secs(4));
        assert_eq!(backoff_delay(4), Duration::from_secs(8));
        assert_eq!(backoff_delay(7), Duration::from_secs(8));
    }

    #[test]
    fn parse_buffer_line_strips_timestamp() {
        assert_eq!(parse_buffer_line("1700000000 {\"a\":1}"), "{\"a\":1}");
        assert_eq!(parse_buffer_line("nojson"), "");
    }

    #[test]
    fn upload_payload_is_stable_and_identifiably_pseudonymized() {
        let m = NetworkMap {
            captured_at: 100,
            devices: vec![sample_device("AA:BB:CC:11:22:33", Some(50))],
            raw: Default::default(),
        };
        let p1 = UploadPayload::from_map(&m, "home-001", b"secret");
        let p2 = UploadPayload::from_map(&m, "home-001", b"secret");
        assert_eq!(p1.id, p2.id);
        assert_eq!(p1.devices.len(), 1);
        let json = serde_json::to_string(&p1).unwrap();
        assert!(!json.contains("AA:BB:CC"));
        assert!(!json.contains("10.0.0.1"));
        assert!(!json.contains("\"raw\""));
        let p3 = UploadPayload::from_map(&m, "home-001", b"other-secret");
        assert_ne!(p1.id, p3.id);
        assert_ne!(p1.devices[0].pseudonym, p3.devices[0].pseudonym);
        let m2 = NetworkMap {
            captured_at: 200,
            devices: vec![sample_device("AA:BB:CC:11:22:33", Some(-60))],
            raw: Default::default(),
        };
        let p4 = UploadPayload::from_map(&m2, "home-001", b"secret");
        assert_eq!(p1.devices[0].pseudonym, p4.devices[0].pseudonym);
        assert_ne!(p1.id, p4.id);
    }

    #[test]
    fn radio_mac_is_pseudonymized_in_upload() {
        let mut d = sample_device("AA:BB:CC:11:22:33", Some(50));
        d.radio_mac = Some("00:11:22:33:44:55".into());
        let m = NetworkMap {
            captured_at: 100,
            devices: vec![d],
            raw: Default::default(),
        };
        let p = UploadPayload::from_map(&m, "home-001", b"secret");
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("00:11:22:33:44:55"));
        assert!(p.devices[0].radio_mac.is_some());
        assert_eq!(
            p.devices[0].radio_mac,
            Some(crate::pseudonymize(b"secret", "00:11:22:33:44:55"))
        );
    }

    #[test]
    fn events_included_in_payload() {
        use crate::events::{Event, EventKind};
        let m = NetworkMap {
            captured_at: 100,
            devices: vec![sample_device("AA:BB:CC:11:22:33", Some(50))],
            raw: Default::default(),
        };
        let events = vec![Event {
            captured_at: 100,
            kind: EventKind::DeviceJoined,
            pseudonym: "abc".into(),
            identity: "AA:BB:CC:11:22:33".into(),
            changed_fields: vec![],
        }];
        let p = UploadPayload::from_map_with_events(&m, &events, "home-001", b"secret");
        assert_eq!(p.events.len(), 1);
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("AA:BB:CC"));
        assert!(!json.contains("10.0.0.1"));
    }

    #[test]
    fn append_bounded_never_splits_a_line() {
        let dir = std::env::temp_dir();
        let path = dir.join("detectic_pub_append_test.jsonl");
        let _ = std::fs::remove_file(&path);
        for _ in 0..2 {
            append_bounded(
                path.to_str().unwrap(),
                "{\"device\":\"aa:bb:cc:dd:ee:ff\",\"rssi\":-50}",
                10,
            );
        }
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(!content.contains("aa:bb:cc:dd:ee:ff"));
        let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
        for l in &lines {
            assert!(l.contains('{') && l.contains('}'));
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn drain_buffer_drops_sent_keeps_failed() {
        let dir = std::env::temp_dir();
        let path = dir.join("detectic_pub_drain_test.jsonl");
        std::fs::write(
            &path,
            "1700000000 {\"id\":\"a\"}\n1700000001 {\"id\":\"b\"}\n",
        )
        .unwrap();
        drain_buffer(path.to_str().unwrap(), |_b| true);
        assert!(!path.exists());
        std::fs::write(
            &path,
            "1700000000 {\"id\":\"a\"}\n1700000001 {\"id\":\"b\"}\n",
        )
        .unwrap();
        drain_buffer(path.to_str().unwrap(), |b| !b.ends_with(b"\"b\"}"));
        let remaining = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(remaining.contains("\"id\":\"b\""));
        assert!(!remaining.contains("\"id\":\"a\""));
        assert_eq!(remaining.lines().filter(|l| !l.is_empty()).count(), 1);
        let _ = std::fs::remove_file(&path);
    }
}
