//! Presence fusion (M10-B).
//!
//! Merges GTPR-associated stations with nearby (monitor) observations into a
//! single deduplicated `UnifiedPresenceDevice` list. The fusion layer:
//!
//! - prefers GTPR identity (hostname, IP, client_type) when the same MAC is
//!   observed by both sources
//! - keeps the strongest RSSI across sources
//! - stores different BSSIDs separately (a device visible on two APs is two
//!   rows until correlated by MAC)
//! - maintains first_seen / last_seen history

use crate::model::Device;
use crate::monitor::NearbyObservation;
use crate::presence::{PresenceState, Proximity, ProximityThresholds};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A unified presence record combining associated + nearby data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedPresenceDevice {
    /// Stable identity (MAC or pseudonym).
    pub identity: String,
    /// True if observed via GTPR (associated).
    pub associated: bool,
    /// True if observed via monitor/survey (nearby).
    pub nearby: bool,
    /// Best hostname (from GTPR if associated).
    pub hostname: Option<String>,
    /// Best IP (from GTPR if associated).
    pub ip: Option<String>,
    /// Best IPv6 (from GTPR if associated).
    pub ipv6: Option<String>,
    /// Best client_type (from GTPR if associated).
    pub client_type: Option<String>,
    /// Interface (from GTPR if associated).
    pub interface: Option<String>,
    /// Strongest RSSI observed across sources.
    pub rssi: Option<i64>,
    /// Smoothed RSSI (if maintained by the presence engine).
    pub rssi_smoothed: Option<f64>,
    /// Presence state from the engine.
    pub presence: PresenceState,
    /// Proximity classification.
    pub proximity: Proximity,
    /// Confidence [0.0, 1.0].
    pub confidence: f64,
    /// First seen (epoch seconds).
    pub first_seen: i64,
    /// Last seen (epoch seconds).
    pub last_seen: i64,
    /// Source BSSID (for nearby observations).
    pub bssid: Option<String>,
    /// Source SSID (for nearby observations).
    pub ssid: Option<String>,
    /// Channel (for nearby observations).
    pub channel: Option<u32>,
    /// Band (for nearby observations).
    pub band: Option<String>,
}

/// Fuse associated devices (from GTPR) and nearby observations (from the
/// monitor provider) into a single deduplicated list.
///
/// `presence_rssi` and `presence_state`/`proximity`/`confidence` come from the
/// `PresenceEngine` keyed by identity (MAC).
pub fn fuse(
    associated: &[Device],
    nearby: &[NearbyObservation],
    presence_state: &HashMap<String, (PresenceState, Proximity, f64, i64, i64)>,
    thresholds: &ProximityThresholds,
) -> Vec<UnifiedPresenceDevice> {
    let mut map: HashMap<String, UnifiedPresenceDevice> = HashMap::new();

    // Associated devices
    for d in associated {
        let id = d.identity();
        let rssi = d.rssi;
        let (presence, proximity, confidence, first, last) =
            presence_state.get(&id).cloned().unwrap_or((
                PresenceState::Present,
                classify_rssi(rssi, thresholds),
                0.5,
                0,
                0,
            ));
        let entry = map.entry(id.clone()).or_insert(UnifiedPresenceDevice {
            identity: id.clone(),
            associated: true,
            nearby: false,
            hostname: d.hostname.clone(),
            ip: d.ip.clone(),
            ipv6: d.ipv6.clone(),
            client_type: d.client_type.clone(),
            interface: d.interface.clone(),
            rssi,
            rssi_smoothed: rssi.map(|v| v as f64),
            presence,
            proximity,
            confidence,
            first_seen: first,
            last_seen: last,
            bssid: d.radio_mac.clone(),
            ssid: None,
            channel: None,
            band: None,
        });
        // Update strongest RSSI
        if let Some(r) = rssi {
            if entry.rssi.map(|e| r > e).unwrap_or(true) {
                entry.rssi = Some(r);
            }
        }
        entry.associated = true;
        // Prefer GTPR identity fields
        if entry.hostname.is_none() {
            entry.hostname = d.hostname.clone();
        }
        if entry.ip.is_none() {
            entry.ip = d.ip.clone();
        }
    }

    // Nearby observations
    for n in nearby {
        let id = n.mac.clone();
        let rssi = n.rssi;
        let (presence, proximity, confidence, first, last) =
            presence_state.get(&id).cloned().unwrap_or((
                PresenceState::Present,
                classify_rssi(rssi, thresholds),
                n.confidence,
                n.timestamp,
                n.timestamp,
            ));
        let entry = map.entry(id.clone()).or_insert(UnifiedPresenceDevice {
            identity: id.clone(),
            associated: false,
            nearby: true,
            hostname: None,
            ip: None,
            ipv6: None,
            client_type: None,
            interface: None,
            rssi,
            rssi_smoothed: rssi.map(|v| v as f64),
            presence,
            proximity,
            confidence,
            first_seen: first,
            last_seen: last,
            bssid: Some(n.bssid.clone()),
            ssid: Some(n.ssid.clone()),
            channel: Some(n.channel),
            band: Some(n.band.clone()),
        });
        entry.nearby = true;
        if let Some(r) = rssi {
            if entry.rssi.map(|e| r > e).unwrap_or(true) {
                entry.rssi = Some(r);
            }
        }
        if entry.bssid.is_none() {
            entry.bssid = Some(n.bssid.clone());
        }
        if entry.ssid.is_none() {
            entry.ssid = Some(n.ssid.clone());
        }
        if entry.channel.is_none() {
            entry.channel = Some(n.channel);
        }
        if entry.band.is_none() {
            entry.band = Some(n.band.clone());
        }
    }

    map.into_values().collect()
}

