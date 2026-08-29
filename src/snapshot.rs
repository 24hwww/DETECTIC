//! Sensor snapshot model (M5-C).
//!
//! A `SensorSnapshot` is the stable internal representation of everything
//! Detectic observed at a single polling instant. It is richer than
//! `NetworkMap` (which is just the merged OID data) because it also carries
//! router identity, uptime, radio statistics, and optional nearby-AP summary.
//!
//! Fields that the firmware does not provide are `None` — we never invent values.

use crate::model::Device;
use crate::proximity::ProximityResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Router identity (from `DEV2_DEV_INFO`).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RouterIdentity {
    pub manufacturer: Option<String>,
    pub model_name: Option<String>,
    pub description: Option<String>,
    pub serial_number: Option<String>,
    pub mac_address: Option<String>,
    pub hardware_version: Option<String>,
    pub software_version: Option<String>,
}

/// Per-radio statistics (from `iwpriv stat`, optional).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RadioStats {
    pub interface: String,
    pub band: Option<String>,
    pub temperature: Option<i64>,
    pub tx_success: Option<u64>,
    pub tx_fail: Option<u64>,
    pub rx_success: Option<u64>,
    pub rx_crc: Option<u64>,
    pub noise_floor_dbm: Option<i64>,
    pub last_tx_rate: Option<String>,
    pub last_rx_rate: Option<String>,
    /// Per-chain receive RSSI (dBm-ish raw values) from the `Rssi:` line of
    /// `iwpriv <if> stat`. Order corresponds to antenna chains. Empty if the
    /// line was absent or malformed; never fabricated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rssi_per_chain: Vec<i64>,
}

/// Nearby AP summary entry (from `iwpriv get_site_survey`, optional).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct NearbyAp {
    pub channel: Option<u8>,
    pub ssid: Option<String>,
    pub bssid: Option<String>,
    pub security: Option<String>,
    pub signal_pct: Option<u8>,
    pub w_mode: Option<String>,
}

/// A complete sensor snapshot — the unit of observation for the runtime.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SensorSnapshot {
    /// Epoch seconds when the snapshot was captured.
    pub timestamp: i64,
    /// Router identity (if `DEV2_DEV_INFO` was fetched).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router: Option<RouterIdentity>,
    /// Router uptime in seconds (if available).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime: Option<u64>,
    /// Wi-Fi stations and host-table devices.
    pub stations: Vec<Device>,
    /// Proximity result per station identity, populated by the presence engine.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub station_proximity: HashMap<String, ProximityResult>,
    /// Per-radio statistics (optional, when `enable_radio_stats` is true).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub radio_stats: Vec<RadioStats>,
    /// Nearby AP summary (optional, when `enable_site_survey` is true).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nearby_aps: Vec<NearbyAp>,
}

impl SensorSnapshot {
    /// Build a snapshot from a `NetworkMap` (the collector output).
    /// Applies the station bound from config.
    pub fn from_map(map: &crate::model::NetworkMap, max_stations: usize) -> Self {
        let stations: Vec<Device> = map.devices.iter().take(max_stations).cloned().collect();
        Self {
            timestamp: map.captured_at,
            router: None,
            uptime: None,
            stations,
            station_proximity: HashMap::new(),
            radio_stats: Vec::new(),
            nearby_aps: Vec::new(),
        }
    }

    /// Number of Wi-Fi associated stations (source == "wifi").
    pub fn wifi_station_count(&self) -> usize {
        self.stations
            .iter()
            .filter(|d| d.source.as_deref() == Some("wifi"))
            .count()
    }

    /// Number of total devices (Wi-Fi + Ethernet + DHCP).
    pub fn device_count(&self) -> usize {
        self.stations.len()
    }
}

/// Difference between two snapshots, used for change detection (M5-D).
/// This is a superset of `MapDiff` that also tracks radio/nearby-AP changes.
#[derive(Debug, Clone, Default)]
pub struct SnapshotDiff {
    /// Devices that joined since the previous snapshot.
    pub joined: Vec<Device>,
    /// Devices that left since the previous snapshot.
    pub left: Vec<Device>,
    /// Devices that changed (before, after) — at least one field differs.
    pub updated: Vec<(Device, Device)>,
}

