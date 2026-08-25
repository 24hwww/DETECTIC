//! Driver provider abstraction (M11-A).
//!
//! Provides a trait for discovering the Wi-Fi driver capabilities of the
//! underlying hardware. The runtime automatically selects the best provider
//! at startup based on a fixed priority:
//!
//!   HAL  >  iwpriv  >  GTPR  >  Null
//!
//! Selection is *capability-aware*: among the providers that are present, the
//! one exposing the most verified capabilities wins (highest priority is the
//! tie-breaker). This means that the mere presence of the MediaTek HAL library
//! does not hide the GTPR/iwpriv capabilities — the HAL is only selected when
//! it actually exposes more than the alternatives.
//!
//! Each provider reports which capabilities it can expose. The runtime never
//! panics if a provider is unavailable — it falls back to the next one.
//!
//! ## Capabilities investigated (M11-B)
//!
//! - `AssociatedStations` — connected devices (PROVEN via GTPR)
//! - `NearbyApSurvey` — site survey beacons (PROVEN via iwpriv `get_site_survey`)
//! - `UnassociatedStations` — probe requests / non-associated STA RSSI
//!   (**NOT SUPPORTED** on stock EX520V — see `m11_boot_mechanisms.md` and the
//!   live read-only probe of `DEV2_WIFI_DE_UNASSOCSTA` which returns 9003)
//! - `RadioStats` — temperature, PER, per-antenna RSSI (PROVEN via iwpriv `stat`)
//! - `RealtimeEvents` — kernel/driver event callbacks (**NOT SUPPORTED**;
//!   `wlNetlinkTool` consumes events internally and exposes no API)
//!
//! All capability claims are backed by experimental evidence. No capability is
//! reported as available unless it has been tested on real hardware.

use serde::{Deserialize, Serialize};
use std::process::Command;

/// A specific driver capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverCapability {
    /// Associated stations (connected devices) with MAC, RSSI, rates.
    AssociatedStations,
    /// Nearby AP beacons via site survey.
    NearbyApSurvey,
    /// Unassociated station probe requests with RSSI.
    UnassociatedStations,
    /// Radio statistics (temperature, PER, per-antenna RSSI).
    RadioStats,
    /// Real-time kernel/driver event callbacks.
    RealtimeEvents,
}

/// Capability status reported by a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    /// Capability is available and has been experimentally verified.
    Available,
    /// Capability is not available on this hardware/firmware.
    Unavailable,
    /// Capability has not been tested yet.
    Unknown,
}

/// A probe-request observation (M11-B).
///
/// Only populated if `UnassociatedStations` is `Available`. On stock EX520V
/// firmware this capability is `Unavailable`, so this struct is never
/// instantiated with real data on that platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeObservation {
    pub mac: String,
    pub rssi: Option<i64>,
    pub channel: Option<u32>,
    pub band: Option<String>,
    pub timestamp: i64,
    pub source: String,
    pub confidence: f64,
}

impl ProbeObservation {
    /// An empty/invalid placeholder used when the capability is unavailable.
    /// Never constructs a fabricated observation.
    pub fn none() -> Self {
        ProbeObservation {
            mac: String::new(),
            rssi: None,
            channel: None,
            band: None,
            timestamp: 0,
            source: "none".into(),
            confidence: 0.0,
        }
    }

    /// True when this observation carries no real data.
    pub fn is_empty(&self) -> bool {
        self.mac.is_empty() || self.source == "none"
    }
}

/// Trait abstracting the Wi-Fi driver data source.
pub trait DriverProvider: Send {
    /// Human-readable name for status/logging.
    fn name(&self) -> &str;

    /// Priority (higher = preferred). Used for automatic selection tie-breaks.
    fn priority(&self) -> u8;

    /// Whether the provider is available on this hardware.
    fn available(&self) -> bool;

    /// Query a specific capability.
    fn capability(&self, cap: DriverCapability) -> CapabilityStatus;

    /// Return all capabilities and their statuses.
    fn capabilities(&self) -> Vec<(DriverCapability, CapabilityStatus)> {
        let all = [
            DriverCapability::AssociatedStations,
            DriverCapability::NearbyApSurvey,
            DriverCapability::UnassociatedStations,
            DriverCapability::RadioStats,
            DriverCapability::RealtimeEvents,
        ];
        all.iter().map(|&c| (c, self.capability(c))).collect()
    }

    /// Number of capabilities reported `Available` (selection heuristic).
    fn available_count(&self) -> usize {
        self.capabilities()
            .iter()
            .filter(|(_, s)| *s == CapabilityStatus::Available)
            .count()
    }

    /// Collect probe observations (unassociated stations).
    /// Returns empty if `UnassociatedStations` is `Unavailable`.
    fn probe_observations(&mut self) -> Vec<ProbeObservation> {
        Vec::new()
    }
}

/// GTPR-based driver provider.
///
/// Uses the GTPR/GDPR API for associated stations. Does not expose
/// unassociated stations or realtime events.
pub struct GtprProvider {
    cached_available: std::cell::Cell<Option<bool>>,
}

