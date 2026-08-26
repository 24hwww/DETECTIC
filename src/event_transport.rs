//! Event transport abstraction with delivery guarantees (Phases 7–8).
//!
//! [`EventTransport`] decouples the sensor from any specific realtime channel.
//! [`HttpEventTransport`] posts canonical envelopes over HTTPS with the
//! existing HMAC contract (extended with `X-Detectic-Timestamp` replay
//! protection). [`SpoolEventTransport`] adds a bounded on-disk JSONL spool so
//! observation never depends on backend availability: undelivered events are
//! persisted and re-sent with their original `event_id`/`sequence`, making
//! redelivery idempotent at the server (`events.event_id UNIQUE`).

use crate::temporal::EventEnvelope;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

pub trait EventTransport {
    /// Attempt delivery. Returns the event_ids acknowledged by the receiver.
    fn send_events(&mut self, events: &[EventEnvelope]) -> Vec<String>;

    fn name(&self) -> &str;

    fn is_connected(&self) -> bool;
}

#[derive(Debug, Clone, Serialize)]
struct EventBatch<'a> {
    events: &'a [EventEnvelope],
}

const HTTP_MAX_ATTEMPTS: usize = 3;

pub struct HttpEventTransport {
    url: String,
    sensor_id: String,
    secret: Vec<u8>,
    agent: ureq::Agent,
    connected: bool,
}

impl HttpEventTransport {
    pub fn new(base_url: &str, sensor_id: &str, secret: &[u8], timeout: Duration) -> Self {
        let mut url = base_url.trim_end_matches('/').to_string();
        if !url.ends_with("/api/v1/events") {
            url.push_str("/api/v1/events");
        }
        Self {
            url,
            sensor_id: sensor_id.to_string(),
            secret: secret.to_vec(),
            agent: ureq::AgentBuilder::new().timeout(timeout).build(),
            connected: false,
        }
    }
}

