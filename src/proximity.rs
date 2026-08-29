//! Proximity Engine (M6+).
//!
//! Turns raw signal observations (RCPI 0-127 from the EX520 firmware or dBm
//! from site-survey) plus band and recent history into stable, privacy-safe
//! proximity estimates.
//!
//! Output for every observed identity:
//!   - `ProximityZone`: immediate / near / medium / far / edge / unknown
//!   - `ProximityTrend`: approaching / away / stable / unknown
//!   - `Heat`: 0-100 temperature-like scale for UI (cold = weak/far, hot = strong/near)
//!   - `DistanceEstimate` in meters (always with explicit confidence)
//!   - `rssi_dbm` converted from RCPI when needed
//!
//! The engine is stateful (per-identity signal history) but bounded: the default
//! window is 10 samples, so memory stays small on the EX520.
//!
//! Privacy: it operates on identity strings that are already pseudonyms or
//! locally-derived keys; no raw MAC is required.

use crate::calibrate::{self, rcpi_to_dbm, Band, DistanceProfile};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

/// Default per-device history window.
///
/// Kept small (3) so the smoothed zone/distance track a real signal change in
/// 1-2 polls (the EX520's `signalStrength` is coarse and refresh-cadence is
/// tied to client activity, so a large window would add tens of seconds lag).
pub const DEFAULT_HISTORY_WINDOW: usize = 3;
/// Default EMA smoothing factor.
///
/// Raised (0.7) so the zone/trend react quickly to a genuine signal change.
pub const DEFAULT_EMA_ALPHA: f64 = 0.7;
/// Minimum dB change to declare a trend (avoid noise).
///
/// Kept small (1.0) so the trend flips as soon as the device genuinely starts
/// moving (the EX520 signal is coarse; a 2 dB threshold lagged the detection).
pub const DEFAULT_TREND_DELTA_DB: f64 = 1.0;
/// Minimum samples before trend detection is reported.
pub const DEFAULT_TREND_MIN_SAMPLES: usize = 2;

/// Source scale of the raw signal value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignalType {
    /// MediaTek RCPI 0-127 (higher = stronger on this firmware).
    Rcpi,
    /// dBm (already converted).
    Dbm,
}

impl Default for SignalType {
    fn default() -> Self {
        SignalType::Rcpi
    }
}

/// Coarse proximity zone relative to the sensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProximityZone {
    /// Very close, typically < 2 m.
    Immediate,
    /// Same room, 2-7 m.
    Near,
    /// Nearby room / 7-20 m.
    Medium,
    /// Several rooms away / 20-50 m.
    Far,
    /// At the edge of coverage, > 50 m or very weak.
    Edge,
    /// No signal or unclassifiable.
    Unknown,
}

impl Default for ProximityZone {
    fn default() -> Self {
        ProximityZone::Unknown
    }
}

impl ProximityZone {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProximityZone::Immediate => "immediate",
            ProximityZone::Near => "near",
            ProximityZone::Medium => "medium",
            ProximityZone::Far => "far",
            ProximityZone::Edge => "edge",
            ProximityZone::Unknown => "unknown",
        }
    }

    /// Ordering rank used for a configurable "within radius" threshold:
    /// 0=Immediate, 1=Near, 2=Medium, 3=Far, 4=Edge, 5=Unknown.
    pub fn rank(&self) -> u8 {
        match self {
            ProximityZone::Immediate => 0,
            ProximityZone::Near => 1,
            ProximityZone::Medium => 2,
            ProximityZone::Far => 3,
            ProximityZone::Edge => 4,
            ProximityZone::Unknown => 5,
        }
    }

    /// Spanish UI label.
    pub fn es_label(&self) -> &'static str {
        match self {
            ProximityZone::Immediate => "inmediato",
            ProximityZone::Near => "cerca",
            ProximityZone::Medium => "medio",
            ProximityZone::Far => "lejos",
            ProximityZone::Edge => "borde",
            ProximityZone::Unknown => "desconocido",
        }
    }
}

/// Trend of the signal over recent observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProximityTrend {
    /// Signal is getting stronger (device may be approaching).
    Approaching,
    /// Signal is getting weaker (device may be moving away).
    Away,
    /// Signal is stable within the configured delta.
    Stable,
    /// Not enough data to determine a trend.
    Unknown,
}

impl Default for ProximityTrend {
    fn default() -> Self {
        ProximityTrend::Unknown
    }
}