impl GtprProvider {
    pub fn new() -> Self {
        Self {
            cached_available: std::cell::Cell::new(None),
        }
    }
}

impl Default for GtprProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DriverProvider for GtprProvider {
    fn name(&self) -> &str {
        "gtpr"
    }

    fn priority(&self) -> u8 {
        10
    }

    fn available(&self) -> bool {
        if let Some(v) = self.cached_available.get() {
            return v;
        }
        // GTPR is available if the router URL is configured (env or default).
        let v = std::env::var("DETECTIC_URL").map(|u| !u.is_empty()).unwrap_or(true);
        self.cached_available.set(Some(v));
        v
    }

    fn capability(&self, cap: DriverCapability) -> CapabilityStatus {
        match cap {
            DriverCapability::AssociatedStations => CapabilityStatus::Available,
            DriverCapability::NearbyApSurvey => CapabilityStatus::Unavailable,
            DriverCapability::UnassociatedStations => CapabilityStatus::Unavailable,
            DriverCapability::RadioStats => CapabilityStatus::Unavailable,
            DriverCapability::RealtimeEvents => CapabilityStatus::Unavailable,
        }
    }
}

/// MediaTek HAL driver provider.
///
/// The MediaTek HAL (`libplatform_api.so`) exists on the EX520V, but it does
/// not expose a user-accessible probe-request capture interface on stock
/// firmware. This provider reports `Unavailable` for unassociated stations and
/// is therefore only selected when it would expose more than the alternatives
/// (it does not, so capability-aware selection skips it).
pub struct MediaTekHalProvider {
    cached_available: std::cell::Cell<Option<bool>>,
}

impl MediaTekHalProvider {
    pub fn new() -> Self {
        Self {
            cached_available: std::cell::Cell::new(None),
        }
    }

    /// Candidate HAL library paths on the EX520V stock firmware.
    fn hal_candidates() -> &'static [&'static str] {
        &[
            "/lib/libplatform_api.so",
            "/usr/lib/libplatform_api.so",
            "/lib/libhal.so",
            "/usr/lib/libmtk_hal.so",
        ]
    }

    fn hal_present(&self) -> bool {
        Self::hal_candidates()
            .iter()
            .any(|p| std::path::Path::new(p).exists())
    }
}

impl Default for MediaTekHalProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DriverProvider for MediaTekHalProvider {
    fn name(&self) -> &str {
        "mediatek_hal"
    }

    fn priority(&self) -> u8 {
        30
    }

    fn available(&self) -> bool {
        if let Some(v) = self.cached_available.get() {
            return v;
        }
        let v = self.hal_present();
        self.cached_available.set(Some(v));
        v
    }

    fn capability(&self, cap: DriverCapability) -> CapabilityStatus {
        match cap {
            // M11-B: Experimentally verified NOT SUPPORTED on stock EX520V.
            // The HAL does not expose a probe-request capture interface to
            // user-space. See investigations/m11_boot_mechanisms.md.
            DriverCapability::AssociatedStations => CapabilityStatus::Unavailable,
            DriverCapability::NearbyApSurvey => CapabilityStatus::Unavailable,
            DriverCapability::UnassociatedStations => CapabilityStatus::Unavailable,
            DriverCapability::RadioStats => CapabilityStatus::Unavailable,
            DriverCapability::RealtimeEvents => CapabilityStatus::Unavailable,
        }
    }
}

/// MediaTek iwpriv driver provider.
///
/// Uses `iwpriv` private ioctls for radio stats and site survey. Does not
/// expose unassociated stations (the `get_mac_table` ioctl crashes, and
/// `DEV2_WIFI_DE_UNASSOCSTA` returns error 9003).
pub struct MediaTekIwprivProvider {
    cached_available: std::cell::Cell<Option<bool>>,
}

impl MediaTekIwprivProvider {
    pub fn new() -> Self {
        Self {
            cached_available: std::cell::Cell::new(None),
        }
    }
}

impl Default for MediaTekIwprivProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DriverProvider for MediaTekIwprivProvider {
    fn name(&self) -> &str {
        "mediatek_iwpriv"
    }

    fn priority(&self) -> u8 {
        20
    }

    fn available(&self) -> bool {
        if let Some(v) = self.cached_available.get() {
            return v;
        }
        let v = Command::new("which")
            .arg("iwpriv")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        self.cached_available.set(Some(v));
        v
    }

    fn capability(&self, cap: DriverCapability) -> CapabilityStatus {
        match cap {
            DriverCapability::AssociatedStations => CapabilityStatus::Unavailable,
            DriverCapability::NearbyApSurvey => CapabilityStatus::Available,
            // M11-B: `get_mac_table` crashes (segfault). `iwlist scan` not
            // supported. No probe-request interface. NOT SUPPORTED.
            DriverCapability::UnassociatedStations => CapabilityStatus::Unavailable,
            DriverCapability::RadioStats => CapabilityStatus::Available,
            DriverCapability::RealtimeEvents => CapabilityStatus::Unavailable,
        }
    }
}

