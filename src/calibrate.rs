//! Band-aware proximity calibration tooling.
//!
//! The EX520V reports MediaTek RCPI 0..127 via signalStrength.
//! We treat the native scale as primary and avoid false dBm conversion.
//! Calibration records samples at known distances for empirical relative proximity.
//!
//! The new RSSI distance model only produces estimates with explicit confidence.
//! It never claims FTM/PHY timing, never simulates FTM, and always preserves raw RCPI.

use crate::model::{Device, DistanceEstimate, ProximityBucket as ModelProximityBucket};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

/// Maximum valid RCPI per MediaTek (0-127 inclusive).
pub const RCPI_MAX: i64 = 127;
pub const RCPI_MIN: i64 = 0;

/// Quality classification for a raw RCPI value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RssiQuality {
    Valid,
    Missing,
    OutOfRange,
    Saturated,
    Sentinel,
}

impl RssiQuality {
    pub fn is_usable(&self) -> bool {
        matches!(self, RssiQuality::Valid | RssiQuality::Saturated)
    }
}

/// Classify a raw RCPI value.
pub fn classify_rcpi(rcpi: i64) -> RssiQuality {
    if rcpi == -100 {
        // Explicit sentinel observed in some pipelines (though not yet on EX520).
        RssiQuality::Sentinel
    } else if rcpi < RCPI_MIN || rcpi > RCPI_MAX {
        RssiQuality::OutOfRange
    } else if rcpi == RCPI_MAX {
        RssiQuality::Saturated
    } else if rcpi < 0 {
        RssiQuality::OutOfRange
    } else {
        RssiQuality::Valid
    }
}

/// Convert RCPI to estimated dBm using the linear mapping documented in
/// `investigations/rssi_semantics.md`.
///
/// `RSSI(dBm) ≈ -110 + (RCPI / 127) * 30`
///
/// This is a vendor-approximate, uncalibrated conversion. It returns `None` for
/// values that cannot represent a real RCPI, including the common `-100` sentinel.
pub fn rcpi_to_dbm(rcpi: i64) -> Option<f64> {
    if !classify_rcpi(rcpi).is_usable() {
        return None;
    }
    Some(-110.0 + (rcpi as f64 * 30.0) / RCPI_MAX as f64)
}