impl ProximityTrend {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProximityTrend::Approaching => "approaching",
            ProximityTrend::Away => "away",
            ProximityTrend::Stable => "stable",
            ProximityTrend::Unknown => "unknown",
        }
    }

    /// Spanish UI label.
    pub fn es_label(&self) -> &'static str {
        match self {
            ProximityTrend::Approaching => "acercándose",
            ProximityTrend::Away => "alejándose",
            ProximityTrend::Stable => "estable",
            ProximityTrend::Unknown => "desconocido",
        }
    }

    pub fn arrow(&self) -> &'static str {
        match self {
            ProximityTrend::Approaching => "\u{2191}", // ↑
            ProximityTrend::Away => "\u{2193}",        // ↓
            ProximityTrend::Stable => "\u{2192}",      // →
            ProximityTrend::Unknown => "?",
        }
    }
}

/// The output of the proximity engine for one identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProximityResult {
    pub zone: ProximityZone,
    pub trend: ProximityTrend,
    pub heat: u8,
    pub raw_signal: Option<i64>,
    pub signal_type: SignalType,
    pub rssi_dbm: Option<f64>,
    pub distance_m: Option<f32>,
    pub confidence: f32,
    pub calibrated: bool,
    pub band_mhz: Option<u32>,
    pub samples: usize,
}

impl ProximityResult {
    pub fn unknown() -> Self {
        Self {
            zone: ProximityZone::Unknown,
            trend: ProximityTrend::Unknown,
            heat: 0,
            raw_signal: None,
            signal_type: SignalType::Rcpi,
            rssi_dbm: None,
            distance_m: None,
            confidence: 0.0,
            calibrated: false,
            band_mhz: None,
            samples: 0,
        }
    }

    pub fn zone_label(&self) -> &'static str {
        self.zone.es_label()
    }

    pub fn trend_label(&self) -> &'static str {
        self.trend.es_label()
    }

    /// Human-readable combined label in Spanish, e.g. "cerca · acercándose".
    pub fn label(&self) -> String {
        format!("{} · {}", self.zone_label(), self.trend_label())
    }

    /// CSS class for the dashboard thermal scale.
    pub fn color_class(&self) -> &'static str {
        match self.heat {
            0..=20 => "proximity-cold",
            21..=40 => "proximity-cool",
            41..=60 => "proximity-warm",
            61..=80 => "proximity-hot",
            _ => "proximity-burning",
        }
    }
}

/// Tunables for the proximity engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProximityConfig {
    /// Number of recent samples kept per identity.
    pub history_window: usize,
    /// EMA smoothing factor (0.0 = no smoothing, 1.0 = only last sample).
    pub ema_alpha: f64,
    /// Minimum dB difference to declare approaching/away.
    pub trend_delta_db: f64,
    /// Minimum samples before trend is reported.
    pub trend_min_samples: usize,
    /// Distance profile for 2.4 GHz.
    pub profile_2_4: DistanceProfile,
    /// Distance profile for 5 GHz.
    pub profile_5: DistanceProfile,
}