/// Compute the diff between two snapshots by comparing station identities.
/// This is polling-derived change detection, NOT real-time kernel events.
pub fn diff_snapshots(prev: &SensorSnapshot, curr: &SensorSnapshot) -> SnapshotDiff {
    use std::collections::HashMap;
    let prev_map: HashMap<String, &Device> =
        prev.stations.iter().map(|d| (d.identity(), d)).collect();
    let curr_map: HashMap<String, &Device> =
        curr.stations.iter().map(|d| (d.identity(), d)).collect();

    let mut diff = SnapshotDiff::default();
    for (id, d) in &curr_map {
        match prev_map.get(id) {
            None => diff.joined.push((*d).clone()),
            Some(p) if *p != *d => diff.updated.push(((*p).clone(), (*d).clone())),
            _ => {}
        }
    }
    for (id, d) in &prev_map {
        if !curr_map.contains_key(id) {
            diff.left.push((*d).clone());
        }
    }
    diff
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn snapshot_from_map_respects_max_stations() {
        let map = crate::model::NetworkMap {
            captured_at: 100,
            devices: vec![
                station("AA:BB:CC:00:00:01", Some(50)),
                station("AA:BB:CC:00:00:02", Some(60)),
                station("AA:BB:CC:00:00:03", Some(70)),
            ],
            raw: Default::default(),
        };
        let snap = SensorSnapshot::from_map(&map, 2);
        assert_eq!(snap.stations.len(), 2);
        assert_eq!(snap.timestamp, 100);
    }

    #[test]
    fn wifi_station_count_filters_by_source() {
        let mut d = station("AA:BB:CC:00:00:01", Some(50));
        d.source = Some("host".into());
        let snap = SensorSnapshot {
            timestamp: 100,
            stations: vec![station("AA:BB:CC:00:00:02", Some(60)), d],
            ..Default::default()
        };
        assert_eq!(snap.wifi_station_count(), 1);
        assert_eq!(snap.device_count(), 2);
    }

    #[test]
    fn diff_detects_join() {
        let prev = SensorSnapshot {
            timestamp: 100,
            stations: vec![station("AA:BB:CC:00:00:01", Some(50))],
            ..Default::default()
        };
        let curr = SensorSnapshot {
            timestamp: 200,
            stations: vec![
                station("AA:BB:CC:00:00:01", Some(50)),
                station("AA:BB:CC:00:00:02", Some(60)),
            ],
            ..Default::default()
        };
        let diff = diff_snapshots(&prev, &curr);
        assert_eq!(diff.joined.len(), 1);
        assert_eq!(diff.left.len(), 0);
        assert_eq!(diff.updated.len(), 0);
    }

    #[test]
    fn diff_detects_leave() {
        let prev = SensorSnapshot {
            timestamp: 100,
            stations: vec![
                station("AA:BB:CC:00:00:01", Some(50)),
                station("AA:BB:CC:00:00:02", Some(60)),
            ],
            ..Default::default()
        };
        let curr = SensorSnapshot {
            timestamp: 200,
            stations: vec![station("AA:BB:CC:00:00:01", Some(50))],
            ..Default::default()
        };
        let diff = diff_snapshots(&prev, &curr);
        assert_eq!(diff.joined.len(), 0);
        assert_eq!(diff.left.len(), 1);
        assert_eq!(diff.updated.len(), 0);
    }

    #[test]
    fn diff_detects_rssi_change() {
        let prev = SensorSnapshot {
            timestamp: 100,
            stations: vec![station("AA:BB:CC:00:00:01", Some(50))],
            ..Default::default()
        };
        let curr = SensorSnapshot {
            timestamp: 200,
            stations: vec![station("AA:BB:CC:00:00:01", Some(60))],
            ..Default::default()
        };
        let diff = diff_snapshots(&prev, &curr);
        assert_eq!(diff.updated.len(), 1);
        assert_eq!(diff.joined.len(), 0);
        assert_eq!(diff.left.len(), 0);
    }

    #[test]
    fn diff_detects_tx_rate_change() {
        let mut prev_d = station("AA:BB:CC:00:00:01", Some(50));
        prev_d.tx_rate = Some(72000);
        let mut curr_d = station("AA:BB:CC:00:00:01", Some(50));
        curr_d.tx_rate = Some(96000);
        let prev = SensorSnapshot {
            timestamp: 100,
            stations: vec![prev_d],
            ..Default::default()
        };
        let curr = SensorSnapshot {
            timestamp: 200,
            stations: vec![curr_d],
            ..Default::default()
        };
        let diff = diff_snapshots(&prev, &curr);
        assert_eq!(diff.updated.len(), 1);
    }

    #[test]
    fn diff_no_change_when_identical() {
        let snap = SensorSnapshot {
            timestamp: 100,
            stations: vec![station("AA:BB:CC:00:00:01", Some(50))],
            ..Default::default()
        };
        let diff = diff_snapshots(&snap, &snap);
        assert!(diff.joined.is_empty());
        assert!(diff.left.is_empty());
        assert!(diff.updated.is_empty());
    }
}