/// Convert RCPI noise to dBm if and only if the noise scale is known.
///
/// Current EX520 `noise` field also uses a vendor scale, but the exact mapping
/// to dBm has not been validated. Therefore this returns `None` and explicitly
/// refuses to produce a fake SNR.
pub fn noise_to_dbm(_noise_rcpi: u64) -> Option<f64> {
    None
}

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

    pub fn center_mhz(&self) -> Option<u32> {
        match self {
            Band::Ghz2_4 => Some(2400),
            Band::Ghz5 => Some(5000),
            Band::Unknown => None,
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

    pub fn as_str(&self) -> &'static str {
        match self {
            ProximityConfidence::High => "high",
            ProximityConfidence::Medium => "medium",
            ProximityConfidence::Low => "low",
            ProximityConfidence::None => "none",
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

    /// Fit a log-distance profile from the collected samples.
    /// Requires at least two distinct distances and ten total samples.
    /// Returns `None` if the data cannot support a meaningful fit.
    pub fn fit(&self) -> Option<DistanceProfile> {
        let mut points: Vec<(f64, f64)> = self
            .summary()
            .per_distance_stats
            .into_iter()
            .map(|(d, mean, _)| (d as f64, mean))
            .collect();
        if points.len() < 2 || self.samples.len() < 10 {
            return None;
        }
        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        // Convert per-distance mean RCPI to dBm before fitting. The fit uses
        // the log-distance model: rssi(d) = rssi0 - 10*n*log10(d/d0).
        let mut dbm_points: Vec<(f64, f64)> = points
            .iter()
            .filter_map(|(d, rcpi)| {
                let rcpi_i = (*rcpi as i64).clamp(RCPI_MIN, RCPI_MAX);
                rcpi_to_dbm(rcpi_i).map(|dbm| (*d, dbm))
            })
            .collect();
        if dbm_points.len() < 2 {
            return None;
        }
        dbm_points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        let (d0, rssi0_dbm) = dbm_points[0];
        let mut n_sum = 0.0;
        let mut n_count = 0usize;
        for i in 1..dbm_points.len() {
            let (d2, rssi2_dbm) = dbm_points[i];
            let ratio = d2 / d0;
            if ratio > 1.0 {
                let log_ratio = ratio.log10();
                if log_ratio.abs() > 1e-6 {
                    let n = (rssi0_dbm - rssi2_dbm) / (10.0 * log_ratio);
                    if n.is_finite() && n > 0.0 && n < 10.0 {
                        n_sum += n;
                        n_count += 1;
                    }
                }
            }
        }

        let n = if n_count > 0 { n_sum / n_count as f64 } else { 2.0 };
        Some(DistanceProfile {
            band: self.session.band,
            d0_m: d0,
            rssi0_dbm,
            n,
            calibrated: true,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CalibrationSummary {
    pub per_distance_stats: Vec<(f32, f64, f64)>, // distance, mean, stddev
}

/// Calibration profile for the log-distance path-loss model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistanceProfile {
    pub band: Band,
    pub d0_m: f64,
    pub rssi0_dbm: f64,
    pub n: f64,
    pub calibrated: bool,
}

impl DistanceProfile {
    /// Uncalibrated default: do not trust distance values, cap confidence.
    pub fn uncalibrated(band: Band) -> Self {
        Self {
            band,
            d0_m: 1.0,
            rssi0_dbm: -45.0, // educated placeholder only
            n: 2.2,
            calibrated: false,
        }
    }

    /// Log-distance path-loss: d = d0 * 10^((rssi0 - rssi) / (10*n))
    pub fn distance_m(&self, rssi_dbm: f64) -> f64 {
        log_distance_m(rssi_dbm, self.rssi0_dbm, self.n, self.d0_m)
    }
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{:x}", n)
}

/// Log-distance path-loss model.
///
/// `d = d0 * 10^((rssi0 - rssi) / (10*n))`
///
/// All dBm values may be from an uncalibrated RCPI conversion. The returned
/// distance is an inference, not a physical measurement.
pub fn log_distance_m(rssi_dbm: f64, rssi0_dbm: f64, n: f64, d0_m: f64) -> f64 {
    if n <= 0.0 || d0_m <= 0.0 {
        return f64::NAN;
    }
    d0_m * 10.0_f64.powf((rssi0_dbm - rssi_dbm) / (10.0 * n))
}

/// Map a distance in meters to the canonical model proximity bucket.
pub fn proximity_bucket_from_meters(m: f64) -> ModelProximityBucket {
    if m.is_nan() || m < 0.0 {
        ModelProximityBucket::Unknown
    } else if m <= 2.0 {
        ModelProximityBucket::VeryNear
    } else if m <= 7.0 {
        ModelProximityBucket::Near
    } else if m <= 20.0 {
        ModelProximityBucket::Medium
    } else if m <= 50.0 {
        ModelProximityBucket::Far
    } else {
        ModelProximityBucket::VeryFar
    }
}

/// Moving median over the last `window` samples.
/// Returns `None` for indices before `window` samples are available.
pub fn moving_median(samples: &[f64], window: usize) -> Vec<Option<f64>> {
    let mut out = Vec::with_capacity(samples.len());
    for (i, _) in samples.iter().enumerate() {
        let start = if i + 1 >= window { i + 1 - window } else { 0 };
        let slice = &samples[start..=i];
        if slice.len() < window {
            out.push(None);
        } else {
            let mut v = slice.to_vec();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            out.push(Some(v[v.len() / 2]));
        }
    }
    out
}

/// Exponential moving average.
/// The first output is the first input; subsequent outputs are EMA.
pub fn ema(samples: &[f64], alpha: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity(samples.len());
    let mut prev: Option<f64> = None;
    for &v in samples {
        let next = match prev {
            None => v,
            Some(p) => alpha * v + (1.0 - alpha) * p,
        };
        out.push(next);
        prev = Some(next);
    }
    out
}

/// Compute a 0.0-1.0 confidence score.
///
/// - Uncalibrated profiles are hard-capped at 0.5.
/// - Very weak signals (below noise floor effective) reduce confidence.
/// - Strong saturated signals also reduce confidence because the model becomes
///   insensitive and prone to underestimating very close distances.
/// - More samples increase confidence up to a calibrated maximum.
pub fn confidence_score(rssi_dbm: f64, sample_count: usize, calibrated: bool) -> f32 {
    let calibrated_cap = if calibrated { 0.95 } else { 0.5 };

    let signal_conf = if rssi_dbm.is_nan() {
        0.1
    } else if rssi_dbm <= -90.0 {
        0.2
    } else if rssi_dbm <= -75.0 {
        0.5
    } else if rssi_dbm <= -55.0 {
        0.85
    } else if rssi_dbm <= -40.0 {
        0.7
    } else {
        // Strong / saturated
        0.5
    };

    let sample_factor = (sample_count as f64 / 20.0).clamp(0.0, 1.0);
    let conf = calibrated_cap * (0.3 + 0.7 * signal_conf) * (0.5 + 0.5 * sample_factor);
    conf.min(calibrated_cap) as f32
}

/// Produce a `DistanceEstimate` from raw RCPI and an optional profile.
///
/// `raw_rssi_dbm` is the smoothed/converted dBm value to use. If `None`, the
/// function will attempt to convert `rcpi`.
pub fn estimate_distance(
    rcpi: i64,
    band: Band,
    raw_rssi_dbm: Option<f64>,
    profile: &DistanceProfile,
    sample_count: usize,
) -> DistanceEstimate {
    let raw = raw_rssi_dbm.or_else(|| rcpi_to_dbm(rcpi));
    let (rssi_dbm, bucket, estimated_m, confidence) = match raw {
        Some(db) if db.is_finite() => {
            let m = profile.distance_m(db);
            let bucket = proximity_bucket_from_meters(m);
            let conf = confidence_score(db, sample_count, profile.calibrated);
            (Some(db as f32), bucket, Some(m as f32), conf)
        }
        _ => {
            (None, ModelProximityBucket::Unknown, None, 0.0)
        }
    };

    DistanceEstimate {
        bucket,
        estimated_distance_m: estimated_m,
        rssi_dbm: rssi_dbm,
        confidence,
        calibrated: profile.calibrated,
        band_mhz: band.center_mhz(),
    }
}

/// A per-device log-distance estimator with EMA + moving-median smoothing.
///
/// Keeps lightweight state; never stores raw MACs (operates on pseudonym + RCPI).
pub struct LogDistanceEstimator {
    profile: DistanceProfile,
    alpha: f64,
    window: usize,
    ema: Option<f64>,
    rssi_history: VecDeque<f64>,
    sample_count: usize,
    last: Option<DistanceEstimate>,
}

impl LogDistanceEstimator {
    pub fn new(profile: DistanceProfile, alpha: f64, window: usize) -> Self {
        Self {
            profile,
            alpha,
            window,
            ema: None,
            rssi_history: VecDeque::with_capacity(window),
            sample_count: 0,
            last: None,
        }
    }

    /// Feed a raw RCPI observation. Returns a reference to the updated estimate.
    pub fn feed(&mut self, rcpi: i64, _ts: i64) -> Option<&DistanceEstimate> {
        let dbm = rcpi_to_dbm(rcpi)?;
        self.sample_count += 1;

        let next_ema = match self.ema {
            None => dbm,
            Some(prev) => self.alpha * dbm + (1.0 - self.alpha) * prev,
        };
        self.ema = Some(next_ema);

        self.rssi_history.push_back(next_ema);
        if self.rssi_history.len() > self.window {
            self.rssi_history.pop_front();
        }

        let filtered = if self.rssi_history.len() >= self.window {
            let mut v: Vec<f64> = self.rssi_history.iter().copied().collect();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            Some(v[v.len() / 2])
        } else {
            Some(next_ema)
        };

        let est = estimate_distance(rcpi, self.profile.band, filtered, &self.profile, self.sample_count);
        self.last = Some(est);
        self.last.as_ref()
    }

    /// Current best estimate.
    pub fn estimate(&self) -> Option<&DistanceEstimate> {
        self.last.as_ref()
    }

    pub fn sample_count(&self) -> usize {
        self.sample_count
    }
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

    #[test]
    fn rcpi_to_dbm_boundaries() {
        // RCPI 0 -> -110 dBm, 127 -> -80 dBm per rssi_semantics.md
        assert!((rcpi_to_dbm(0).unwrap() + 110.0).abs() < 0.01);
        assert!((rcpi_to_dbm(127).unwrap() + 80.0).abs() < 0.01);

        // A typical strong associated value
        let db = rcpi_to_dbm(104).unwrap();
        assert!(db < -80.0 && db > -90.0);
    }

    #[test]
    fn rcpi_invalid_values_rejected() {
        assert_eq!(rcpi_to_dbm(-100), None);
        assert_eq!(rcpi_to_dbm(-1), None);
        assert_eq!(rcpi_to_dbm(128), None);
        assert_eq!(rcpi_to_dbm(i64::MAX), None);
    }

    #[test]
    fn noise_to_dbm_returns_none() {
        assert_eq!(noise_to_dbm(50), None);
    }

    #[test]
    fn log_distance_math() {
        // At the reference RSSI the distance should equal d0.
        let d = log_distance_m(-60.0, -60.0, 2.0, 1.0);
        assert!((d - 1.0).abs() < 1e-6);

        // 20 dB weaker at n=2 should be 10x the reference distance.
        let d2 = log_distance_m(-80.0, -60.0, 2.0, 1.0);
        assert!((d2 - 10.0).abs() < 1e-6);

        // 40 dB weaker at n=4 should be ~10x.
        let d3 = log_distance_m(-100.0, -60.0, 4.0, 1.0);
        assert!((d3 - 10.0).abs() < 1e-6);
    }

    #[test]
    fn proximity_bucket_meters() {
        assert_eq!(proximity_bucket_from_meters(1.5), ModelProximityBucket::VeryNear);
        assert_eq!(proximity_bucket_from_meters(5.0), ModelProximityBucket::Near);
        assert_eq!(proximity_bucket_from_meters(15.0), ModelProximityBucket::Medium);
        assert_eq!(proximity_bucket_from_meters(30.0), ModelProximityBucket::Far);
        assert_eq!(proximity_bucket_from_meters(100.0), ModelProximityBucket::VeryFar);
        assert_eq!(proximity_bucket_from_meters(f64::NAN), ModelProximityBucket::Unknown);
    }

    #[test]
    fn ema_smoothing() {
        let v = vec![-80.0, -70.0, -90.0];
        let e = ema(&v, 0.5);
        assert!((e[0] - -80.0).abs() < 1e-6);
        // e[1] = 0.5 * -70 + 0.5 * -80 = -75
        assert!((e[1] - -75.0).abs() < 1e-6);
        // e[2] = 0.5 * -90 + 0.5 * -75 = -82.5
        assert!((e[2] - -82.5).abs() < 1e-6);
    }

    #[test]
    fn moving_median_rejects_spikes() {
        let v = vec![-80.0, -82.0, 100.0, -81.0, -83.0];
        let m = moving_median(&v, 5);
        // Once the window is full (index 4) the 5-sample median replaces the
        // spike 100 with the middle value after sorting.
        // Sorted: -83, -82, -81, -80, 100 -> median -81.
        assert_eq!(m[4], Some(-81.0));
    }

    #[test]
    fn confidence_uncalibrated_capped() {
        let c = confidence_score(-60.0, 20, false);
        assert!(c <= 0.5);
        assert!(c > 0.0);
    }

    #[test]
    fn confidence_calibrated_higher_than_uncalibrated() {
        let c_cal = confidence_score(-60.0, 20, true);
        let c_uncal = confidence_score(-60.0, 20, false);
        assert!(c_cal > c_uncal);
    }

    #[test]
    fn estimate_distance_with_and_without_calibration() {
        let uncal = DistanceProfile::uncalibrated(Band::Ghz2_4);
        let est = estimate_distance(104, Band::Ghz2_4, None, &uncal, 1);
        assert_eq!(est.calibrated, false);
        assert!(est.confidence <= 0.5);
        assert!(est.estimated_distance_m.is_some());

        let est2 = estimate_distance(-100, Band::Ghz2_4, None, &uncal, 0);
        assert_eq!(est2.bucket, ModelProximityBucket::Unknown);
        assert_eq!(est2.rssi_dbm, None);
    }

    #[test]
    fn log_distance_estimator_updates() {
        let profile = DistanceProfile::uncalibrated(Band::Ghz2_4);
        let mut est = LogDistanceEstimator::new(profile, 0.2, 5);
        for i in 0..10 {
            est.feed(100 + i as i64, i as i64 * 1000);
        }
        assert!(est.estimate().is_some());
        assert_eq!(est.sample_count(), 10);
    }

    #[test]
    fn calibrator_fit_requires_two_distances() {
        let session = CalibrationSession {
            session_id: "test".to_string(),
            started_at: 0,
            environment: "test".to_string(),
            device_id: "d".to_string(),
            band: Band::Ghz2_4,
            radio_id: "rai0".to_string(),
            distance_positions: vec![1.0, 5.0],
        };
        let mut c = Calibrator::new(session);
        for _ in 0..5 {
            let mut d = Device::default();
            d.mac = Some("aa:bb:cc:dd:ee:01".to_string());
            c.record(&d, 100, None, 1.0, "front");
        }
        // Not enough samples at second distance -> fit fails
        assert!(c.fit().is_none());

        for _ in 0..5 {
            let mut d = Device::default();
            d.mac = Some("aa:bb:cc:dd:ee:01".to_string());
            c.record(&d, 80, None, 5.0, "front");
        }
        let profile = c.fit().unwrap();
        assert!(profile.calibrated);
        assert!(profile.n > 0.0);
    }
}