impl EventTransport for HttpEventTransport {
    fn send_events(&mut self, events: &[EventEnvelope]) -> Vec<String> {
        if events.is_empty() {
            return Vec::new();
        }
        let body = match serde_json::to_vec(&EventBatch { events }) {
            Ok(b) => b,
            Err(_) => return Vec::new(),
        };

        for attempt in 0..HTTP_MAX_ATTEMPTS {
            if attempt > 0 {
                let secs = (1u64 << (attempt - 1)).min(8);
                std::thread::sleep(Duration::from_secs(secs));
            }
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let mut signed_msg = format!("{}\n", ts).into_bytes();
            signed_msg.extend_from_slice(&body);
            let sig = crate::hmac_sha256_hex(&self.secret, &signed_msg);
            let result = self
                .agent
                .post(&self.url)
                .set("Content-Type", "application/json")
                .set("X-Detectic-Sensor", &self.sensor_id)
                .set("X-Detectic-Signature", &sig)
                .set("X-Detectic-Timestamp", &ts.to_string())
                .send_bytes(&body)
                .map(|resp| parse_ack(resp))
                .map_err(classify_send_error);
            match result {
                Ok(Ok(ids)) => {
                    self.connected = true;
                    return ids;
                }
                Ok(Err(SendError::Permanent)) | Err(SendError::Permanent) => {
                    self.connected = true;
                    return Vec::new();
                }
                Ok(Err(SendError::Transient)) | Err(SendError::Transient) => {
                    self.connected = false;
                }
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "http-events"
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

enum SendError {
    Transient,
    Permanent,
}

fn parse_ack(resp: ureq::Response) -> Result<Vec<String>, SendError> {
    let status = resp.status();
    if status >= 300 && status < 500 {
        return Err(SendError::Permanent);
    }
    if status >= 500 {
        return Err(SendError::Transient);
    }
    let mut text = String::new();
    if std::io::Read::read_to_string(&mut resp.into_reader(), &mut text).is_err() {
        return Err(SendError::Transient);
    }
    #[derive(Deserialize)]
    struct Ack {
        #[serde(default)]
        accepted_ids: Vec<String>,
    }
    Ok(serde_json::from_str::<Ack>(&text)
        .ok()
        .map(|a| a.accepted_ids)
        .unwrap_or_default())
}

fn classify_send_error(err: ureq::Error) -> SendError {
    match err {
        ureq::Error::Status(code, _) if code / 100 == 4 => SendError::Permanent,
        _ => SendError::Transient,
    }
}

/// Bounded spool wrapper: failed batches are appended as JSONL lines and
/// re-delivered on [`SpoolEventTransport::drain`]. The spool never grows past
/// `max_bytes`; oldest lines are dropped first when exceeded.
pub struct SpoolEventTransport {
    inner: Box<dyn EventTransport>,
    path: PathBuf,
    max_bytes: u64,
}

impl SpoolEventTransport {
    pub fn new(inner: Box<dyn EventTransport>, path: impl Into<PathBuf>, max_bytes: u64) -> Self {
        Self {
            inner,
            path: path.into(),
            max_bytes,
        }
    }

    pub fn drain(&mut self) -> usize {
        let lines = match std::fs::read_to_string(&self.path) {
            Ok(content) => content
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(String::from)
                .collect::<Vec<_>>(),
            Err(_) => return 0,
        };
        if lines.is_empty() {
            return 0;
        }
        let mut delivered = 0usize;
        let mut remaining: Vec<String> = Vec::new();
        for line in lines {
            match serde_json::from_str::<EventEnvelope>(&line) {
                Ok(env) => {
                    let acked = self.inner.send_events(std::slice::from_ref(&env));
                    if acked.iter().any(|id| *id == env.event_id) {
                        delivered += 1;
                    } else {
                        remaining.push(line);
                    }
                }
                Err(_) => {}
            }
        }
        if remaining.is_empty() {
            let _ = std::fs::remove_file(&self.path);
        } else if let Ok(mut f) = std::fs::File::create(&self.path) {
            for line in &remaining {
                let _ = f.write_all(line.as_bytes());
                let _ = f.write_all(b"\n");
            }
        }
        delivered
    }

    fn append_bounded(&self, line: &str) {
        let existing_len = std::fs::metadata(&self.path)
            .map(|m| m.len())
            .unwrap_or(0);
        let line_len = line.len() as u64 + 1;
        if line_len > self.max_bytes {
            return;
        }
        if existing_len + line_len > self.max_bytes {
            let content = std::fs::read_to_string(&self.path).unwrap_or_default();
            let kept: String = {
                let lines: Vec<&str> = content.lines().collect();
                let mut keep_from = lines.len();
                let mut total = 0u64;
                for (i, l) in lines.iter().enumerate().rev() {
                    total += l.len() as u64 + 1;
                    if total + line_len > self.max_bytes {
                        break;
                    }
                    keep_from = i;
                }
                lines[keep_from.min(lines.len())..]
                    .join("\n")
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| format!("{l}\n"))
                    .collect()
            };
            if let Ok(mut f) = std::fs::File::create(&self.path) {
                let _ = f.write_all(kept.as_bytes());
            }
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = f.write_all(line.as_bytes());
            let _ = f.write_all(b"\n");
        }
    }
}

impl EventTransport for SpoolEventTransport {
    fn send_events(&mut self, events: &[EventEnvelope]) -> Vec<String> {
        let acked = self.inner.send_events(events);
        if acked.len() == events.len() {
            return acked;
        }
        let acked_set: std::collections::HashSet<&String> = acked.iter().collect();
        for env in events {
            if !acked_set.contains(&env.event_id) {
                if let Ok(line) = serde_json::to_string(env) {
                    self.append_bounded(&line);
                }
            }
        }
        acked
    }

    fn name(&self) -> &str {
        "spool-events"
    }

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }
}

/// In-memory pending queue enforcing resource bounds (Phase 18): a maximum
/// number of queued events and a maximum serialized size per event.
pub struct ReliableQueue {
    pending: VecDeque<EventEnvelope>,
    max_pending: usize,
    max_event_bytes: usize,
    dropped_events: u64,
}

#[derive(Debug, Default, PartialEq)]
pub struct FlushReport {
    pub sent: usize,
    pub kept: usize,
    pub dropped_overflow: usize,
}

impl Default for ReliableQueue {
    fn default() -> Self {
        Self::new(1024, 8192)
    }
}

impl ReliableQueue {
    pub fn new(max_pending: usize, max_event_bytes: usize) -> Self {
        Self {
            pending: VecDeque::new(),
            max_pending,
            max_event_bytes,
            dropped_events: 0,
        }
    }

    pub fn submit(&mut self, events: Vec<EventEnvelope>) {
        for ev in events {
            let too_big = serde_json::to_vec(&ev)
                .map(|b| b.len() > self.max_event_bytes)
                .unwrap_or(true);
            if too_big {
                self.dropped_events += 1;
                continue;
            }
            self.pending.push_back(ev);
        }
        while self.pending.len() > self.max_pending {
            self.pending.pop_front();
            self.dropped_events += 1;
        }
    }

    /// Attempt to flush all pending events through the transport. Events not
    /// acknowledged stay queued in original order.
    pub fn flush(&mut self, transport: &mut dyn EventTransport) -> FlushReport {
        let batch: Vec<EventEnvelope> = self.pending.iter().cloned().collect();
        if batch.is_empty() {
            return FlushReport::default();
        }
        let acked: std::collections::HashSet<String> =
            transport.send_events(&batch).into_iter().collect();
        let before = self.pending.len();
        self.pending.retain(|e| !acked.contains(&e.event_id));
        FlushReport {
            sent: before - self.pending.len(),
            kept: self.pending.len(),
            dropped_overflow: 0,
        }
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn dropped_total(&self) -> u64 {
        self.dropped_events
    }

    pub fn last_sequence(&self) -> Option<u64> {
        self.pending.back().map(|e| e.sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn envelope(seq: u64, id: &str) -> EventEnvelope {
        EventEnvelope {
            event_id: id.to_string(),
            sequence: seq,
            sensor_id: "s1".into(),
            timestamp: 1000,
            event_type: crate::temporal::EventType::DeviceConnected,
            device_id: Some("h01".into()),
            payload: serde_json::json!({}),
        }
    }

    struct MockTransport {
        ack_all: bool,
        calls: RefCell<Vec<usize>>,
    }

    impl MockTransport {
        fn new(ack_all: bool) -> Self {
            Self {
                ack_all,
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl EventTransport for MockTransport {
        fn send_events(&mut self, events: &[EventEnvelope]) -> Vec<String> {
            self.calls.borrow_mut().push(events.len());
            if self.ack_all {
                events.iter().map(|e| e.event_id.clone()).collect()
            } else {
                Vec::new()
            }
        }
        fn name(&self) -> &str {
            "mock"
        }
        fn is_connected(&self) -> bool {
            true
        }
    }

    #[test]
    fn queue_flush_removes_acked_keeps_unacked_in_order() {
        let mut q = ReliableQueue::new(16, 4096);
        q.submit(vec![envelope(1, "e1"), envelope(2, "e2"), envelope(3, "e3")]);

        struct PartialAck;
        impl EventTransport for PartialAck {
            fn send_events(&mut self, events: &[EventEnvelope]) -> Vec<String> {
                events
                    .iter()
                    .filter(|e| e.event_id != "e2")
                    .map(|e| e.event_id.clone())
                    .collect()
            }
            fn name(&self) -> &str {
                "partial"
            }
            fn is_connected(&self) -> bool {
                true
            }
        }

        let mut t = PartialAck;
        let report = q.flush(&mut t);
        assert_eq!(report.sent, 2);
        assert_eq!(report.kept, 1);
        assert_eq!(q.pending_len(), 1);

        let mut full = MockTransport::new(true);
        let report = q.flush(&mut full);
        assert_eq!(report.sent, 1);
        assert_eq!(q.pending_len(), 0);
    }

    #[test]
    fn queue_bounds_drop_oldest() {
        let mut q = ReliableQueue::new(3, 4096);
        q.submit(vec![envelope(1, "a"), envelope(2, "b")]);
        q.submit(vec![envelope(3, "c"), envelope(4, "d"), envelope(5, "e")]);
        assert!(q.pending_len() <= 3);
        assert_eq!(
            q.last_sequence(),
            Some(5),
            "newest events retained under bound"
        );
        assert!(q.dropped_total() >= 2);
    }

    #[test]
    fn oversized_events_rejected() {
        let mut q = ReliableQueue::new(10, 200);
        let mut big = envelope(1, "big");
        big.payload = serde_json::json!({ "blob": "x".repeat(256) });
        q.submit(vec![envelope(2, "ok"), big]);
        assert_eq!(q.pending_len(), 1);
        assert_eq!(q.dropped_total(), 1);
    }

    #[test]
    fn empty_flush_is_noop() {
        let mut q = ReliableQueue::default();
        let mut t = MockTransport::new(false);
        let report = q.flush(&mut t);
        assert_eq!(report, FlushReport::default());
        assert!(t.calls.borrow().is_empty());
    }

    #[test]
    fn spool_persists_unacked_and_redelivers_with_same_id() {
        use std::env;
        let dir = env::temp_dir().join(format!("detectic-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("spool.jsonl");

        struct FailOnce {
            fail: RefCell<bool>,
            seen: RefCell<Vec<EventEnvelope>>,
        }
        impl EventTransport for FailOnce {
            fn send_events(&mut self, events: &[EventEnvelope]) -> Vec<String> {
                if *self.fail.borrow() {
                    *self.fail.borrow_mut() = false;
                    return Vec::new();
                }
                self.seen.borrow_mut().extend_from_slice(events);
                events.iter().map(|e| e.event_id.clone()).collect()
            }
            fn name(&self) -> &str {
                "fail-once"
            }
            fn is_connected(&self) -> bool {
                true
            }
        }

        let inner = Box::new(FailOnce {
            fail: RefCell::new(true),
            seen: RefCell::new(Vec::new()),
        });
        let mut t = SpoolEventTransport::new(inner, &path, 65536);

        let batch = vec![envelope(7, "x7"), envelope(8, "x8")];
        let acked = t.send_events(&batch);
        assert!(acked.is_empty(), "first attempt fails");

        t.drain();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn spool_respects_size_bound() {
        use std::env;
        let dir = env::temp_dir().join(format!("detectic-bound-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("spool.jsonl");
        let noop = MockTransport::new(false);
        let mut t = SpoolEventTransport::new(Box::new(noop), &path, 512);
        for i in 0..50 {
            let batch = vec![envelope(i, &format!("e{i}"))];
            t.send_events(&batch);
        }
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        assert!(size <= 600, "spool must stay bounded, got {size}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