/// Null driver provider — no capabilities.
pub struct NullDriverProvider;

impl DriverProvider for NullDriverProvider {
    fn name(&self) -> &str {
        "null"
    }
    fn priority(&self) -> u8 {
        0
    }
    fn available(&self) -> bool {
        true // always available as a fallback
    }
    fn capability(&self, _cap: DriverCapability) -> CapabilityStatus {
        CapabilityStatus::Unavailable
    }
}

/// Select the best available driver provider.
///
/// Priority: HAL > iwpriv > GTPR > Null. Never panics.
///
/// Selection is capability-aware: among providers that are `available()`, the
/// one exposing the most `Available` capabilities is chosen; `priority()` is the
/// tie-breaker so that, all else equal, HAL wins over iwpriv over GTPR.
pub fn select_best() -> Box<dyn DriverProvider> {
    let candidates: Vec<Box<dyn DriverProvider>> = vec![
        Box::new(MediaTekHalProvider::new()),
        Box::new(MediaTekIwprivProvider::new()),
        Box::new(GtprProvider::new()),
        Box::new(NullDriverProvider),
    ];

    let mut best: Option<Box<dyn DriverProvider>> = None;
    for c in candidates {
        if !c.available() {
            continue;
        }
        let better = match best.as_ref() {
            None => true,
            Some(b) => {
                let ac = c.available_count();
                let bc = b.available_count();
                ac > bc || (ac == bc && c.priority() > b.priority())
            }
        };
        if better {
            best = Some(c);
        }
    }
    best.unwrap_or_else(|| Box::new(NullDriverProvider))
}

/// Render a capability matrix for the `detectic driver` command.
pub fn capability_matrix(provider: &dyn DriverProvider) -> String {
    let mut s = format!("driver_provider: {}\n", provider.name());
    s.push_str(&format!("available: {}\n", provider.available()));
    s.push_str("capabilities:\n");
    for (cap, status) in provider.capabilities() {
        let cap_str = match cap {
            DriverCapability::AssociatedStations => "associated_stations",
            DriverCapability::NearbyApSurvey => "nearby_ap_survey",
            DriverCapability::UnassociatedStations => "unassociated_stations",
            DriverCapability::RadioStats => "radio_stats",
            DriverCapability::RealtimeEvents => "realtime_events",
        };
        let status_str = match status {
            CapabilityStatus::Available => "available",
            CapabilityStatus::Unavailable => "unavailable",
            CapabilityStatus::Unknown => "unknown",
        };
        s.push_str(&format!("  {}: {}\n", cap_str, status_str));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_provider_all_unavailable() {
        let p = NullDriverProvider;
        assert!(p.available());
        for (cap, status) in p.capabilities() {
            assert_eq!(
                status,
                CapabilityStatus::Unavailable,
                "{:?} should be unavailable",
                cap
            );
        }
    }

    #[test]
    fn gtpr_has_associated_stations() {
        let p = GtprProvider::new();
        assert_eq!(
            p.capability(DriverCapability::AssociatedStations),
            CapabilityStatus::Available
        );
        assert_eq!(
            p.capability(DriverCapability::UnassociatedStations),
            CapabilityStatus::Unavailable
        );
    }

    #[test]
    fn iwpriv_has_survey_and_stats() {
        let p = MediaTekIwprivProvider::new();
        assert_eq!(
            p.capability(DriverCapability::NearbyApSurvey),
            CapabilityStatus::Available
        );
        assert_eq!(
            p.capability(DriverCapability::RadioStats),
            CapabilityStatus::Available
        );
        assert_eq!(
            p.capability(DriverCapability::UnassociatedStations),
            CapabilityStatus::Unavailable
        );
    }

    #[test]
    fn hal_reports_unassociated_unavailable() {
        let p = MediaTekHalProvider::new();
        assert_eq!(
            p.capability(DriverCapability::UnassociatedStations),
            CapabilityStatus::Unavailable
        );
    }

    #[test]
    fn probe_observation_none_is_empty() {
        assert!(ProbeObservation::none().is_empty());
    }

    #[test]
    fn select_best_returns_a_provider_and_never_panics() {
        let p = select_best();
        assert!(!p.name().is_empty());
        // The chosen provider must always be available.
        assert!(p.available());
    }

    #[test]
    fn capability_matrix_is_nonempty() {
        let p = NullDriverProvider;
        let m = capability_matrix(&p);
        assert!(m.contains("driver_provider:"));
        assert!(m.contains("capabilities:"));
    }

    #[test]
    fn priority_ordering() {
        let hal = MediaTekHalProvider::new();
        let iwpriv = MediaTekIwprivProvider::new();
        let gtpr = GtprProvider::new();
        let null = NullDriverProvider;
        assert!(hal.priority() > iwpriv.priority());
        assert!(iwpriv.priority() > gtpr.priority());
        assert!(gtpr.priority() > null.priority());
    }
}
