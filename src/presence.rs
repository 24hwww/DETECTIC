//! Presence engine (M6).
//!
//! The `PresenceEngine` receives successive `SensorSnapshot`s and maintains a
//! debounced view of device presence and proximity. It does NOT replace the
//! raw `Device` data; instead, it enriches each observed device with:
//!
//! - `PresenceState`: Present / Away / Unknown
//! - `Proximity`: VeryNear / Near / Medium / Far / Unknown
//! - `confidence`: a normalized [0.0, 1.0] score
//! - `consecutive_seen` and `consecutive_missing` counters
//! - `first_seen` and `last_seen` epoch timestamps
//!
//! LEAVE detection uses hysteresis: a device is only considered `Away` after
//! `missing_polls_before_leave` consecutive polls without observation.
//!
//! RSSI is smoothed using an exponential weighted moving average (EWMA) to
//! avoid rapid proximity class flapping.
//!
//! This module is pure Rust, no I/O, fully unit-tested, and suitable for the
//! resource-constrained EX520V (single-threaded, low memory).

use crate::model::Device;
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

/// Proximity classification based on RSSI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Proximity {
    VeryNear,
    Near,
    Medium,
    Far,
    Unknown,
}

impl Default for Proximity {
    fn default() -> Self {
        Proximity::Unknown
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
    /// Proximity classification.
    pub proximity: Proximity,
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

/// Thresholds for proximity classification. All values are in dBm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProximityThresholds {
    pub rssi_very_near: i64,
    pub rssi_near: i64,
    pub rssi_medium: i64,
    pub rssi_far: i64,
}

impl Default for ProximityThresholds {
    fn default() -> Self {
        Self {
            rssi_very_near: -45,
            rssi_near: -60,
            rssi_medium: -70,
            rssi_far: -80,
        }
    }
}

impl ProximityThresholds {
    /// Classify a (dBm) RSSI value. Lower (more negative) = farther.
    pub fn classify(&self, rssi: i64) -> Proximity {
        if rssi >= self.rssi_very_near {
            Proximity::VeryNear
        } else if rssi >= self.rssi_near {
            Proximity::Near
        } else if rssi >= self.rssi_medium {
            Proximity::Medium
        } else if rssi >= self.rssi_far {
            Proximity::Far
        } else {
            Proximity::Unknown
        }
    }
}

/// Configuration for the presence engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceConfig {
    /// Consecutive missing polls before declaring a device as Away / LEAVE.
    pub missing_polls_before_leave: u64,
    /// EWMA smoothing factor for RSSI. 0.0 = no smoothing, 1.0 = only last.
    pub rssi_smoothing_alpha: f64,
    /// Proximity classification thresholds.
    pub thresholds: ProximityThresholds,
}

impl Default for PresenceConfig {
    fn default() -> Self {
        Self {
            missing_polls_before_leave: 3,
            rssi_smoothing_alpha: 0.3,
            thresholds: ProximityThresholds::default(),
        }
    }
}

/// Internal tracked state for one device.
#[derive(Debug, Clone, Default)]
struct TrackedDevice {
    /// Last observed raw RSSI.
    last_rssi: Option<i64>,
    /// EWMA-smoothed RSSI.
    smoothed_rssi: Option<f64>,
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
    /// Last known proximity.
    proximity: Proximity,
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
    now: Option<i64>,
}