fn classify_rssi(rssi: Option<i64>, t: &ProximityThresholds) -> Proximity {
    match rssi {
        Some(r) => t.classify(r),
        None => Proximity::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::NearbySource;

    fn assoc_dev(mac: &str, rssi: Option<i64>) -> Device {
        Device {
            hostname: Some(format!("host-{}", mac)),
            ip: Some("192.168.0.10".into()),
            mac: Some(mac.into()),
            rssi,
            standard: None,
            onemesh_stack: None,
            assoc_time: None,
            radio_mac: Some("3C:6A:D2:5F:AB:C1".into()),
            source: Some("wifi".into()),
            tx_rate: None,
            rx_rate: None,
            noise: None,
            signal_level: None,
            max_link_rate: None,
            interface: Some("Device.WiFi.AccessPoint.1.".into()),
            ipv6: None,
            client_type: Some("Android".into()),
            active: Some("1".into()),
        }
    }

    fn nearby(mac: &str, rssi: i64) -> NearbyObservation {
        NearbyObservation {
            mac: mac.into(),
            bssid: "64:61:40:41:e0:e0".into(),
            ssid: "Juliana".into(),
            channel: 1,
            band: "2.4GHz".into(),
            rssi: Some(rssi),
            timestamp: 1000,
            source: NearbySource::Survey,
            confidence: 0.6,
        }
    }

    #[test]
    fn fusion_dedupes_by_mac_preferring_gtpr_identity() {
        let associated = vec![assoc_dev("AA:BB:CC:00:00:01", Some(-50))];
        let nearby_obs = vec![nearby("AA:BB:CC:00:00:01", -70)];
        let mut state = HashMap::new();
        state.insert(
            "AA:BB:CC:00:00:01".into(),
            (PresenceState::Present, Proximity::Near, 0.9, 1000, 1010),
        );
        let fused = fuse(
            &associated,
            &nearby_obs,
            &state,
            &ProximityThresholds::default(),
        );
        assert_eq!(fused.len(), 1);
        let d = &fused[0];
        assert!(d.associated);
        assert!(d.nearby);
        assert_eq!(d.hostname, Some("host-AA:BB:CC:00:00:01".into()));
        assert_eq!(d.ip, Some("192.168.0.10".into()));
        assert_eq!(d.rssi, Some(-50)); // strongest
    }

    #[test]
    fn fusion_keeps_separate_macs() {
        let associated = vec![assoc_dev("AA:BB:CC:00:00:01", Some(-50))];
        let nearby_obs = vec![nearby("11:22:33:00:00:09", -75)];
        let state = HashMap::new();
        let fused = fuse(
            &associated,
            &nearby_obs,
            &state,
            &ProximityThresholds::default(),
        );
        assert_eq!(fused.len(), 2);
        let assoc = fused.iter().find(|d| d.associated).unwrap();
        let nb = fused.iter().find(|d| !d.associated).unwrap();
        assert_eq!(assoc.identity, "AA:BB:CC:00:00:01");
        assert_eq!(nb.identity, "11:22:33:00:00:09");
        assert!(nb.nearby);
        assert!(!nb.associated);
    }

    #[test]
    fn fusion_keeps_strongest_rssi() {
        let associated = vec![assoc_dev("AA:BB:CC:00:00:01", Some(-60))];
        let nearby_obs = vec![nearby("AA:BB:CC:00:00:01", -40)];
        let state = HashMap::new();
        let fused = fuse(
            &associated,
            &nearby_obs,
            &state,
            &ProximityThresholds::default(),
        );
        assert_eq!(fused[0].rssi, Some(-40));
    }
}