impl Default for ProximityConfig {
    fn default() -> Self {
        Self {
            history_window: DEFAULT_HISTORY_WINDOW,
            ema_alpha: DEFAULT_EMA_ALPHA,
            trend_delta_db: DEFAULT_TREND_DELTA_DB,
            trend_min_samples: DEFAULT_TREND_MIN_SAMPLES,
            profile_2_4: DistanceProfile::uncalibrated(Band::Ghz2_4),
            profile_5: DistanceProfile::uncalibrated(Band::Ghz5),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct SignalHistory {
    /// (raw signal value, timestamp)
    samples: VecDeque<(i64, i64)>,
    /// EMA of dBm values.
    ema_dbm: Option<f64>,
    /// Last computed result (for lookups without recomputation).
    last_result: Option<ProximityResult>,
}

impl SignalHistory {
    /// Median of the stored samples converted to dBm.
    fn median_dbm(&self, signal_type: SignalType) -> Option<f64> {
        let mut dbms: Vec<f64> = self
            .samples
            .iter()
            .filter_map(|(s, _)| match signal_type {
                SignalType::Rcpi => rcpi_to_dbm(*s),
                SignalType::Dbm => Some(*s as f64),
            })
            .collect();
        if dbms.is_empty() {
            return None;
        }
        dbms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Some(dbms[dbms.len() / 2])
    }

    /// Number of valid samples (not sentinel/missing).
    fn valid_sample_count(&self, signal_type: SignalType) -> usize {
        self.samples
            .iter()
            .filter(|(s, _)| match signal_type {
                SignalType::Rcpi => calibrate::classify_rcpi(*s).is_usable(),
                SignalType::Dbm => *s >= -120 && *s <= 0,
            })
            .count()
    }
}

/// Proximity engine: per-identity signal history and classification.
pub struct ProximityEngine {
    config: ProximityConfig,
    histories: HashMap<String, SignalHistory>,
}

impl ProximityEngine {
    pub fn new(config: ProximityConfig) -> Self {
        Self {
            config,
            histories: HashMap::new(),
        }
    }

    pub fn default() -> Self {
        Self::new(ProximityConfig::default())
    }

    /// Feed a new observation and return the current best estimate.
    ///
    /// `identity` should be a stable per-device key (pseudonym, MAC, or IP).
    /// `raw_signal` is the firmware value; `signal_type` tells us how to convert it.
    /// `band` and `ts` are used for band-aware path-loss and trend timing.
    pub fn update(
        &mut self,
        identity: &str,
        raw_signal: Option<i64>,
        signal_type: SignalType,
        band: Band,
        ts: i64,
    ) -> ProximityResult {
        let mut result = ProximityResult::unknown();
        result.raw_signal = raw_signal;
        result.signal_type = signal_type;
        result.band_mhz = band.center_mhz();

        let dbm = raw_signal.and_then(|s| match signal_type {
            SignalType::Rcpi => {
                if calibrate::classify_rcpi(s).is_usable() {
                    rcpi_to_dbm(s)
                } else {
                    None
                }
            }
            SignalType::Dbm => Some(s as f64),
        });
        result.rssi_dbm = dbm;

        let Some(d) = dbm else {
            self.histories
                .entry(identity.to_string())
                .or_default()
                .last_result = Some(result.clone());
            return result;
        };

        let profile = match band {
            Band::Ghz5 => self.config.profile_5.clone(),
            _ => self.config.profile_2_4.clone(),
        };

        let hist = self.histories.entry(identity.to_string()).or_default();
        hist.samples.push_back((raw_signal.unwrap_or(-100), ts));
        if hist.samples.len() > self.config.history_window {
            hist.samples.pop_front();
        }

        let prev_ema = hist.ema_dbm;
        let next_ema = match prev_ema {
            Some(e) => self.config.ema_alpha * d + (1.0 - self.config.ema_alpha) * e,
            None => d,
        };
        hist.ema_dbm = Some(next_ema);

        // Use median of the window when we have enough samples; otherwise EMA.
        let smoothed_dbm = if hist.valid_sample_count(signal_type) >= self.config.trend_min_samples
        {
            hist.median_dbm(signal_type)
                .filter(|m| m.is_finite())
                .or(Some(next_ema))
        } else {
            Some(next_ema)
        };

        let used_dbm = smoothed_dbm.unwrap_or(d);
        let distance_m = profile.distance_m(used_dbm);
        result.distance_m = if distance_m.is_finite() && distance_m >= 0.0 {
            Some(distance_m as f32)
        } else {
            None
        };

        // Primary classification is dBm-based, because the uncalibrated
        // log-distance model overestimates distance on the EX520.  The distance
        // estimate is still exposed for diagnostics and future calibration.
        result.zone = zone_from_dbm(used_dbm, band);

        result.confidence =
            calibrate::confidence_score(used_dbm, hist.samples.len(), profile.calibrated);
        result.calibrated = profile.calibrated;
        result.samples = hist.samples.len();

        // Trend: compare current EMA with the previous one.
        result.trend = if hist.samples.len() >= self.config.trend_min_samples {
            if let Some(prev) = prev_ema {
                let delta = next_ema - prev;
                if delta > self.config.trend_delta_db {
                    ProximityTrend::Approaching
                } else if delta < -self.config.trend_delta_db {
                    ProximityTrend::Away
                } else {
                    ProximityTrend::Stable
                }
            } else {
                ProximityTrend::Unknown
            }
        } else {
            ProximityTrend::Unknown
        };

        result.heat = compute_heat(used_dbm);

        hist.last_result = Some(result.clone());
        result
    }

    /// Look up the last computed result for an identity without feeding a new sample.
    pub fn lookup(&self, identity: &str) -> Option<&ProximityResult> {
        self.histories
            .get(identity)
            .and_then(|h| h.last_result.as_ref())
    }

    /// Remove stale histories that have not been updated since `before_ts`.
    pub fn prune(&mut self, before_ts: i64) {
        self.histories
            .retain(|_, h| h.samples.back().map_or(true, |(_, ts)| *ts >= before_ts));
    }

    /// Number of tracked identities.
    pub fn len(&self) -> usize {
        self.histories.len()
    }

    pub fn is_empty(&self) -> bool {
        self.histories.is_empty()
    }
}

/// Classify a dBm value into a proximity zone.
///
/// These thresholds are intentionally conservative for the EX520's
/// uncalibrated MediaTek RCPI conversion.  They are not a physical distance
/// measure; they are a signal-strength proxy.
pub fn zone_from_dbm(dbm: f64, _band: Band) -> ProximityZone {
    // Future: apply a per-band offset once calibration data is available.
    if dbm >= -50.0 {
        ProximityZone::Immediate
    } else if dbm >= -70.0 {
        ProximityZone::Near
    } else if dbm >= -85.0 {
        ProximityZone::Medium
    } else if dbm >= -100.0 {
        ProximityZone::Far
    } else {
        ProximityZone::Edge
    }
}

/// Map dBm to a 0-100 thermal scale.
///
/// -100 dBm (very weak / far) -> 0
/// -50 dBm (very strong / near) -> 100
fn compute_heat(dbm: f64) -> u8 {
    let t = ((dbm + 100.0) / 50.0 * 100.0).clamp(0.0, 100.0);
    t as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ProximityConfig {
        ProximityConfig {
            history_window: 5,
            ema_alpha: 0.5,
            trend_delta_db: 1.5,
            trend_min_samples: 3,
            ..Default::default()
        }
    }

    #[test]
    fn unknown_when_no_signal() {
        let mut engine = ProximityEngine::new(test_config());
        let r = engine.update("aa", None, SignalType::Rcpi, Band::Ghz2_4, 1000);
        assert_eq!(r.zone, ProximityZone::Unknown);
        assert_eq!(r.trend, ProximityTrend::Unknown);
        assert_eq!(r.heat, 0);
        assert_eq!(r.confidence, 0.0);
    }

    #[test]
    fn invalid_rcpi_is_unknown() {
        let mut engine = ProximityEngine::new(test_config());
        let r = engine.update("aa", Some(-100), SignalType::Rcpi, Band::Ghz2_4, 1000);
        assert_eq!(r.zone, ProximityZone::Unknown);
    }

    #[test]
    fn strong_rcpi_is_medium_or_better() {
        let mut engine = ProximityEngine::new(test_config());
        // RCPI 127 -> -80 dBm with the default conversion.  That falls in the
        // "Medium" zone with the uncalibrated dBm thresholds.
        let r = engine.update("aa", Some(127), SignalType::Rcpi, Band::Ghz2_4, 1000);
        assert!(
            matches!(
                r.zone,
                ProximityZone::Medium | ProximityZone::Near | ProximityZone::Immediate
            ),
            "unexpected zone {:?}",
            r.zone
        );
        assert!(r.rssi_dbm.is_some());
        assert!(r.heat > 0);
    }

    #[test]
    fn dbm_is_used_directly() {
        let mut engine = ProximityEngine::new(test_config());
        let r = engine.update("aa", Some(-55), SignalType::Dbm, Band::Ghz2_4, 1000);
        assert_eq!(r.rssi_dbm, Some(-55.0));
        assert!(matches!(
            r.zone,
            ProximityZone::Near | ProximityZone::Medium | ProximityZone::Immediate
        ));
    }

    #[test]
    fn trend_approaching_when_signal_strengthens() {
        let mut engine = ProximityEngine::new(test_config());
        engine.update("aa", Some(-80), SignalType::Dbm, Band::Ghz2_4, 1000);
        engine.update("aa", Some(-78), SignalType::Dbm, Band::Ghz2_4, 1010);
        let r = engine.update("aa", Some(-55), SignalType::Dbm, Band::Ghz2_4, 1020);
        assert_eq!(r.trend, ProximityTrend::Approaching);
    }

    #[test]
    fn trend_away_when_signal_weakens() {
        let mut engine = ProximityEngine::new(test_config());
        engine.update("aa", Some(-55), SignalType::Dbm, Band::Ghz2_4, 1000);
        engine.update("aa", Some(-60), SignalType::Dbm, Band::Ghz2_4, 1010);
        let r = engine.update("aa", Some(-80), SignalType::Dbm, Band::Ghz2_4, 1020);
        assert_eq!(r.trend, ProximityTrend::Away);
    }

    #[test]
    fn heat_maps_dbm_to_0_100() {
        assert_eq!(compute_heat(-100.0), 0);
        assert_eq!(compute_heat(-50.0), 100);
        assert_eq!(compute_heat(-75.0), 50);
    }

    #[test]
    fn lookup_returns_last_result() {
        let mut engine = ProximityEngine::new(test_config());
        let r = engine.update("aa", Some(-60), SignalType::Dbm, Band::Ghz2_4, 1000);
        let looked = engine.lookup("aa").unwrap();
        assert_eq!(looked.zone, r.zone);
        assert_eq!(looked.heat, r.heat);
    }

    #[test]
    fn prune_removes_stale_histories() {
        let mut engine = ProximityEngine::new(test_config());
        engine.update("aa", Some(-60), SignalType::Dbm, Band::Ghz2_4, 1000);
        engine.update("bb", Some(-60), SignalType::Dbm, Band::Ghz2_4, 2000);
        engine.prune(1500);
        assert!(engine.lookup("aa").is_none());
        assert!(engine.lookup("bb").is_some());
    }
}
