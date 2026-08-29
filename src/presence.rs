//! Presence engine (M6).
//!
//! The `PresenceEngine` receives successive `SensorSnapshot`s and maintains a
//! debounced view of device presence and proximity. It does NOT replace the
//! raw `Device` data; instead, it enriches each observed device with:
//!
//! - `PresenceState`: Present / Away / Unknown / Weakening / Approaching / Departing
//! - `ProximityResult`: zone, trend, heat, distance and confidence
//! - `consecutive_seen` and `consecutive_missing` counters
//! - `first_seen` and `last_seen` epoch timestamps
//!
//! LEAVE detection uses hysteresis: a device is only considered `Away` after
//! `missing_polls_before_leave` consecutive polls without observation.
//!
//! Proximity is computed by the dedicated `ProximityEngine` which converts the
//! MediaTek RCPI scale to dBm, applies EMA/median smoothing, and detects
//! approach/away trends.

use crate::calibrate::Band;
use crate::model::Device;
use crate::proximity::{ProximityConfig, ProximityEngine, ProximityResult, SignalType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Presence state of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresenceState {
    Present,
    Away,
    Unknown,
    /// Signal weakening over recent observations — may be departing.
    Weakening,
    /// Signal strengthening over recent observations — may be approaching.
    Approaching,
    /// Was present, now absent (transition state).
    Departing,
}

impl Default for PresenceState {
    fn default() -> Self {
        PresenceState::Unknown
    }
}

/// Single presence observation for a device at a given timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceObservation {
    /// Stable device identity (MAC or fallback).
    pub identity: String,
    /// Raw or smoothed RSSI (dBm, or firmware scale if no conversion).
    pub rssi: Option<i64>,
    /// Smoothed RSSI used for classification.
    pub rssi_smoothed: Option<f64>,
    /// Presence state.
    pub presence: PresenceState,
    /// Detailed proximity result (zone, trend, heat, distance, confidence).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proximity: Option<ProximityResult>,
    /// Confidence [0.0, 1.0].
    pub confidence: f64,
    /// Epoch seconds of first observation.
    pub first_seen: i64,
    /// Epoch seconds of most recent observation.
    pub last_seen: i64,
    /// Consecutive polls where the device was observed.
    pub consecutive_seen: u64,
    /// Consecutive polls where the device was not observed.
    pub consecutive_missing: u64,
    /// Best available hostname (for local diagnostics, never sent to backend).
    #[serde(skip_serializing)]
    pub hostname: Option<String>,
    /// Best available IP (for local diagnostics, never sent to backend).
    #[serde(skip_serializing)]
    pub ip: Option<String>,
    /// MAC address (for local diagnostics, never sent to backend raw).
    #[serde(skip_serializing)]
    pub mac: Option<String>,
}

/// Configuration for the presence engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceConfig {
    /// Consecutive missing polls before declaring a device as Away / LEAVE.
    pub missing_polls_before_leave: u64,
    /// Proximity engine configuration.
    pub proximity: ProximityConfig,
}

impl Default for PresenceConfig {
    fn default() -> Self {
        Self {
            missing_polls_before_leave: 3,
            proximity: ProximityConfig::default(),
        }
    }
}

/// Internal tracked state for one device.
#[derive(Debug, Clone, Default)]
struct TrackedDevice {
    /// Last observed raw signal (RCPI or dBm, depending on source).
    last_rssi: Option<i64>,
    /// Number of consecutive polls in which the device was observed.
    consecutive_seen: u64,
    /// Number of consecutive polls in which the device was NOT observed.
    consecutive_missing: u64,
    /// First seen timestamp.
    first_seen: i64,
    /// Last seen timestamp.
    last_seen: i64,
    /// Last known presence state.
    presence: PresenceState,
    /// Last known proximity result.
    proximity: Option<ProximityResult>,
    /// Cached device identity, hostname, ip, mac.
    identity: String,
    hostname: Option<String>,
    ip: Option<String>,
    mac: Option<String>,
}

