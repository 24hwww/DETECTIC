//! Band-aware proximity calibration tooling.
//!
//! The EX520V reports MediaTek RCPI 0..127 via signalStrength.
//! We treat the native scale as primary and avoid false dBm conversion.
//! Calibration records samples at known distances for empirical relative proximity.

use crate::model::Device;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Band {
    Ghz2_4,
    Ghz5,
    Unknown,
}

impl Band {
    /// Derive band from the EX520's RadioMac suffix.
    /// C1 = 2.4GHz, C3 = 5GHz (proven from live GTPR observations).
    pub fn from_radio_mac(radio_mac: &str) -> Self {
        let mac = radio_mac.to_uppercase();
        if mac.ends_with(":C3") {
            Band::Ghz5
        } else if mac.ends_with(":C1") {
            Band::Ghz2_4
        } else {
            Band::Unknown
        }
    }
}

/// Proximity bucket — coarse distance estimation with confidence.
/// Does NOT claim exact physical distance without calibration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProximityBucket {
    /// Very close (RCPI typically > 90)
    Immediate,
    /// In the same room (RCPI typically 60-90)
    Near,
    /// A few rooms away (RCPI typically 30-60)
    Far,
    /// At the edge of coverage (RCPI typically < 30)
    Edge,
    /// Unknown / no signal
    Unknown,
}

impl ProximityBucket {
    /// Estimate proximity from raw RCPI value (0-127, MediaTek scale).
    /// These thresholds are preliminary and should be refined with
    /// real calibration data.
    pub fn from_rcpi(rcpi: Option<i64>) -> Self {
        match rcpi {
            Some(r) if r >= 90 => ProximityBucket::Immediate,
            Some(r) if r >= 60 => ProximityBucket::Near,
            Some(r) if r >= 30 => ProximityBucket::Far,
            Some(_) => ProximityBucket::Edge,
            None => ProximityBucket::Unknown,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ProximityBucket::Immediate => "immediate",
            ProximityBucket::Near => "near",
            ProximityBucket::Far => "far",
            ProximityBucket::Edge => "edge",
            ProximityBucket::Unknown => "unknown",
        }
    }
}

/// Confidence level for proximity estimation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProximityConfidence {
    High,
    Medium,
    Low,
    None,
}

impl ProximityConfidence {
    /// Confidence is based on whether calibration data exists and
    /// how many samples were used.
    pub fn from_calibration(sample_count: usize) -> Self {
        match sample_count {
            n if n >= 20 => ProximityConfidence::High,
            n if n >= 10 => ProximityConfidence::Medium,
            n if n >= 3 => ProximityConfidence::Low,
            _ => ProximityConfidence::None,
        }
    }
}

/// Proximity estimate with confidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProximityEstimate {
    pub bucket: ProximityBucket,
    pub confidence: ProximityConfidence,
    pub raw_signal: Option<i64>,
    pub raw_signal_type: String,
    pub calibration_samples: usize,
}

