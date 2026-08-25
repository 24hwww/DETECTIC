//! Data models for Detectic: the normalized device record and the network map.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single observed Wi-Fi / network device, normalized from the GTPR OID
/// responses. Fields are `Option` because not every OID exposes every field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Device {
    pub hostname: Option<String>,
    pub ip: Option<String>,
    #[serde(rename = "mac")]
    pub mac: Option<String>,
    /// RSSI as reported by the firmware (0-128 scale; higher = weaker signal).
    pub rssi: Option<i64>,
    /// Operating standard, e.g. "ax", "ac", "n".
    pub standard: Option<String>,
    /// OneMesh hierarchical stack, e.g. "1,1,2,1,0,0".
    pub onemesh_stack: Option<String>,
    /// Association time (epoch seconds), when available.
    pub assoc_time: Option<i64>,
    /// Radio MAC (the AP radio this device is attached to).
    pub radio_mac: Option<String>,
    /// Where this record came from: "wifi", "dhcp", or "host".
    pub source: Option<String>,
    // --- M5 extended fields (from DEV2_WIFI_APDEV_ASSOCDEV / DEV2_HOST_ENTRY) ---
    /// TX (downlink) rate in kbps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tx_rate: Option<u64>,
    /// RX (uplink) rate in kbps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rx_rate: Option<u64>,
    /// Noise floor (vendor scale 0-127).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub noise: Option<u64>,
    /// Signal strength level (1-5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal_level: Option<u8>,
    /// Max link rate in kbps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_link_rate: Option<u64>,
    /// Interface name (e.g. "rai0", "rax0") or L2 path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
    /// IPv6 address, when available from the host table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ipv6: Option<String>,
    /// Client type (e.g. "Android", "iOS", "Other"), from host table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_type: Option<String>,
    /// Active flag ("1" = active).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
}

impl Device {
    /// Stable per-snapshot identity: prefer MAC, then IP, then hostname.
    pub fn identity(&self) -> String {
        self.mac
            .clone()
            .or_else(|| self.ip.clone())
            .or_else(|| self.hostname.clone())
            .unwrap_or_default()
    }
}

/// A complete network map snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkMap {
    pub captured_at: i64,
    pub devices: Vec<Device>,
    /// Raw OID payloads by OID, preserved for debugging / future parsing.
    #[serde(default)]
    pub raw: HashMap<String, serde_json::Value>,
}

/// Coarse distance bucket — avoids false precision from RSSI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProximityBucket {
    VeryNear,
    Near,
    Medium,
    Far,
    VeryFar,
    Unknown,
}

/// Estimated distance from the sensor, derived from RSSI/RCPI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DistanceEstimate {
    /// Coarse proximity bucket.
    pub bucket: ProximityBucket,
    /// Estimated distance in meters (for logging; not high-precision).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_distance_m: Option<f32>,
    /// RSSI in dBm used for this estimate (after smoothing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rssi_dbm: Option<f32>,
    /// Confidence [0.0, 1.0].
    pub confidence: f32,
    /// Whether the sensor has been calibrated for this band.
    #[serde(default)]
    pub calibrated: bool,
    /// Wi-Fi band: 2.4 GHz or 5 GHz (channel center frequency in MHz).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub band_mhz: Option<u32>,
}

/// Difference between two snapshots, used for change detection.
#[derive(Debug, Clone, Default)]
pub struct MapDiff {
    pub added: Vec<Device>,
    pub removed: Vec<Device>,
    pub changed: Vec<(Device, Device)>, // (before, after)
}

impl NetworkMap {
    /// Compute what changed between `self` (previous) and `other` (current).
    pub fn diff(&self, other: &NetworkMap) -> MapDiff {
        let prev: HashMap<_, _> = self
            .devices
            .iter()
            .map(|d| (d.identity(), d.clone()))
            .collect();
        let curr: HashMap<_, _> = other
            .devices
            .iter()
            .map(|d| (d.identity(), d.clone()))
            .collect();

        let mut diff = MapDiff::default();
        for (id, d) in &curr {
            match prev.get(id) {
                None => diff.added.push(d.clone()),
                Some(p) if p != d => diff.changed.push((p.clone(), d.clone())),
                _ => {}
            }
        }
        for (id, d) in &prev {
            if !curr.contains_key(id) {
                diff.removed.push(d.clone());
            }
        }
        diff
    }
}
