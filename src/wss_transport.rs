//! Persistent WebSocket event transport for the Detectic sensor.
//!
//! This sends canonical events over a WSS connection to the Cloudflare
//! RealtimeHub Durable Object. It maintains the connection between flushes,
//! reconnects with backoff on error, and returns the event_ids that the server
//! acknowledged.

use crate::temporal::EventEnvelope;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::time::{Duration, Instant};
use tungstenite::protocol::{Message, WebSocket};
use tungstenite::stream::MaybeTlsStream;

const WSS_PROTOCOL: u8 = 1;
// Allow the DO time to persist each event and reply.
const ACK_TIMEOUT: Duration = Duration::from_secs(30);
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);

pub struct WssEventTransport {
    base_url: String,
    socket: Option<WebSocket<MaybeTlsStream<TcpStream>>>,
    connected: bool,
    last_ok: Instant,
    backoff: Duration,
}

impl WssEventTransport {
    pub fn new(base_url: &str, sensor_id: &str) -> Self {
        let mut url = base_url.trim_end_matches('/').to_string();
        if !url.contains('?') {
            url.push_str("?role=sensor&sensor_id=");
            url.push_str(sensor_id);
        } else if !url.contains("sensor_id=") {
            url.push_str("&role=sensor&sensor_id=");
            url.push_str(sensor_id);
        }
        Self {
            base_url: url,
            socket: None,
            connected: false,
            last_ok: Instant::now(),
            backoff: Duration::from_secs(1),
        }
    }

    fn ensure(&mut self) -> bool {
        if let Some(ref s) = self.socket {
            if s.can_write() && s.can_read() && self.last_ok.elapsed() < IDLE_TIMEOUT {
                return true;
            }
        }
        self.socket = None;
        self.connected = false;

        let _ = rustls::crypto::ring::default_provider().install_default();

        if self.backoff > Duration::from_secs(0) {
            std::thread::sleep(self.backoff);
        }

        match tungstenite::connect(&self.base_url) {
            Ok((mut ws, _resp)) => {
                // Read the automatic hello_ack the DO sends on connection.
                if let Ok(Message::Text(_)) = ws.read() {
                    // send hello and wait for the hello_ack (and diagnostic command)
                    let hello = serde_json::json!({
                        "type": "hello",
                        "protocol": WSS_PROTOCOL,
                    });
                    if ws.send(Message::Text(hello.to_string())).is_ok() {
                        // drain hello_ack + optional command
                        let _ = drain_until(&mut ws, &["hello_ack", "command", "command_ack_ok"], Duration::from_secs(2));
                        self.socket = Some(ws);
                        self.connected = true;
                        self.last_ok = Instant::now();
                        self.backoff = Duration::from_secs(1);
                        crate::logging::info("wss_connected");
                        return true;
                    }
                }
                let _ = ws.close(None);
                crate::logging::warn("wss_handshake_failed");
                false
            }
            Err(e) => {
                crate::logging::warn(&format!("wss_connect_error err={} url={}", e, self.base_url));
                self.backoff = (self.backoff * 2).min(Duration::from_secs(60));
                false
            }
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }
}

fn set_read_timeout(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>, d: Option<Duration>) {
    fn set_tcp(s: &TcpStream, d: Option<Duration>) -> std::io::Result<()> {
        s.set_read_timeout(d)
    }
    let _ = match ws.get_mut() {
        MaybeTlsStream::Plain(s) => set_tcp(s, d),
        MaybeTlsStream::Rustls(s) => set_tcp(s.get_mut(), d),
        _ => Ok(()),
    };
}

fn parse_msg(text: &str) -> Option<serde_json::Value> {
    serde_json::from_str(text).ok()
}

fn drain_until(
    ws: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    stop_types: &[&str],
    timeout: Duration,
) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let remaining = timeout - start.elapsed();
        set_read_timeout(ws, Some(remaining));
        match ws.read() {
            Ok(Message::Text(t)) => {
                if let Some(v) = parse_msg(&t) {
                    if let Some(ty) = v.get("type").and_then(|x| x.as_str()) {
                        if stop_types.contains(&ty) {
                            return;
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(_) => return,
        }
    }
}

fn is_timeout_or_closed(e: &tungstenite::Error) -> bool {
    matches!(
        e,
        tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed
    ) || matches!(e, tungstenite::Error::Io(ref i) if matches!(i.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::NotConnected))
}

impl crate::event_transport::EventTransport for WssEventTransport {
    fn send_events(&mut self, events: &[EventEnvelope]) -> Vec<String> {
        if events.is_empty() {
            return Vec::new();
        }

        if !self.ensure() {
            return Vec::new();
        }

        let ws = match self.socket.as_mut() {
            Some(s) => s,
            None => return Vec::new(),
        };

        let mut expected: std::collections::HashSet<&str> = events.iter().map(|e| e.event_id.as_str()).collect();
        let mut acked: Vec<String> = Vec::with_capacity(events.len());

        for env in events {
            let msg = serde_json::json!({
                "type": "event",
                "protocol": WSS_PROTOCOL,
                "event_id": env.event_id,
                "sensor_id": env.sensor_id,
                "observed_at": env.timestamp * 1000,
                "payload": env,
            });
            crate::logging::info(&format!(
                "T4_SEND event_id={} sensor_id={} observed_at={}",
                env.event_id, env.sensor_id, env.timestamp
            ));
            if let Err(e) = ws.send(Message::Text(msg.to_string())) {
                if is_timeout_or_closed(&e) {
                    self.connected = false;
                }
                break;
            }
        }

        // Collect acks, ignoring unrelated pushed messages (pong/command).
        let deadline = Instant::now() + ACK_TIMEOUT;
        while !expected.is_empty() && Instant::now() < deadline {
            let remaining = deadline - Instant::now();
            set_read_timeout(ws, Some(remaining));
            match ws.read() {
                Ok(Message::Text(t)) => {
                    if let Some(v) = parse_msg(&t) {
                        if v.get("type").and_then(|x| x.as_str()) == Some("event_ack") {
                            if let Some(id) = v.get("event_id").and_then(|x| x.as_str()) {
                                if expected.remove(id) {
                                    crate::logging::info(&format!(
                                        "T6_ACK event_id={} ack_payload={}",
                                        id, v.to_string()
                                    ));
                                    acked.push(id.to_string());
                                }
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    if is_timeout_or_closed(&e) {
                        self.connected = false;
                    }
                    break;
                }
            }
        }

        set_read_timeout(ws, None);

        if expected.is_empty() {
            self.last_ok = Instant::now();
        }

        acked
    }

    fn name(&self) -> &str {
        "wss-events"
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_transport::EventTransport;
    use crate::temporal::{EventEnvelope, EventType};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    #[ignore = "requires live WSS backend"]
    fn test_wss_roundtrip() {
        let url = std::env::var("WSS_TEST_URL")
            .unwrap_or_else(|_| "wss://detectic.24hwww.workers.dev/ws".to_string());
        let mut t = WssEventTransport::new(&url, "ex520-test-001");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let env = EventEnvelope {
            event_id: format!("test-wss-{}", now),
            sequence: 1,
            sensor_id: "ex520-test-001".to_string(),
            timestamp: now,
            event_type: EventType::DeviceConnected,
            device_id: Some("test-dev-001".to_string()),
            payload: serde_json::json!({ "rssi": -62, "band": "2.4GHz" }),
        };
        let acked = t.send_events(&[env]);
        assert!(
            !acked.is_empty(),
            "expected at least one event_ack from the WSS backend"
        );
    }
}