/// Presence engine: maintains per-device state and computes
/// presence/proximity observations.
pub struct PresenceEngine {
    config: PresenceConfig,
    state: HashMap<String, TrackedDevice>,
    proximity_engine: ProximityEngine,
    now: Option<i64>,
}

impl PresenceEngine {
    pub fn new(config: PresenceConfig) -> Self {
        let proximity_engine = ProximityEngine::new(config.proximity.clone());
        Self {
            config,
            state: HashMap::new(),
            proximity_engine,
            now: None,
        }
    }

    /// Update the engine with a new snapshot. Returns the current observation
    /// for every tracked device (both observed and unobserved).
    pub fn update(&mut self, devices: &[Device], timestamp: i64) -> Vec<PresenceObservation> {
        self.now = Some(timestamp);

        // Mark all currently tracked devices as missing; observed ones will
        // be re-marked below.
        let observed_ids: std::collections::HashSet<String> =
            devices.iter().map(|d| d.identity()).collect();

        for (id, t) in &mut self.state {
            if !observed_ids.contains(id) {
                t.consecutive_missing += 1;
            }
            // presence/proximity will be recomputed for observed devices
        }

        // Process observed devices
        for d in devices {
            let id = d.identity();
            let raw_rssi = d.rssi;

            // Determine band from the radio MAC or interface.
            let band = d
                .radio_mac
                .as_deref()
                .map(Band::from_radio_mac)
                .unwrap_or_else(|| {
                    d.interface
                        .as_deref()
                        .map(|i| {
                            if i.starts_with("rax") {
                                Band::Ghz5
                            } else if i.starts_with("rai") {
                                Band::Ghz2_4
                            } else {
                                Band::Unknown
                            }
                        })
                        .unwrap_or(Band::Unknown)
                });

            // Determine whether rssi is the EX520's 0-127 RCPI scale or an
            // already-converted dBm value.  This lets tests and future sources
            // pass dBm directly while production GTPR data stays RCPI.
            let signal_type = match raw_rssi {
                Some(r) if (0..=255).contains(&r) => SignalType::Rcpi,
                Some(r) if (-120..=0).contains(&r) => SignalType::Dbm,
                _ => SignalType::Rcpi,
            };

            let proximity =
                self.proximity_engine
                    .update(&id, raw_rssi, signal_type, band, timestamp);

            let t = self
                .state
                .entry(id.clone())
                .or_insert_with(|| TrackedDevice {
                    last_rssi: raw_rssi,
                    consecutive_seen: 0,
                    consecutive_missing: 0,
                    first_seen: timestamp,
                    last_seen: timestamp,
                    presence: PresenceState::Unknown,
                    proximity: None,
                    identity: id.clone(),
                    hostname: d.hostname.clone(),
                    ip: d.ip.clone(),
                    mac: d.mac.clone(),
                });

            t.last_rssi = raw_rssi;
            t.consecutive_seen += 1;
            t.consecutive_missing = 0;
            t.last_seen = timestamp;
            t.hostname = d.hostname.clone().or(t.hostname.clone());
            t.ip = d.ip.clone().or(t.ip.clone());
            t.mac = d.mac.clone().or(t.mac.clone());
            t.proximity = Some(proximity);
            t.presence = PresenceState::Present;
        }

        // Update presence for missing devices (and possibly mark Away)
        for (id, t) in &mut self.state {
            if !observed_ids.contains(id) && t.consecutive_missing > 0 {
                let threshold = self.config.missing_polls_before_leave;
                if t.consecutive_missing >= threshold && t.presence != PresenceState::Away {
                    t.presence = PresenceState::Away;
                }
            }
        }

        // Build observations for all tracked devices
        self.state
            .values()
            .map(|t| PresenceObservation {
                identity: t.identity.clone(),
                rssi: t.last_rssi,
                rssi_smoothed: t.proximity.as_ref().and_then(|p| p.rssi_dbm),
                presence: t.presence,
                proximity: t.proximity.clone(),
                confidence: confidence(t),
                first_seen: t.first_seen,
                last_seen: t.last_seen,
                consecutive_seen: t.consecutive_seen,
                consecutive_missing: t.consecutive_missing,
                hostname: t.hostname.clone(),
                ip: t.ip.clone(),
                mac: t.mac.clone(),
            })
            .collect()
    }

