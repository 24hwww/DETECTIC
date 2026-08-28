//! Temporal event engine: canonical event envelopes, hysteresis-based snapshot
//! diffing, the device connection state machine, and connection-session
//! tracking. Pure Rust, no I/O, fully unit-tested.
//!
//! Privacy contract: every event carries only the HMAC pseudonym (`device_id`);
//! raw MACs/IPs/hostnames never enter [`EventEnvelope`].

use crate::calibrate::{ProximityBucket, ProximityConfidence};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;

type HmacSha256 = Hmac<Sha256>;

fn short_digest(parts: &[&str]) -> String {
    let mut mac = HmacSha256::new_from_slice(b"detectic-event-id").expect("key");
    for p in parts {
        mac.update(p.as_bytes());
        mac.update(b"\x1f");
    }
    let out = mac.finalize().into_bytes();
    let mut s = String::with_capacity(32);
    for b in &out[..16] {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    #[serde(rename = "device.connected")]
    DeviceConnected,
    #[serde(rename = "device.disconnected")]
    DeviceDisconnected,
    #[serde(rename = "device.signal_changed")]
    DeviceSignalChanged,
    #[serde(rename = "device.band_changed")]
    DeviceBandChanged,
    #[serde(rename = "device.network_changed")]
    DeviceNetworkChanged,
    #[serde(rename = "device.presence_changed")]
    DevicePresenceChanged,
    #[serde(rename = "network.detected")]
    NetworkDetected,
    #[serde(rename = "network.disappeared")]
    NetworkDisappeared,
    #[serde(rename = "network.changed")]
    NetworkChanged,
    #[serde(rename = "rf.environment_snapshot")]
    RfEnvironmentSnapshot,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::DeviceConnected => "device.connected",
            EventType::DeviceDisconnected => "device.disconnected",
            EventType::DeviceSignalChanged => "device.signal_changed",
            EventType::DeviceBandChanged => "device.band_changed",
            EventType::DeviceNetworkChanged => "device.network_changed",
            EventType::DevicePresenceChanged => "device.presence_changed",
            EventType::NetworkDetected => "network.detected",
            EventType::NetworkDisappeared => "network.disappeared",
            EventType::NetworkChanged => "network.changed",
            EventType::RfEnvironmentSnapshot => "rf.environment_snapshot",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TemporalState {
    Unknown,
    Connected,
    SuspectedAbsence,
    Disconnected,
    RfPresent,
    Absent,
}

impl Default for TemporalState {
    fn default() -> Self {
        TemporalState::Unknown
    }
}

impl TemporalState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TemporalState::Unknown => "UNKNOWN",
            TemporalState::Connected => "CONNECTED",
            TemporalState::SuspectedAbsence => "SUSPECTED_ABSENCE",
            TemporalState::Disconnected => "DISCONNECTED",
            TemporalState::RfPresent => "RF_PRESENT",
            TemporalState::Absent => "ABSENT",
        }
    }

    fn is_associated(&self) -> bool {
        matches!(
            self,
            TemporalState::Connected | TemporalState::SuspectedAbsence
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_id: String,
    pub sequence: u64,
    pub sensor_id: String,
    pub timestamp: i64,
    #[serde(rename = "type")]
    pub event_type: EventType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct DeviceObs {
    pub identity: String,
    pub pseudonym: String,
    pub rssi: Option<i64>,
    pub noise: Option<i64>,
    pub band: Option<String>,
    pub interface: Option<String>,
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkObs {
    pub bssid_pseudonym: String,
    pub band: Option<String>,
    pub channel: Option<u8>,
    pub signal: Option<i64>,
    pub ssid: Option<String>,
    pub security: Option<String>,
    pub w_mode: Option<String>,
    pub extch: Option<String>,
}

/// RF-only probe observation from an external sensor (USB monitor adapter,
/// OpenWrt SBC, Linux laptop, etc.). The `device_id` is already a pseudonym
/// computed by the external sensor; no raw MAC ever reaches the EX520 or the
/// backend through this path.
#[derive(Debug, Clone, Default)]
pub struct ProbeObservation {
    pub device_id: String,
    pub timestamp: i64,
    pub sensor_id: String,
    pub band: Option<String>,
    pub channel: Option<u8>,
    pub frequency: Option<u32>,
    pub rssi: Option<i64>,
    pub per_chain_rssi: Vec<i64>,
    pub ssid: Option<String>,
    pub ht_vht_he: Option<String>,
    pub vendor_ies: Vec<String>,
    pub supported_rates: Vec<String>,
    pub randomized: bool,
    pub confidence: f64,
}

/// Snapshot of the RF environment at one observation time.
/// Contains aggregate statistics and the strongest APs. No raw MACs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RFEnvironmentSnapshot {
    pub timestamp: i64,
    pub ap_count: usize,
    pub ap_count_2_4: usize,
    pub ap_count_5: usize,
    pub strongest_signal: Option<i64>,
    pub weakest_signal: Option<i64>,
    pub average_signal: Option<i64>,
    pub rssi_variance: Option<f64>,
    /// Channel count distribution. Keys are channel numbers as strings;
    /// unknown-channel APs are counted under "unknown".
    pub channel_distribution: HashMap<String, u32>,
    pub top_aps: Vec<RFTopAp>,
}

/// Strongest AP in an RF environment snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RFTopAp {
    pub ap_id: String,
    pub band: Option<String>,
    pub channel: Option<u8>,
    pub signal: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct TemporalConfig {
    /// Consecutive missed polls before SUSPECTED_ABSENCE becomes DISCONNECTED.
    pub missing_polls_to_disconnect: u32,
    /// Consecutive missed polls before DISCONNECTED/RF_PRESENT become ABSENT.
    pub polls_to_absent: u32,
    /// Minimum absolute RSSI delta required to emit device.signal_changed.
    pub signal_delta_threshold: i64,
    /// Maximum devices tracked simultaneously (memory bound).
    pub max_tracked_devices: usize,
    /// Maximum site-survey networks tracked simultaneously (memory bound).
    pub max_tracked_networks: usize,
}

impl Default for TemporalConfig {
    fn default() -> Self {
        Self {
            missing_polls_to_disconnect: 2,
            polls_to_absent: 240,
            signal_delta_threshold: 5,
            max_tracked_devices: 4096,
            max_tracked_networks: 512,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectionSession {
    pub session_id: String,
    pub device_id: String,
    pub started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub band: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_signal: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_noise: Option<i64>,
}

impl ConnectionSession {
    fn open(device_id: &str, started_at: i64) -> Self {
        Self {
            session_id: short_digest(&["session", device_id, &started_at.to_string()]),
            device_id: device_id.to_string(),
            started_at,
            ended_at: None,
            duration_seconds: None,
            band: None,
            last_signal: None,
            last_noise: None,
        }
    }

    fn close(&mut self, ended_at: i64) {
        self.ended_at = Some(ended_at);
        self.duration_seconds = Some((ended_at - self.started_at).max(0));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceSummary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_seen: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_connection_started: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_connection_started: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_connection_ended: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_connection_duration: Option<i64>,
    pub total_connected_time: i64,
    pub connection_count: u64,
}

struct TrackedDevice {
    pseudonym: String,
    state: TemporalState,
    missing_polls: u32,
    last_rssi: Option<i64>,
    last_band: Option<String>,
    last_interface: Option<String>,
    hostname: Option<String>,
    current_session: Option<ConnectionSession>,
    summary: DeviceSummary,
}

impl Default for TrackedDevice {
    fn default() -> Self {
        Self {
            pseudonym: String::new(),
            state: TemporalState::Unknown,
            missing_polls: 0,
            last_rssi: None,
            last_band: None,
            last_interface: None,
            hostname: None,
            current_session: None,
            summary: DeviceSummary::default(),
        }
    }
}

#[derive(Default, Clone)]
struct TrackedNetwork {
    band: Option<String>,
    channel: Option<u8>,
    last_signal: Option<i64>,
    ssid: Option<String>,
    security: Option<String>,
    w_mode: Option<String>,
    extch: Option<String>,
    missing_polls: u32,
}

fn make_event(
    seq: &mut u64,
    sensor_id: &str,
    ts: i64,
    event_type: EventType,
    device_id: Option<String>,
    payload: serde_json::Value,
) -> EventEnvelope {
    *seq += 1;
    let dev = device_id.clone().unwrap_or_default();
    let event_id = short_digest(&[sensor_id, event_type.as_str(), &dev, &ts.to_string()]);
    EventEnvelope {
        event_id,
        sequence: *seq,
        sensor_id: sensor_id.to_string(),
        timestamp: ts,
        event_type,
        device_id,
        payload,
    }
}

pub struct TemporalEngine {
    sensor_id: String,
    config: TemporalConfig,
    seq: u64,
    devices: HashMap<String, TrackedDevice>,
    networks: HashMap<String, TrackedNetwork>,
}

impl TemporalEngine {
    pub fn new(sensor_id: &str, config: TemporalConfig) -> Self {
        Self {
            sensor_id: sensor_id.to_string(),
            config,
            seq: 0,
            devices: HashMap::new(),
            networks: HashMap::new(),
        }
    }

    /// Restore the monotonic sequence counter after a restart so sequences
    /// never regress within a sensor.
    pub fn with_sequence_start(mut self, next_seq: u64) -> Self {
        self.seq = next_seq.saturating_sub(1);
        self
    }

    pub fn next_sequence(&self) -> u64 {
        self.seq + 1
    }

    pub fn state_of(&self, identity: &str) -> TemporalState {
        self.devices
            .get(identity)
            .map(|d| d.state)
            .unwrap_or(TemporalState::Unknown)
    }

    pub fn summary_of(&self, identity: &str) -> Option<&DeviceSummary> {
        self.devices.get(identity).map(|d| &d.summary)
    }

    pub fn current_session(&self, identity: &str) -> Option<&ConnectionSession> {
        self.devices
            .get(identity)
            .and_then(|d| d.current_session.as_ref())
    }

    pub fn tracked_devices(&self) -> usize {
        self.devices.len()
    }

    pub fn tracked_networks(&self) -> usize {
        self.networks.len()
    }

    /// Process one associated-station poll. Devices absent from `observed`
    /// advance through SUSPECTED_ABSENCE -> DISCONNECTED -> ABSENT.
    pub fn process_associated(&mut self, ts: i64, observed: &[DeviceObs]) -> Vec<EventEnvelope> {
        let mut out = Vec::new();
        let mut seen: Vec<&str> = Vec::with_capacity(observed.len());

        for obs in observed {
            seen.push(obs.identity.as_str());
            if !self.devices.contains_key(&obs.identity) {
                self.devices.insert(obs.identity.clone(), TrackedDevice::default());
            }
            let prev_rssi;
            let prev_band;
            let prev_interface;
            let prev_state;
            let is_new_device;
            {
                let entry = self.devices.get_mut(&obs.identity).unwrap();
                prev_state = entry.state;
                is_new_device = entry.summary.first_seen.is_none();

                entry.pseudonym = obs.pseudonym.clone();
                entry.hostname = obs.hostname.clone().or_else(|| entry.hostname.clone());
                entry.summary.last_seen = Some(ts);
                if entry.summary.first_seen.is_none() {
                    entry.summary.first_seen = Some(ts);
                }
                entry.missing_polls = 0;

                prev_rssi = entry.last_rssi;
                prev_band = entry.last_band.clone();
                prev_interface = entry.last_interface.clone();
                entry.last_rssi = obs.rssi;
                entry.last_band = obs.band.clone();
                entry.last_interface = obs.interface.clone();
            }

            match prev_state {
                TemporalState::Connected | TemporalState::SuspectedAbsence => {
                    let entry = self.devices.get_mut(&obs.identity).unwrap();
                    entry.state = TemporalState::Connected;
                    if let Some(session) = entry.current_session.as_mut() {
                        session.last_signal = obs.rssi;
                        session.last_noise = obs.noise;
                        if session.band.is_none() {
                            session.band = obs.band.clone();
                        }
                    }
                }
                _ => {
                    let mut session = ConnectionSession::open(&obs.pseudonym, ts);
                    session.band = obs.band.clone();
                    session.last_signal = obs.rssi;
                    session.last_noise = obs.noise;
                    let sid = session.session_id.clone();
                    let count;
                    {
                        let entry = self.devices.get_mut(&obs.identity).unwrap();
                        entry.state = TemporalState::Connected;
                        entry.current_session = Some(session);
                        entry.summary.current_connection_started = Some(ts);
                        entry.summary.connection_count += 1;
                        count = entry.summary.connection_count;
                    }
                    let prox = ProximityBucket::from_rcpi(obs.rssi);
                    let prox_conf = ProximityConfidence::from_calibration(0);
                    out.push(make_event(
                        &mut self.seq,
                        &self.sensor_id,
                        ts,
                        EventType::DeviceConnected,
                        Some(obs.pseudonym.clone()),
                        serde_json::json!({
                            "session_id": sid,
                            "new_device": is_new_device,
                            "connection_count": count,
                            "rssi": obs.rssi,
                            "noise": obs.noise,
                            "band": obs.band,
                            "hostname": obs.hostname,
                            "proximity": prox.label(),
                            "proximity_confidence": prox_conf.as_str(),
                        }),
                    ));
                    continue;
                }
            }

            if let (Some(old), Some(new)) = (prev_band, &obs.band) {
                if old != *new {
                    out.push(make_event(
                        &mut self.seq,
                        &self.sensor_id,
                        ts,
                        EventType::DeviceBandChanged,
                        Some(obs.pseudonym.clone()),
                        serde_json::json!({ "old_band": old, "new_band": new }),
                    ));
                }
            }

            if let (Some(old), Some(new)) = (prev_interface, &obs.interface) {
                if old != *new {
                    out.push(make_event(
                        &mut self.seq,
                        &self.sensor_id,
                        ts,
                        EventType::DeviceNetworkChanged,
                        Some(obs.pseudonym.clone()),
                        serde_json::json!({ "old_interface": old, "new_interface": new }),
                    ));
                }
            }

            if let (Some(old), Some(new)) = (prev_rssi, obs.rssi) {
                if (old - new).abs() >= self.config.signal_delta_threshold {
                    let prox = ProximityBucket::from_rcpi(Some(new));
                    let prox_conf = ProximityConfidence::from_calibration(0);
                    out.push(make_event(
                        &mut self.seq,
                        &self.sensor_id,
                        ts,
                        EventType::DeviceSignalChanged,
                        Some(obs.pseudonym.clone()),
                        serde_json::json!({
                            "old_signal": old,
                            "new_signal": new,
                            "band": obs.band,
                            "hostname": obs.hostname,
                            "proximity": prox.label(),
                            "proximity_confidence": prox_conf.as_str(),
                        }),
                    ));
                }
            }
        }

        let missing_threshold = self.config.missing_polls_to_disconnect;
        let absent_threshold = self.config.polls_to_absent;
        let identities: Vec<String> = self.devices.keys().cloned().collect();
        for identity in identities {
            if seen.contains(&identity.as_str()) {
                continue;
            }
            let entry = self.devices.get_mut(&identity).unwrap();
            if entry.state == TemporalState::Absent {
                continue;
            }
            entry.missing_polls = entry.missing_polls.saturating_add(1);

            match entry.state {
                TemporalState::Connected => {
                    if entry.missing_polls >= missing_threshold {
                        // Threshold already met on first miss (e.g. threshold=1):
                        // go straight to Disconnected.
                        entry.state = TemporalState::Disconnected;
                        if let Some(mut session) = entry.current_session.take() {
                            session.close(ts);
                            session.band = session.band.or_else(|| entry.last_band.clone());
                            entry.summary.current_connection_started = None;
                            entry.summary.last_connection_started = Some(session.started_at);
                            entry.summary.last_connection_ended = session.ended_at;
                            entry.summary.last_connection_duration = session.duration_seconds;
                            if let Some(dur) = session.duration_seconds {
                                entry.summary.total_connected_time += dur;
                            }
                            out.push(make_event(
                                &mut self.seq,
                                &self.sensor_id,
                                ts,
                                EventType::DeviceDisconnected,
                                Some(session.device_id.clone()),
                                serde_json::json!({
                                    "session_id": session.session_id,
                                    "started_at": session.started_at,
                                    "ended_at": session.ended_at,
                                    "duration_seconds": session.duration_seconds,
                                    "last_signal": session.last_signal,
                                    "last_noise": session.last_noise,
                                    "band": session.band,
                                    "hostname": entry.hostname,
                                    "missing_polls": entry.missing_polls,
                                }),
                            ));
                        }
                    } else {
                        entry.state = TemporalState::SuspectedAbsence;
                    }
                }
                TemporalState::SuspectedAbsence => {
                    if entry.missing_polls >= missing_threshold {
                        entry.state = TemporalState::Disconnected;
                        if let Some(mut session) = entry.current_session.take() {
                            session.close(ts);
                            session.band = session.band.or_else(|| entry.last_band.clone());
                            entry.summary.current_connection_started = None;
                            entry.summary.last_connection_started = Some(session.started_at);
                            entry.summary.last_connection_ended = session.ended_at;
                            entry.summary.last_connection_duration = session.duration_seconds;
                            if let Some(dur) = session.duration_seconds {
                                entry.summary.total_connected_time += dur;
                            }
                            out.push(make_event(
                                &mut self.seq,
                                &self.sensor_id,
                                ts,
                                EventType::DeviceDisconnected,
                                Some(session.device_id.clone()),
                                serde_json::json!({
                                    "session_id": session.session_id,
                                    "started_at": session.started_at,
                                    "ended_at": session.ended_at,
                                    "duration_seconds": session.duration_seconds,
                                    "last_signal": session.last_signal,
                                    "last_noise": session.last_noise,
                                    "band": session.band,
                                    "hostname": entry.hostname,
                                    "missing_polls": entry.missing_polls,
                                }),
                            ));
                        }
                    }
                }
                TemporalState::Disconnected | TemporalState::RfPresent => {
                    if entry.missing_polls >= absent_threshold {
                        let from = entry.state;
                        let pseudo = entry.pseudonym.clone();
                        entry.state = TemporalState::Absent;
                        out.push(make_event(
                            &mut self.seq,
                            &self.sensor_id,
                            ts,
                            EventType::DevicePresenceChanged,
                            Some(pseudo),
                            serde_json::json!({
                                "from_state": from.as_str(),
                                "to_state": TemporalState::Absent.as_str(),
                            }),
                        ));
                    }
                }
                _ => {}
            }
        }

        self.enforce_device_bound();
        out.sort_by_key(|e| e.sequence);
        out
    }

    /// Process RF-only evidence (site survey / probes) for unassociated
    /// devices. Moves DISCONNECTED/ABSENT/UNKNOWN devices into RF_PRESENT.
    pub fn process_rf_evidence(&mut self, ts: i64, observed: &[DeviceObs]) -> Vec<EventEnvelope> {
        let mut out = Vec::new();
        for obs in observed {
            if !self.devices.contains_key(&obs.identity) {
                self.devices
                    .insert(obs.identity.clone(), TrackedDevice::default());
            }
            let entry = self.devices.get_mut(&obs.identity).unwrap();
            if entry.state.is_associated() || entry.state == TemporalState::RfPresent {
                entry.summary.last_seen = Some(ts);
                continue;
            }
            entry.pseudonym = obs.pseudonym.clone();
            entry.hostname = obs.hostname.clone().or_else(|| entry.hostname.clone());
            entry.state = TemporalState::RfPresent;
            entry.missing_polls = 0;
            entry.summary.last_seen = Some(ts);
            if entry.summary.first_seen.is_none() {
                entry.summary.first_seen = Some(ts);
            }
            out.push(make_event(
                &mut self.seq,
                &self.sensor_id,
                ts,
                EventType::DevicePresenceChanged,
                Some(obs.pseudonym.clone()),
                serde_json::json!({
                    "to_state": TemporalState::RfPresent.as_str(),
                    "rssi": obs.rssi,
                    "band": obs.band,
                }),
            ));
        }

        let absent_threshold = self.config.polls_to_absent;
        let rf_ids: Vec<String> = self
            .devices
            .iter()
            .filter(|(_, d)| d.state == TemporalState::RfPresent)
            .map(|(k, _)| k.clone())
            .collect();
        for identity in rf_ids {
            if observed.iter().any(|o| o.identity == identity) {
                continue;
            }
            let entry = self.devices.get_mut(&identity).unwrap();
            entry.missing_polls = entry.missing_polls.saturating_add(1);
            if entry.missing_polls >= absent_threshold {
                let pseudo = entry.pseudonym.clone();
                entry.state = TemporalState::Absent;
                out.push(make_event(
                    &mut self.seq,
                    &self.sensor_id,
                    ts,
                    EventType::DevicePresenceChanged,
                    Some(pseudo),
                    serde_json::json!({
                        "from_state": TemporalState::RfPresent.as_str(),
                        "to_state": TemporalState::Absent.as_str(),
                    }),
                ));
            }
        }

        self.enforce_device_bound();
        out.sort_by_key(|e| e.sequence);
        out
    }

    /// Convenience wrapper for `process_rf_evidence` that accepts
    /// `ProbeObservation`s from an external RF sensor. The `device_id` is
    /// already an HMAC pseudonym produced by the external sensor.
    pub fn process_probes(&mut self, ts: i64, probes: &[ProbeObservation]) -> Vec<EventEnvelope> {
        let obs: Vec<DeviceObs> = probes
            .iter()
            .map(|p| DeviceObs {
                identity: p.device_id.clone(),
                pseudonym: p.device_id.clone(),
                rssi: p.rssi,
                noise: None,
                band: p.band.clone(),
                interface: None,
                hostname: None,
            })
            .collect();
        self.process_rf_evidence(ts, &obs)
    }

    /// Process site-survey observations: network.detected / changed /
    /// disappeared with signal hysteresis.
    pub fn process_networks(&mut self, ts: i64, observed: &[NetworkObs]) -> Vec<EventEnvelope> {
        let mut out = Vec::new();
        let mut seen: Vec<&str> = Vec::with_capacity(observed.len());
        let delta = self.config.signal_delta_threshold;

        for n in observed {
            seen.push(n.bssid_pseudonym.as_str());
            let is_new = !self.networks.contains_key(&n.bssid_pseudonym);
            let entry = self
                .networks
                .entry(n.bssid_pseudonym.clone())
                .or_default();
            let prev_band = entry.band.clone();
            let prev_channel = entry.channel;
            let prev_signal = entry.last_signal;
            let prev_ssid = entry.ssid.clone();
            let prev_security = entry.security.clone();
            let prev_w_mode = entry.w_mode.clone();
            let prev_extch = entry.extch.clone();

            if is_new {
                out.push(make_event(
                    &mut self.seq,
                    &self.sensor_id,
                    ts,
                    EventType::NetworkDetected,
                    Some(n.bssid_pseudonym.clone()),
                    serde_json::json!({
                        "band": n.band,
                        "channel": n.channel,
                        "signal": n.signal,
                        "ssid": n.ssid,
                        "security": n.security,
                        "w_mode": n.w_mode,
                        "extch": n.extch,
                    }),
                ));
            } else {
                let mut changed = serde_json::Map::new();
                if prev_channel != n.channel {
                    changed.insert(
                        "channel".into(),
                        serde_json::json!({ "old": prev_channel, "new": n.channel }),
                    );
                }
                if prev_band != n.band && !(prev_band.is_none() && n.band.is_none()) {
                    changed.insert(
                        "band".into(),
                        serde_json::json!({ "old": prev_band, "new": n.band }),
                    );
                }
                if let (Some(old), Some(new)) = (prev_signal, n.signal) {
                    if (old - new).abs() >= delta {
                        changed.insert(
                            "signal".into(),
                            serde_json::json!({ "old": old, "new": new }),
                        );
                    }
                }
                if prev_ssid != n.ssid && !(prev_ssid.is_none() && n.ssid.is_none()) {
                    changed.insert(
                        "ssid".into(),
                        serde_json::json!({ "old": prev_ssid, "new": n.ssid }),
                    );
                }
                if prev_security != n.security && !(prev_security.is_none() && n.security.is_none()) {
                    changed.insert(
                        "security".into(),
                        serde_json::json!({ "old": prev_security, "new": n.security }),
                    );
                }
                if prev_w_mode != n.w_mode && !(prev_w_mode.is_none() && n.w_mode.is_none()) {
                    changed.insert(
                        "w_mode".into(),
                        serde_json::json!({ "old": prev_w_mode, "new": n.w_mode }),
                    );
                }
                if prev_extch != n.extch && !(prev_extch.is_none() && n.extch.is_none()) {
                    changed.insert(
                        "extch".into(),
                        serde_json::json!({ "old": prev_extch, "new": n.extch }),
                    );
                }
                if !changed.is_empty() {
                    out.push(make_event(
                        &mut self.seq,
                        &self.sensor_id,
                        ts,
                        EventType::NetworkChanged,
                        Some(n.bssid_pseudonym.clone()),
                        serde_json::Value::Object(changed),
                    ));
                }
            }

            entry.band = n.band.clone();
            entry.channel = n.channel;
            entry.last_signal = n.signal;
            entry.ssid = n.ssid.clone();
            entry.security = n.security.clone();
            entry.w_mode = n.w_mode.clone();
            entry.extch = n.extch.clone();
            entry.missing_polls = 0;
        }

        let gone_threshold = self.config.missing_polls_to_disconnect.max(2);
        let ids: Vec<String> = self.networks.keys().cloned().collect();
        for bssid in ids {
            if seen.contains(&bssid.as_str()) {
                continue;
            }
            let entry = self.networks.get_mut(&bssid).unwrap();
            entry.missing_polls = entry.missing_polls.saturating_add(1);
            if entry.missing_polls == gone_threshold {
                out.push(make_event(
                    &mut self.seq,
                    &self.sensor_id,
                    ts,
                    EventType::NetworkDisappeared,
                    Some(bssid),
                    serde_json::json!({ "band": entry.band, "channel": entry.channel }),
                ));
            }
        }

        self.enforce_network_bound();
        out.sort_by_key(|e| e.sequence);
        out
    }

    /// Build an `RFEnvironmentSnapshot` from the currently tracked networks and
    /// emit a `rf.environment_snapshot` event. Returns `None` when no network
    /// has been observed yet.
    pub fn rf_environment_snapshot(&mut self, ts: i64) -> Option<EventEnvelope> {
        if self.networks.is_empty() {
            return None;
        }

        let mut signals = Vec::with_capacity(self.networks.len());
        let mut ap_count_2_4 = 0usize;
        let mut ap_count_5 = 0usize;
        let mut channel_dist: HashMap<String, u32> = HashMap::new();
        let mut aps: Vec<(&str, &TrackedNetwork)> = self
            .networks
            .iter()
            .map(|(id, e)| (id.as_str(), e))
            .collect();

        for (_, e) in &aps {
            if let Some(s) = e.last_signal {
                signals.push(s);
            }
            match e.band.as_deref() {
                Some("2.4GHz") => ap_count_2_4 += 1,
                Some("5GHz") => ap_count_5 += 1,
                _ => {}
            }
            let key = e.channel.map(|c| c.to_string()).unwrap_or_else(|| "unknown".into());
            *channel_dist.entry(key).or_insert(0) += 1;
        }

        aps.sort_by(|a, b| {
            // Descending by signal, then by AP id for determinism.
            b.1.last_signal.cmp(&a.1.last_signal).then_with(|| a.0.cmp(b.0))
        });

        let strongest = signals.iter().copied().max();
        let weakest = signals.iter().copied().min();
        let average = if signals.is_empty() {
            None
        } else {
            Some(signals.iter().sum::<i64>() / signals.len() as i64)
        };
        let variance = if signals.len() >= 2 {
            let mean = signals.iter().sum::<i64>() as f64 / signals.len() as f64;
            Some(signals.iter().map(|&s| (s as f64 - mean).powi(2)).sum::<f64>() / signals.len() as f64)
        } else {
            None
        };

        let top = aps
            .into_iter()
            .take(10)
            .map(|(id, e)| RFTopAp {
                ap_id: id.to_string(),
                band: e.band.clone(),
                channel: e.channel,
                signal: e.last_signal,
            })
            .collect();

        let snap = RFEnvironmentSnapshot {
            timestamp: ts,
            ap_count: self.networks.len(),
            ap_count_2_4,
            ap_count_5,
            strongest_signal: strongest,
            weakest_signal: weakest,
            average_signal: average,
            rssi_variance: variance,
            channel_distribution: channel_dist,
            top_aps: top,
        };

        Some(make_event(
            &mut self.seq,
            &self.sensor_id,
            ts,
            EventType::RfEnvironmentSnapshot,
            None,
            serde_json::to_value(&snap).unwrap_or(serde_json::Value::Null),
        ))
    }

    fn enforce_device_bound(&mut self) {
        if self.devices.len() <= self.config.max_tracked_devices {
            return;
        }
        // Evict oldest absent devices first. Tie-break deterministically by
        // first_seen and then pseudonym so tests are stable across process
        // HashMap seeds. The identity is the final tie-breaker.
        let mut candidates: Vec<(String, i64, i64, String)> = self
            .devices
            .iter()
            .filter(|(_, d)| d.state == TemporalState::Absent)
            .map(|(k, d)| {
                (
                    k.clone(),
                    d.summary.last_seen.unwrap_or(0),
                    d.summary.first_seen.unwrap_or(0),
                    d.pseudonym.clone(),
                )
            })
            .collect();
        candidates.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| a.2.cmp(&b.2))
                .then_with(|| a.3.cmp(&b.3))
                .then_with(|| a.0.cmp(&b.0))
        });
        let excess = self.devices.len() - self.config.max_tracked_devices;
        for (id, _, _, _) in candidates.into_iter().take(excess) {
            self.devices.remove(&id);
        }
    }

    fn enforce_network_bound(&mut self) {
        if self.networks.len() <= self.config.max_tracked_networks {
            return;
        }
        let excess = self.networks.len() - self.config.max_tracked_networks;
        let ids: Vec<String> = self.networks.keys().take(excess).cloned().collect();
        for id in ids {
            self.networks.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(id: &str, rssi: Option<i64>) -> DeviceObs {
        use sha2::Digest;
        let digest = Sha256::digest(id.as_bytes());
        let mut pseudo = String::from("h");
        for b in digest.iter().take(8) {
            pseudo.push_str(&format!("{b:02x}"));
        }
        DeviceObs {
            identity: id.to_string(),
            pseudonym: pseudo,
            rssi,
            noise: None,
            band: Some("2.4GHz".into()),
            interface: Some("rai0".into()),
            hostname: None,
        }
    }

    fn engine() -> TemporalEngine {
        TemporalEngine::new(
            "ex520-001",
            TemporalConfig {
                missing_polls_to_disconnect: 2,
                polls_to_absent: 4,
                ..Default::default()
            },
        )
    }

    fn kinds(events: &[EventEnvelope]) -> Vec<EventType> {
        events.iter().map(|e| e.event_type).collect()
    }

    #[test]
    fn envelope_has_all_required_fields() {
        let mut e = engine();
        let events = e.process_associated(1000, &[obs("AA", Some(-50))]);
        assert_eq!(events.len(), 1);
        let env = &events[0];
        assert_eq!(env.sensor_id, "ex520-001");
        assert_eq!(env.sequence, 1);
        assert_eq!(env.timestamp, 1000);
        assert_eq!(env.event_type, EventType::DeviceConnected);
        assert!(env
            .device_id
            .as_deref()
            .map(|d| d.starts_with('h') && !d.contains("AA"))
            .unwrap_or(false));
        assert!(env.payload.is_object());
        assert_eq!(env.event_id.len(), 32);

        let json = serde_json::to_value(env).unwrap();
        for key in [
            "event_id",
            "sequence",
            "sensor_id",
            "timestamp",
            "type",
            "payload",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
        assert_eq!(json["type"], "device.connected");
    }

    #[test]
    fn event_id_is_deterministic_across_engines() {
        let mut a = engine();
        let mut b = engine();
        let ea = a.process_associated(1000, &[obs("AA", Some(-50))]);
        let eb = b.process_associated(1000, &[obs("AA", Some(-50))]);
        assert_eq!(ea[0].event_id, eb[0].event_id);

        let ec = a.process_associated(2000, &[obs("BB", Some(-50))]);
        assert_eq!(ec.len(), 1);
        assert_ne!(ea[0].event_id, ec[0].event_id);
    }

    // --- STEP 2 tests: active filtering in temporal engine ---

    fn engine_threshold_1() -> TemporalEngine {
        TemporalEngine::new(
            "ex520-001",
            TemporalConfig {
                missing_polls_to_disconnect: 1,
                polls_to_absent: 4,
                ..Default::default()
            },
        )
    }

    /// Simulates the EX520 behavior: device stays in the table with active=0.
    /// Currently service.rs builds device_obs from ALL stations (no active filter),
    /// so the temporal engine sees the device as "observed" even when active=0.
    /// This test documents that the temporal engine itself works correctly
    /// when the device is removed from the observed list (which is what
    /// the fix in service.rs will do).
    #[test]
    fn step2_temporal_active_1_to_0_with_filtering() {
        let mut e = engine_threshold_1();

        // Poll 1: device active=1 → in observed list
        let e1 = e.process_associated(1000, &[obs("AA", Some(-50))]);
        assert_eq!(kinds(&e1), vec![EventType::DeviceConnected]);
        assert_eq!(e.state_of("AA"), TemporalState::Connected);

        // Poll 2: device active=0 → NOT in observed list (after fix)
        let e2 = e.process_associated(1001, &[]);
        // With threshold=1, first miss should disconnect immediately
        assert_eq!(
            kinds(&e2),
            vec![EventType::DeviceDisconnected],
            "active=1→0 should produce DeviceDisconnected"
        );
        assert_eq!(e.state_of("AA"), TemporalState::Disconnected);
    }

    #[test]
    fn step2_temporal_active_0_to_1_reconnect() {
        let mut e = engine_threshold_1();

        // Device connects
        e.process_associated(1000, &[obs("AA", Some(-50))]);
        // Device disconnects (active=0, filtered out)
        e.process_associated(1001, &[]);
        assert_eq!(e.state_of("AA"), TemporalState::Disconnected);

        // Device reconnects (active=1, back in observed list)
        let e3 = e.process_associated(1002, &[obs("AA", Some(-55))]);
        assert_eq!(
            kinds(&e3),
            vec![EventType::DeviceConnected],
            "active=0→1 should produce DeviceConnected"
        );
    }

    #[test]
    fn step2_temporal_active_0_stays_disconnected() {
        let mut e = engine_threshold_1();

        // Device connects then disconnects
        e.process_associated(1000, &[obs("AA", Some(-50))]);
        e.process_associated(1001, &[]);
        assert_eq!(e.state_of("AA"), TemporalState::Disconnected);

        // Still absent (active=0) → no new events
        let e3 = e.process_associated(1002, &[]);
        assert!(e3.is_empty(), "active=0→0 should produce 0 events");
        assert_eq!(e.state_of("AA"), TemporalState::Disconnected);
    }

    /// STEP 4: Verify device identity is preserved across active=0/1 transitions.
    /// The same device (identity "AA") must be treated as ONE device across:
    ///   Snapshot 1: active=1 (connected)
    ///   Snapshot 2: active=0 (disconnected)
    ///   Snapshot 3: active=1 (reconnected)
    #[test]
    fn step4_identity_preserved_across_active_transitions() {
        let mut e = engine_threshold_1();

        // Snapshot 1: active=1 → DeviceConnected
        let e1 = e.process_associated(1000, &[obs("AA", Some(-50))]);
        assert_eq!(kinds(&e1), vec![EventType::DeviceConnected]);
        let pseudo_1 = e1[0].device_id.clone().unwrap();

        // Snapshot 2: active=0 → DeviceDisconnected (same device_id)
        let e2 = e.process_associated(1001, &[]);
        assert_eq!(kinds(&e2), vec![EventType::DeviceDisconnected]);
        let pseudo_2 = e2[0].device_id.clone().unwrap();
        assert_eq!(
            pseudo_1, pseudo_2,
            "device_id must be the same across active=1→0"
        );

        // Snapshot 3: active=1 → DeviceConnected (same device_id)
        let e3 = e.process_associated(1002, &[obs("AA", Some(-55))]);
        assert_eq!(kinds(&e3), vec![EventType::DeviceConnected]);
        let pseudo_3 = e3[0].device_id.clone().unwrap();
        assert_eq!(
            pseudo_2, pseudo_3,
            "device_id must be the same across active=0→1"
        );

        // Connection count should be 2 (two separate connection sessions)
        let summary = e.summary_of("AA").unwrap();
        assert_eq!(summary.connection_count, 2);
    }

    #[test]
    fn connect_disconnect_reconnect_full_cycle() {
        let mut e = engine();

        let e1 = e.process_associated(1000, &[obs("AA", Some(-50))]);
        assert_eq!(kinds(&e1), vec![EventType::DeviceConnected]);
        assert_eq!(e.state_of("AA"), TemporalState::Connected);

        let e2 = e.process_associated(1030, &[]);
        assert!(e2.is_empty(), "first miss only suspects");
        assert_eq!(e.state_of("AA"), TemporalState::SuspectedAbsence);

        let e3 = e.process_associated(1060, &[]);
        assert_eq!(kinds(&e3), vec![EventType::DeviceDisconnected]);
        assert_eq!(e.state_of("AA"), TemporalState::Disconnected);
        let disc = &e3[0];
        assert_eq!(disc.payload["started_at"], 1000);
        assert_eq!(disc.payload["ended_at"], 1060);
        assert_eq!(disc.payload["duration_seconds"], 60);
        assert_eq!(disc.payload["missing_polls"], 2);

        let e4 = e.process_associated(1090, &[obs("AA", Some(-55))]);
        assert_eq!(kinds(&e4), vec![EventType::DeviceConnected]);

        let summary = e.summary_of("AA").unwrap();
        assert_eq!(summary.connection_count, 2);
        assert_eq!(summary.total_connected_time, 60);
        assert_eq!(summary.last_connection_duration, Some(60));
        assert_eq!(summary.first_seen, Some(1000));
        assert_eq!(summary.last_seen, Some(1090));
        assert_eq!(summary.current_connection_started, Some(1090));
    }

    #[test]
    fn absent_after_sustained_missing_polls() {
        let mut e = engine();
        e.process_associated(1000, &[obs("AA", Some(-50))]);
        e.process_associated(1030, &[]);
        e.process_associated(1060, &[]);
        assert_eq!(e.state_of("AA"), TemporalState::Disconnected);
        let _ = e.process_associated(1090, &[]);
        let absent_events = e.process_associated(1120, &[]);
        assert_eq!(e.state_of("AA"), TemporalState::Absent);
        assert_eq!(
            kinds(&absent_events),
            vec![EventType::DevicePresenceChanged]
        );
        assert_eq!(absent_events[0].payload["from_state"], "DISCONNECTED");
        assert_eq!(absent_events[0].payload["to_state"], "ABSENT");

        let later = e.process_associated(1150, &[]);
        assert!(later.is_empty(), "absent is terminal until re-observation");
    }

    #[test]
    fn rf_evidence_moves_disconnected_to_rf_present_then_rejoin() {
        let mut e = engine();
        e.process_associated(1000, &[obs("AA", Some(-50))]);
        e.process_associated(1030, &[]);
        e.process_associated(1060, &[]);
        assert_eq!(e.state_of("AA"), TemporalState::Disconnected);

        let events = e.process_rf_evidence(1070, &[obs("AA", Some(-75))]);
        assert_eq!(kinds(&events), vec![EventType::DevicePresenceChanged]);
        assert_eq!(events[0].payload["to_state"], "RF_PRESENT");
        assert!(events[0].device_id.is_some());
        assert_eq!(e.state_of("AA"), TemporalState::RfPresent);

        let rejoin = e.process_associated(1080, &[obs("AA", Some(-48))]);
        assert_eq!(kinds(&rejoin), vec![EventType::DeviceConnected]);
        assert_eq!(e.state_of("AA"), TemporalState::Connected);
    }

    #[test]
    fn rf_ignored_for_connected_devices() {
        let mut e = engine();
        e.process_associated(1000, &[obs("AA", Some(-50))]);
        let events = e.process_rf_evidence(1010, &[obs("AA", Some(-70))]);
        assert!(events.is_empty());
        assert_eq!(e.state_of("AA"), TemporalState::Connected);
    }

    #[test]
    fn small_signal_changes_do_not_emit() {
        let mut e = engine();
        e.process_associated(1000, &[obs("AA", Some(-50))]);
        let events = e.process_associated(1030, &[obs("AA", Some(-52))]);
        assert!(events.is_empty(), "delta 2 < threshold 5");
        let events = e.process_associated(1060, &[obs("AA", Some(-58))]);
        assert_eq!(kinds(&events), vec![EventType::DeviceSignalChanged]);
        assert_eq!(events[0].payload["old_signal"], -52);
        assert_eq!(events[0].payload["new_signal"], -58);
    }

    #[test]
    fn rcpi_flapping_does_not_produce_event_storm() {
        let mut e = engine();
        e.process_associated(1000, &[obs("AA", Some(88))]);
        for (i, rcpi) in [87i64, 89, 86, 88, 87].iter().enumerate() {
            let events = e.process_associated(1030 + (i as i64) * 30, &[obs("AA", Some(*rcpi))]);
            assert!(events.is_empty(), "rcpi {rcpi} must not emit");
        }
    }

    #[test]
    fn band_change_emits_once_per_actual_change() {
        let mut e = engine();
        e.process_associated(1000, &[obs("AA", Some(-50))]);

        let mut five_g = obs("AA", Some(-52));
        five_g.band = Some("5GHz".into());
        let events = e.process_associated(1030, &[five_g]);
        assert_eq!(kinds(&events), vec![EventType::DeviceBandChanged]);
        assert_eq!(events[0].payload["old_band"], "2.4GHz");
        assert_eq!(events[0].payload["new_band"], "5GHz");

        let again = e.process_associated(1060, &[{
            let mut o = obs("AA", Some(-52));
            o.band = Some("5GHz".into());
            o
        }]);
        assert!(again.is_empty());
    }

    #[test]
    fn interface_roaming_emits_network_changed() {
        let mut e = engine();
        e.process_associated(1000, &[obs("AA", Some(-50))]);
        let mut moved = obs("AA", Some(-50));
        moved.interface = Some("rax0".into());
        let events = e.process_associated(1030, &[moved]);
        assert_eq!(kinds(&events), vec![EventType::DeviceNetworkChanged]);
        assert_eq!(events[0].payload["old_interface"], "rai0");
        assert_eq!(events[0].payload["new_interface"], "rax0");
    }

    #[test]
    fn sequence_monotonic_and_restart_restorable() {
        let mut e = engine();
        let mut last = 0u64;
        for i in 0..5 {
            let events = e.process_associated(1000 + i * 30, &[obs("AA", Some(-50))]);
            for ev in events {
                assert!(ev.sequence > last);
                last = ev.sequence;
            }
        }
        let restored = engine().with_sequence_start(last + 1);
        assert_eq!(restored.next_sequence(), last + 1);
    }

    #[test]
    fn new_device_flag_only_first_connect() {
        let mut e = engine();
        let e1 = e.process_associated(1000, &[obs("AA", Some(-50))]);
        assert_eq!(e1[0].payload["new_device"], true);
        e.process_associated(1030, &[]);
        e.process_associated(1060, &[]);
        let e2 = e.process_associated(1090, &[obs("AA", Some(-50))]);
        assert_eq!(e2[0].payload["new_device"], false);
        assert_eq!(e2[0].payload["connection_count"], 2);
    }

    #[test]
    fn session_ids_unique_per_connection() {
        let mut e = engine();
        e.process_associated(1000, &[obs("AA", Some(-50))]);
        let s1 = e.current_session("AA").unwrap().session_id.clone();
        e.process_associated(1030, &[]);
        e.process_associated(1060, &[]);
        e.process_associated(1090, &[obs("AA", Some(-50))]);
        let s2 = e.current_session("AA").unwrap().session_id.clone();
        assert_ne!(s1, s2);
    }

    #[test]
    fn no_raw_identity_in_any_event() {
        let mut e = engine();
        let mut all = Vec::new();
        all.extend(e.process_associated(
            1000,
            &[obs("11:22:33:44:55:66", Some(-50))],
        ));
        all.extend(e.process_associated(1030, &[]));
        all.extend(e.process_associated(1060, &[]));
        all.extend(e.process_rf_evidence(1070, &[obs("aa:bb:cc:dd:ee:ff", Some(-80))]));
        let ser = serde_json::to_string(&all).unwrap();
        assert!(!ser.contains("11:22:33:44:55:66"));
        assert!(!ser.contains("aa:bb:cc:dd:ee:ff"));
    }

    #[test]
    fn network_detected_changed_disappeared() {
        let mut e = engine();
        let net = |sig: i64, ch: u8| NetworkObs {
            bssid_pseudonym: "net1".into(),
            band: Some("2.4GHz".into()),
            channel: Some(ch),
            signal: Some(sig),
            ..Default::default()
        };
        let e1 = e.process_networks(1000, &[net(-60, 6)]);
        assert_eq!(kinds(&e1), vec![EventType::NetworkDetected]);

        let e2 = e.process_networks(1030, &[net(-62, 6)]);
        assert!(e2.is_empty(), "within hysteresis");

        let e3 = e.process_networks(1060, &[net(-75, 11)]);
        assert_eq!(kinds(&e3), vec![EventType::NetworkChanged]);

        let e4 = e.process_networks(1090, &[]);
        assert!(e4.is_empty(), "first miss");

        let e5 = e.process_networks(1120, &[]);
        assert_eq!(kinds(&e5), vec![EventType::NetworkDisappeared]);
    }

    #[test]
    fn rf_environment_snapshot_summarizes_networks() {
        let mut e = engine();
        let two = NetworkObs {
            bssid_pseudonym: "net1".into(),
            band: Some("2.4GHz".into()),
            channel: Some(6),
            signal: Some(-60),
            ..Default::default()
        };
        let five = NetworkObs {
            bssid_pseudonym: "net2".into(),
            band: Some("5GHz".into()),
            channel: Some(40),
            signal: Some(-40),
            ..Default::default()
        };
        e.process_networks(1000, &[two, five]);

        let env = e.rf_environment_snapshot(1000).unwrap();
        assert_eq!(env.event_type, EventType::RfEnvironmentSnapshot);
        let snap: RFEnvironmentSnapshot = serde_json::from_value(env.payload).unwrap();
        assert_eq!(snap.ap_count, 2);
        assert_eq!(snap.ap_count_2_4, 1);
        assert_eq!(snap.ap_count_5, 1);
        assert_eq!(snap.strongest_signal, Some(-40));
        assert_eq!(snap.weakest_signal, Some(-60));
        assert_eq!(snap.average_signal, Some(-50));
        assert_eq!(snap.channel_distribution.get("6").copied(), Some(1));
        assert_eq!(snap.channel_distribution.get("40").copied(), Some(1));
        assert_eq!(snap.top_aps.len(), 2);
        assert_eq!(snap.top_aps[0].ap_id, "net2");
    }

    #[test]
    fn process_probes_moves_unknown_device_to_rf_present() {
        let mut e = engine();
        let probe = ProbeObservation {
            device_id: "PROBE-AA".into(),
            timestamp: 1000,
            sensor_id: "ext1".into(),
            band: Some("2.4GHz".into()),
            rssi: Some(-70),
            randomized: true,
            confidence: 0.75,
            ..Default::default()
        };
        let events = e.process_probes(1000, &[probe]);
        assert_eq!(kinds(&events), vec![EventType::DevicePresenceChanged]);
        assert_eq!(events[0].payload["to_state"], "RF_PRESENT");
        assert_eq!(events[0].device_id, Some("PROBE-AA".into()));
    }

    #[test]
    fn device_bound_evicts_oldest_absent() {
        let cfg = TemporalConfig {
            max_tracked_devices: 2,
            missing_polls_to_disconnect: 1,
            polls_to_absent: 1,
            ..Default::default()
        };
        let mut e = TemporalEngine::new("s", cfg);
        // A appears first and goes absent; B and C appear later. With max=2,
        // the oldest absent (A) is evicted so B and C remain tracked.
        // Need 3 missing polls to reach Absent (Connected -> Suspected ->
        // Disconnected -> Absent).
        let ids = ["A", "B", "C"];
        for (i, id) in ids.iter().enumerate() {
            let t = 1000 + i as i64 * 100;
            e.process_associated(t, &[obs(id, None)]);
            e.process_associated(t + 5, &[]);
            e.process_associated(t + 10, &[]);
            e.process_associated(t + 15, &[]);
        }
        assert_eq!(e.tracked_devices(), 2);
        assert_eq!(e.state_of("A"), TemporalState::Unknown);
        assert_eq!(e.state_of("B"), TemporalState::Absent);
        assert_eq!(e.state_of("C"), TemporalState::Absent);
        assert_eq!(e.tracked_networks(), 0);
    }

    #[test]
    fn two_devices_tracked_independently() {
        let mut e = engine();
        e.process_associated(1000, &[obs("AA", Some(-50)), obs("BB", Some(-60))]);
        let ev = e.process_associated(1030, &[obs("AA", Some(-50))]);
        assert!(ev.is_empty(), "BB miss 1 suspects silently");
        assert_eq!(e.state_of("BB"), TemporalState::SuspectedAbsence);
        assert_eq!(e.state_of("AA"), TemporalState::Connected);
        let ev = e.process_associated(1060, &[obs("AA", Some(-50))]);
        assert_eq!(kinds(&ev), vec![EventType::DeviceDisconnected]);
    }
}