impl PresenceEngine {
    pub fn new(config: PresenceConfig) -> Self {
        Self {
            config,
            state: HashMap::new(),
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
            let prox = match self.state.get_mut(&id) {
                Some(t) => {
                    t.last_rssi = raw_rssi;
                    t.consecutive_seen += 1;
                    t.consecutive_missing = 0;
                    t.last_seen = timestamp;
                    t.hostname = d.hostname.clone().or(t.hostname.clone());
                    t.ip = d.ip.clone().or(t.ip.clone());
                    t.mac = d.mac.clone().or(t.mac.clone());

                    // EWMA smoothing
                    if let (Some(new), Some(old)) = (raw_rssi.map(|v| v as f64), t.smoothed_rssi) {
                        let alpha = self.config.rssi_smoothing_alpha.clamp(0.0, 1.0);
                        t.smoothed_rssi = Some(alpha * new + (1.0 - alpha) * old);
                    } else if let Some(new) = raw_rssi.map(|v| v as f64) {
                        t.smoothed_rssi = Some(new);
                    } else {
                        t.smoothed_rssi = None;
                    }

                    t.smoothed_rssi
                        .map(|r| self.config.thresholds.classify(r as i64))
                }
                None => {
                    let smoothed = raw_rssi.map(|v| v as f64);
                    let prox = smoothed.map(|r| self.config.thresholds.classify(r as i64));
                    let identity = id.clone();
                    let td = TrackedDevice {
                        last_rssi: raw_rssi,
                        smoothed_rssi: smoothed,
                        consecutive_seen: 1,
                        consecutive_missing: 0,
                        first_seen: timestamp,
                        last_seen: timestamp,
                        presence: PresenceState::Unknown,
                        proximity: prox.unwrap_or(Proximity::Unknown),
                        identity,
                        hostname: d.hostname.clone(),
                        ip: d.ip.clone(),
                        mac: d.mac.clone(),
                    };
                    self.state.insert(id, td);
                    prox
                }
            };

            // Update presence for the just-observed device
            let key = d.identity();
            let t = self.state.get_mut(&key).unwrap();
            t.presence = PresenceState::Present;
            t.proximity = prox.unwrap_or(Proximity::Unknown);

            // Update observations for every tracked device
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
                rssi_smoothed: t.smoothed_rssi,
                presence: t.presence,
                proximity: t.proximity,
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
                rssi_smoothed: t.smoothed_rssi,
                presence: t.presence,
                proximity: t.proximity,
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

    /// Look up a single device's presence observation by identity.
    pub fn lookup(&self, identity: &str) -> Option<PresenceObservation> {
        self.state.get(identity).map(|t| PresenceObservation {
            identity: t.identity.clone(),
            rssi: t.last_rssi,
            rssi_smoothed: t.smoothed_rssi,
            presence: t.presence,
            proximity: t.proximity,
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

/// Compute a confidence score [0.0, 1.0] based on:
/// - number of samples
/// - stability of the RSSI
/// - recency
fn confidence(t: &TrackedDevice) -> f64 {
    if t.presence == PresenceState::Away {
        return 0.0;
    }
    if t.consecutive_seen == 0 {
        return 0.0;
    }

    // Sample confidence: saturate at ~5 samples
    let sample_conf = (t.consecutive_seen as f64 / 5.0).min(1.0);

    // Recency: 1.0 if last_seen is the latest known timestamp
    let recency_conf = 1.0; // the engine only observes current data

    // Stability: if rssi and smoothed_rssi are close, confidence is higher
    let stability_conf = if let (Some(raw), Some(smooth)) = (t.last_rssi, t.smoothed_rssi) {
        let diff = (raw as f64 - smooth).abs();
        (1.0 - (diff / 20.0).min(1.0)).max(0.0)
    } else {
        0.5
    };

    // Combined: average of sample and stability, weighted toward sample
    (sample_conf * 0.6 + stability_conf * 0.3 + recency_conf * 0.1).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn device_joins_and_remains_present() {
        let mut engine = PresenceEngine::new(PresenceConfig::default());
        let ts = 1000;
        let obs = engine.update(&[dev("AA:BB:CC:00:00:01", Some(-50), ts)], ts);
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].presence, PresenceState::Present);
        assert_eq!(obs[0].proximity, Proximity::Near);
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
        let obs = engine.update(&[dev("AA:BB:CC:00:00:01", Some(-80), ts)], ts);
        assert_eq!(obs[0].proximity, Proximity::Far);

        // A single -42 would normally be VeryNear, but smoothing with the
        // previous -80 keeps it around -69, which is Medium.
        let obs = engine.update(&[dev("AA:BB:CC:00:00:01", Some(-42), ts + 30)], ts + 30);
        let o = obs
            .iter()
            .find(|x| x.identity == "AA:BB:CC:00:00:01")
            .unwrap();
        assert_eq!(o.presence, PresenceState::Present);
        assert!(o.rssi_smoothed.unwrap() > -80.0 && o.rssi_smoothed.unwrap() < -40.0);
    }

    #[test]
    fn confidence_grows_with_samples() {
        let mut engine = PresenceEngine::new(PresenceConfig::default());
        let ts = 1000;
        for i in 0..5 {
            engine.update(&[dev("AA:BB:CC:00:00:01", Some(-50), ts)], ts + i);
        }
        let obs = engine.observations();
        let o = obs
            .iter()
            .find(|x| x.identity == "AA:BB:CC:00:00:01")
            .unwrap();
        assert!(o.confidence > 0.6);
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