    /// Return only the observations for devices that are currently `Present`.
    pub fn present(&self) -> Vec<PresenceObservation> {
        self.observations()
            .into_iter()
            .filter(|o| o.presence == PresenceState::Present)
            .collect()
    }

    /// Return all current observations.
    pub fn observations(&self) -> Vec<PresenceObservation> {
        self.state
            .values()
            .map(|t| PresenceObservation {
                identity: t.identity.clone(),
                rssi: t.last_rssi,
                rssi_smoothed: t.proximity.as_ref().and_then(|p| p.rssi_dbm),
                presence: t.presence,
                proximity: t.proximity.clone(),
                confidence: confidence(t),
                first_seen: t.first_seen,
                last_seen: t.last_seen,
                consecutive_seen: t.consecutive_seen,
                consecutive_missing: t.consecutive_missing,
                hostname: t.hostname.clone(),
                ip: t.ip.clone(),
                mac: t.mac.clone(),
            })
            .collect()
    }

    /// Prune devices that have been Away for a long time to control memory.
    /// `max_age` is in seconds.
    pub fn prune(&mut self, max_age: i64) {
        if let Some(now) = self.now {
            self.state
                .retain(|_, t| t.presence != PresenceState::Away || now - t.last_seen < max_age);
            let before = now - max_age;
            self.proximity_engine.prune(before);
        }
    }

    /// Number of tracked devices.
    pub fn len(&self) -> usize {
        self.state.len()
    }

    pub fn is_empty(&self) -> bool {
        self.state.is_empty()
    }

    /// Return devices that transitioned to `Away` in the last `update`.
    pub fn left_now(&self) -> Vec<String> {
        self.state
            .values()
            .filter(|t| {
                t.presence == PresenceState::Away
                    && t.consecutive_missing == self.config.missing_polls_before_leave
            })
            .map(|t| t.identity.clone())
            .collect()
    }

    /// Compute a one-off proximity for an arbitrary identity without tracking
    /// it in the presence state.  Useful for site-survey APs and probes.
    pub fn compute_proximity(
        &mut self,
        identity: &str,
        raw_signal: Option<i64>,
        signal_type: SignalType,
        band: Band,
        ts: i64,
    ) -> ProximityResult {
        self.proximity_engine
            .update(identity, raw_signal, signal_type, band, ts)
    }

    /// Look up a single device's presence observation by identity.
    pub fn lookup(&self, identity: &str) -> Option<PresenceObservation> {
        self.state.get(identity).map(|t| PresenceObservation {
            identity: t.identity.clone(),
            rssi: t.last_rssi,
            rssi_smoothed: t.proximity.as_ref().and_then(|p| p.rssi_dbm),
            presence: t.presence,
            proximity: t.proximity.clone(),
            confidence: confidence(t),
            first_seen: t.first_seen,
            last_seen: t.last_seen,
            consecutive_seen: t.consecutive_seen,
            consecutive_missing: t.consecutive_missing,
            hostname: t.hostname.clone(),
            ip: t.ip.clone(),
            mac: t.mac.clone(),
        })
    }

    /// Return devices that are `Present` and were just observed for the first
    /// time (`consecutive_seen == 1`).
    pub fn joined_now(&self) -> Vec<String> {
        self.state
            .values()
            .filter(|t| t.presence == PresenceState::Present && t.consecutive_seen == 1)
            .map(|t| t.identity.clone())
            .collect()
    }
}

