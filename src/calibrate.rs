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
    pub fn from_radio_mac(_radio_mac: &str) -> Self {
        // Placeholder: actual mapping is environment-specific.
        // In production, derive from radio config or stack.
        Band::Unknown
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