impl ProximityEstimate {
    /// Estimate proximity from a raw RCPI value.
    /// Without calibration data, confidence is None.
    pub fn from_device(device: &Device, calibration_samples: usize) -> Self {
        let rcpi = device.rssi;
        Self {
            bucket: ProximityBucket::from_rcpi(rcpi),
            confidence: ProximityConfidence::from_calibration(calibration_samples),
            raw_signal: rcpi,
            raw_signal_type: "rcpi".to_string(),
            calibration_samples,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationSample {
    pub sample_id: String,
    pub collected_at: i64,
    pub device_id: String,
    pub band: Band,
    pub radio_id: String,
    pub known_distance_m: f32,
    pub raw_signal_strength: i64,
    pub smoothed_signal_strength: Option<f64>,
    pub signal_level: Option<u8>,
    pub noise: Option<u64>,
    pub signal_delta: Option<f64>,
    pub orientation: String,
    pub environment: String,
    pub tx_rate: Option<u64>,
    pub rx_rate: Option<u64>,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationSession {
    pub session_id: String,
    pub started_at: i64,
    pub environment: String,
    pub device_id: String,
    pub band: Band,
    pub radio_id: String,
    pub distance_positions: Vec<f32>,
}

pub struct Calibrator {
    session: CalibrationSession,
    samples: Vec<CalibrationSample>,
}

impl Calibrator {
    pub fn new(session: CalibrationSession) -> Self {
        Self { session, samples: Vec::new() }
    }

    pub fn record(&mut self, device: &Device, raw_signal: i64, smoothed: Option<f64>, distance_m: f32, orientation: &str) {
        let device_id = device.identity();
        let radio_id = device.radio_mac.clone().unwrap_or_default();
        let band = Band::Unknown; // Derive from radio config in real implementation
        let sample = CalibrationSample {
            sample_id: uuid_like(),
            collected_at: now(),
            device_id,
            band,
            radio_id,
            known_distance_m: distance_m,
            raw_signal_strength: raw_signal,
            smoothed_signal_strength: smoothed,
            signal_level: device.signal_level.map(|v| v as u8),
            noise: device.noise,
            signal_delta: None,
            orientation: orientation.to_string(),
            environment: self.session.environment.clone(),
            tx_rate: device.tx_rate,
            rx_rate: device.rx_rate,
            session_id: self.session.session_id.clone(),
        };
        self.samples.push(sample);
    }

    pub fn samples(&self) -> &[CalibrationSample] {
        &self.samples
    }

    pub fn summary(&self) -> CalibrationSummary {
        let mut per_dist = std::collections::HashMap::new();
        for s in &self.samples {
            let key = s.known_distance_m as u32; // use integer meters for grouping
            let e = per_dist.entry(key).or_insert(Vec::new());
            e.push(s.raw_signal_strength);
        }
        let stats = per_dist.into_iter().map(|(d, vals)| {
            let n = vals.len() as f64;
            let sum: i64 = vals.iter().sum();
            let mean = sum as f64 / n;
            let var = vals.iter().map(|v| (*v as f64 - mean).powi(2)).sum::<f64>() / n;
            (d as f32, mean, var.sqrt())
        }).collect();
        CalibrationSummary { per_distance_stats: stats }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CalibrationSummary {
    pub per_distance_stats: Vec<(f32, f64, f64)>, // distance, mean, stddev
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{:x}", n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_from_radio_mac_c1_is_2ghz() {
        assert_eq!(Band::from_radio_mac("aa:bb:cc:dd:ee:C1"), Band::Ghz2_4);
    }

    #[test]
    fn band_from_radio_mac_c3_is_5ghz() {
        assert_eq!(Band::from_radio_mac("aa:bb:cc:dd:ee:C3"), Band::Ghz5);
    }

    #[test]
    fn band_from_radio_mac_unknown_is_unknown() {
        assert_eq!(Band::from_radio_mac("aa:bb:cc:dd:ee:ff"), Band::Unknown);
    }

    #[test]
    fn proximity_bucket_from_rcpi() {
        assert_eq!(ProximityBucket::from_rcpi(Some(100)), ProximityBucket::Immediate);
        assert_eq!(ProximityBucket::from_rcpi(Some(75)), ProximityBucket::Near);
        assert_eq!(ProximityBucket::from_rcpi(Some(45)), ProximityBucket::Far);
        assert_eq!(ProximityBucket::from_rcpi(Some(10)), ProximityBucket::Edge);
        assert_eq!(ProximityBucket::from_rcpi(None), ProximityBucket::Unknown);
    }

    #[test]
    fn proximity_confidence_from_samples() {
        assert_eq!(ProximityConfidence::from_calibration(25), ProximityConfidence::High);
        assert_eq!(ProximityConfidence::from_calibration(15), ProximityConfidence::Medium);
        assert_eq!(ProximityConfidence::from_calibration(5), ProximityConfidence::Low);
        assert_eq!(ProximityConfidence::from_calibration(0), ProximityConfidence::None);
    }

    #[test]
    fn proximity_bucket_labels() {
        assert_eq!(ProximityBucket::Immediate.label(), "immediate");
        assert_eq!(ProximityBucket::Near.label(), "near");
        assert_eq!(ProximityBucket::Unknown.label(), "unknown");
    }
}