/// Compute a presence confidence [0.0, 1.0].
/// It combines the proximity engine's signal confidence with the number of
/// consecutive observations to avoid overconfident estimates on first sight.
fn confidence(t: &TrackedDevice) -> f64 {
    if t.presence == PresenceState::Away {
        return 0.0;
    }
    if t.consecutive_seen == 0 {
        return 0.0;
    }

    let signal_conf = t
        .proximity
        .as_ref()
        .map_or(0.0, |p| p.confidence as f64)
        .clamp(0.0, 1.0);

    // Saturate sample confidence at ~5 consecutive observations.
    let sample_conf = (t.consecutive_seen as f64 / 5.0).min(1.0);

    // Weight the proximity confidence more heavily after a few samples.
    (signal_conf * 0.7 + sample_conf * 0.3).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proximity::{ProximityTrend, ProximityZone};

    fn dev(mac: &str, rssi: Option<i64>, _ts: i64) -> Device {
        Device {
            hostname: None,
            ip: None,
            mac: Some(mac.into()),
            rssi,
            standard: None,
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

    fn high_confidence_config() -> PresenceConfig {
        let mut pc = PresenceConfig::default();
        pc.proximity.history_window = 25;
        pc.proximity.trend_min_samples = 3;
        pc
    }

    #[test]
    fn device_joins_and_remains_present() {
        let mut engine = PresenceEngine::new(PresenceConfig::default());
        let ts = 1000;
        let obs = engine.update(&[dev("AA:BB:CC:00:00:01", Some(-50), ts)], ts);
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].presence, PresenceState::Present);
        let p = obs[0].proximity.as_ref().expect("proximity result");
        assert!(matches!(
            p.zone,
            ProximityZone::Immediate | ProximityZone::Near
        ));
        assert_eq!(obs[0].consecutive_seen, 1);
        assert_eq!(obs[0].consecutive_missing, 0);
        assert_eq!(obs[0].first_seen, ts);
        assert_eq!(obs[0].last_seen, ts);
    }

    #[test]
    fn leave_debounces_after_missing_polls() {
        let mut engine = PresenceEngine::new(PresenceConfig::default());
        let ts = 1000;
        engine.update(&[dev("AA:BB:CC:00:00:01", Some(-50), ts)], ts);

        // missing 1 poll → still Present
        let obs = engine.update(&[], ts + 30);
        let o = obs
            .iter()
            .find(|x| x.identity == "AA:BB:CC:00:00:01")
            .unwrap();
        assert_eq!(o.presence, PresenceState::Present);
        assert_eq!(o.consecutive_missing, 1);

        // missing 2 polls → still Present
        let obs = engine.update(&[], ts + 60);
        let o = obs
            .iter()
            .find(|x| x.identity == "AA:BB:CC:00:00:01")
            .unwrap();
        assert_eq!(o.presence, PresenceState::Present);
        assert_eq!(o.consecutive_missing, 2);

        // missing 3 polls → Away (default threshold)
        let obs = engine.update(&[], ts + 90);
        let o = obs
            .iter()
            .find(|x| x.identity == "AA:BB:CC:00:00:01")
            .unwrap();
        assert_eq!(o.presence, PresenceState::Away);
        assert_eq!(o.consecutive_missing, 3);

        // still missing → stays Away
        let obs = engine.update(&[], ts + 120);
        let o = obs
            .iter()
            .find(|x| x.identity == "AA:BB:CC:00:00:01")
            .unwrap();
        assert_eq!(o.presence, PresenceState::Away);
        assert_eq!(o.consecutive_missing, 4);
    }

    #[test]
    fn proximity_classifies_by_smoothed_rssi() {
        let mut engine = PresenceEngine::new(PresenceConfig::default());
        let ts = 1000;

        // Start with a weak/far signal (-80 dBm).
        let obs = engine.update(&[dev("AA:BB:CC:00:00:01", Some(-80), ts)], ts);
        let p = obs[0].proximity.as_ref().expect("proximity");
        assert!(matches!(p.zone, ProximityZone::Far | ProximityZone::Medium));

        // A single strong -42 dBm would normally be Immediate, but smoothing
        // with the previous -80 keeps the dBm value between -80 and -42,
        // pulling the classification toward the middle (Medium/Near).
        let obs = engine.update(&[dev("AA:BB:CC:00:00:01", Some(-42), ts + 30)], ts + 30);
        let o = obs
            .iter()
            .find(|x| x.identity == "AA:BB:CC:00:00:01")
            .unwrap();
        assert_eq!(o.presence, PresenceState::Present);
        let smoothed = o.rssi_smoothed.expect("smoothed rssi");
        assert!(smoothed > -80.0 && smoothed < -40.0);
    }

    #[test]
    fn trend_detected_when_signal_strengthens() {
        let mut engine = PresenceEngine::new(PresenceConfig::default());
        let ts = 1000;
        engine.update(&[dev("AA:BB:CC:00:00:01", Some(-80), ts)], ts);
        engine.update(&[dev("AA:BB:CC:00:00:01", Some(-60), ts + 1)], ts + 1);
        let obs = engine.update(&[dev("AA:BB:CC:00:00:01", Some(-45), ts + 2)], ts + 2);
        let o = obs
            .iter()
            .find(|x| x.identity == "AA:BB:CC:00:00:01")
            .unwrap();
        let p = o.proximity.as_ref().expect("proximity");
        assert_eq!(p.trend, ProximityTrend::Approaching);
    }

    #[test]
    fn confidence_grows_with_samples() {
        let mut engine = PresenceEngine::new(high_confidence_config());
        let ts = 1000;
        for i in 0..20 {
            engine.update(&[dev("AA:BB:CC:00:00:01", Some(-55), ts + i)], ts + i);
        }
        let obs = engine.observations();
        let o = obs
            .iter()
            .find(|x| x.identity == "AA:BB:CC:00:00:01")
            .unwrap();
        assert!(o.confidence > 0.6, "confidence was {}", o.confidence);
        assert!(o.confidence <= 1.0);
    }

    #[test]
    fn away_devices_have_zero_confidence() {
        let mut engine = PresenceEngine::new(PresenceConfig::default());
        engine.update(&[dev("AA:BB:CC:00:00:01", Some(-50), 1000)], 1000);
        for i in 1..=3 {
            engine.update(&[], 1000 + i);
        }
        let obs = engine.observations();
        let o = obs
            .iter()
            .find(|x| x.identity == "AA:BB:CC:00:00:01")
            .unwrap();
        assert_eq!(o.confidence, 0.0);
    }

    #[test]
    fn prune_removes_stale_away_devices() {
        let mut engine = PresenceEngine::new(PresenceConfig::default());
        let ts = 1000;
        // Observe, then miss for 3 polls -> Away at ts+3 with last_seen=ts.
        engine.update(&[dev("AA:BB:CC:00:00:01", Some(-50), ts)], ts);
        for i in 1..=3 {
            engine.update(&[], ts + i);
        }
        // last_seen is still ts (1000). If we prune with max_age < 3 (e.g. 1s)
        // from now=ts+3, it should be removed because it was last seen 3s ago.
        engine.prune(1);
        assert_eq!(engine.len(), 0);

        // Re-observe, then miss 3 polls again. Prune with max_age larger than
        // elapsed since last observation: the Away device should be retained.
        engine.update(&[dev("AA:BB:CC:00:00:01", Some(-50), ts)], ts);
        for i in 1..=3 {
            engine.update(&[], ts + i);
        }
        engine.prune(10);
        assert_eq!(engine.len(), 1); // Away, but still within the 10s grace window
    }

    #[test]
    fn joined_now_and_left_now_helpers() {
        let mut engine = PresenceEngine::new(PresenceConfig::default());
        engine.update(&[dev("AA:BB:CC:00:00:01", Some(-50), 1000)], 1000);
        assert_eq!(engine.joined_now(), vec!["AA:BB:CC:00:00:01"]);
        for i in 1..=3 {
            engine.update(&[], 1000 + i);
        }
        assert_eq!(engine.left_now(), vec!["AA:BB:CC:00:00:01"]);
    }
}
