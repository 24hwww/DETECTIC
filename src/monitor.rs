//! Monitor provider abstraction (M10-A).
//!
//! Provides a trait for discovering nearby (potentially non-associated) Wi-Fi
//! devices using legitimate firmware interfaces only. The default
//! `MediaTekMonitorProvider` uses `iwpriv get_site_survey` (documented in
//! M4.4) when available, and gracefully falls back to "no nearby data" if
//! the tool is unavailable or returns no useful rows.
//!
//! The provider NEVER:
//! - enables monitor mode on the AP
//! - changes BSS/channel configuration
//! - injects packets
//! - performs deauthentication
//! - relies on kernel exploits
//!
//! It only reads existing scan/survey output that the firmware already
//! produces for legitimate diagnostic purposes.

use crate::logging;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Source of a nearby observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NearbySource {
    /// Active probe response (not currently produced on stock firmware).
    Probe,
    /// Beacon from a nearby AP.
    Beacon,
    /// Site survey row (iwpriv get_site_survey).
    Survey,
}

impl Default for NearbySource {
    fn default() -> Self {
        NearbySource::Survey
    }
}

/// A single nearby (potentially non-associated) observation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NearbyObservation {
    pub mac: String,
    pub bssid: String,
    pub ssid: String,
    pub channel: u32,
    pub band: String,
    /// RSSI in dBm, converted from the raw signal percentage.
    pub rssi: Option<i64>,
    /// Raw signal percentage reported by the site survey.
    pub signal_pct: Option<u8>,
    /// Security mode (e.g. "WPA2PSK/AES").
    pub security: Option<String>,
    /// Wireless mode / PHY support (e.g. "11b/g/n/ax").
    pub w_mode: Option<String>,
    /// Extension channel (e.g. "NONE", "ABOVE", "BELOW").
    pub extch: Option<String>,
    pub timestamp: i64,
    pub source: NearbySource,
    /// Confidence in the observation [0.0, 1.0].
    pub confidence: f64,
}

/// Trait abstracting the nearby-device observation source.
pub trait MonitorProvider {
    /// Human-readable name for status/logging.
    fn name(&self) -> &str;

    /// Whether the provider is available on this hardware.
    fn available(&mut self) -> bool;

    /// Collect nearby observations for the given interface (or all).
    fn scan(&mut self) -> Vec<NearbyObservation>;

    /// Collect per-radio statistics via `iwpriv <ifname> stat`.
    /// Returns empty vec if not supported.
    fn radio_stats(&mut self) -> Vec<crate::snapshot::RadioStats> {
        Vec::new()
    }
}

/// MediaTek-based monitor provider using `iwpriv get_site_survey`.
///
/// On the EX520V (MT7981B), `iwpriv rai0 get_site_survey` and
/// `iwpriv rax0 get_site_survey` return a table of nearby APs. The output is
/// parsed conservatively; malformed rows are skipped.
pub struct MediaTekMonitorProvider {
    /// Interfaces to scan (e.g. ["rai0", "rax0"]).
    interfaces: Vec<String>,
    /// Cached availability flag.
    cached_available: Option<bool>,
}

impl MediaTekMonitorProvider {
    pub fn new() -> Self {
        // EX520V uses rai0 (2.4GHz) and rax0 (5GHz) per M4.4.
        Self {
            interfaces: vec!["rai0".into(), "rax0".into()],
            cached_available: None,
        }
    }

    pub fn with_interfaces(interfaces: Vec<String>) -> Self {
        Self {
            interfaces,
            cached_available: None,
        }
    }

    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// Parse a single `iwpriv <ifname> get_site_survey` output block.
    /// Returns nearby AP beacons (NOT associated stations).
    ///
    /// Two table layouts are supported:
    /// - `Ch  SSID  BSSID  Security  Signal(%)  W-Mode  ExtCH` (sample)
    /// - `No  Ch  SSID  BSSID  Security  Siganl(%)  W-Mode  ExtCH  ...`
    ///   (live EX520V output)
    fn parse_survey(&self, ifname: &str, output: &str) -> Vec<NearbyObservation> {
        let ts = Self::now();
        let band = if ifname.starts_with("rai") {
            "2.4GHz"
        } else if ifname.starts_with("rax") {
            "5GHz"
        } else {
            "unknown"
        };

        let mut out = Vec::new();
        let mut channel_col: Option<usize> = None;

        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Identify junk/separator lines. "====", "Total=..." and the table
            // of interface names in `iwconfig` output should not be processed.
            if trimmed.starts_with("====") || trimmed.starts_with("Total=") {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() < 4 {
                continue;
            }

            // Detect header and the column offset of the channel number.
            if channel_col.is_none() {
                if parts[0] == "No" || parts[0] == "NO" || parts[0] == "no" {
                    channel_col = Some(1);
                    continue;
                }
                if parts[0].starts_with("Ch") || parts[0] == "Channel" {
                    channel_col = Some(0);
                    continue;
                }
            }
            let ch_idx = channel_col.unwrap_or(0);
            if ch_idx >= parts.len() {
                continue;
            }

            let ch = match parts[ch_idx].parse::<u32>() {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Locate BSSID (token with exactly 5 colons).
            let bssid_idx = parts
                .iter()
                .enumerate()
                .find(|(_, p)| p.matches(':').count() == 5)
                .map(|(i, _)| i);
            let bssid_idx = match bssid_idx {
                Some(i) if i > ch_idx => i,
                _ => continue,
            };

            // Locate W-Mode (token like "11b/g/n" or "11a/n/ac/ax").
            // It starts with "11" and contains at least one '/'.
            let wmode_idx = parts
                .iter()
                .enumerate()
                .find(|(_, p)| p.starts_with("11") && p.contains('/'))
                .map(|(i, _)| i);
            let wmode_idx = match wmode_idx {
                Some(i) if i > bssid_idx + 1 => i,
                _ => continue,
            };

            // SSID is everything between the channel and the BSSID.
            let ssid = if ch_idx + 1 < bssid_idx {
                parts[ch_idx + 1..bssid_idx].join(" ")
            } else {
                String::new()
            };

            // Security is everything between BSSID and the signal token, which
            // sits immediately before W-Mode. Multiple tokens are joined with
            // "/" to match both "WPA2PSK/AES" and "WPA2PSK AES" variants.
            let security = if bssid_idx + 1 < wmode_idx - 1 {
                Some(parts[bssid_idx + 1..wmode_idx - 1].join("/"))
            } else {
                None
            };

            // Signal percentage is the token directly before W-Mode.
            let signal_pct_i64: Option<i64> = parts[wmode_idx - 1].parse::<i64>().ok();
            let signal_pct = signal_pct_i64.and_then(|p| {
                if p >= 0 && p <= 100 {
                    Some(p as u8)
                } else {
                    None
                }
            });
            let rssi = signal_pct_i64.map(|p| (p / 2) - 100);

            let w_mode = Some(parts[wmode_idx].to_string());
            let extch = parts.get(wmode_idx + 1).map(|s| s.to_string());

            out.push(NearbyObservation {
                mac: parts[bssid_idx].to_string(),
                bssid: parts[bssid_idx].to_string(),
                ssid,
                channel: ch,
                band: band.to_string(),
                rssi,
                signal_pct,
                security,
                w_mode,
                extch,
                timestamp: ts,
                source: NearbySource::Survey,
                confidence: 0.6,
            });
        }
        out
    }
}

impl Default for MediaTekMonitorProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MonitorProvider for MediaTekMonitorProvider {
    fn name(&self) -> &str {
        "mediatek_iwpriv_site_survey"
    }

    fn available(&mut self) -> bool {
        if let Some(v) = self.cached_available {
            return v;
        }
        // Check that at least one configured interface exists and iwpriv is present.
        let iwpriv_ok = Command::new("which")
            .arg("iwpriv")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        let v = iwpriv_ok
            && self.interfaces.iter().any(|ifname| {
                Command::new("iwpriv")
                    .arg(ifname)
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false)
            });
        self.cached_available = Some(v);
        v
    }

    fn scan(&mut self) -> Vec<NearbyObservation> {
        if !self.available() {
            logging::debug("monitor_provider_unavailable");
            return Vec::new();
        }
        let mut all = Vec::new();
        for ifname in &self.interfaces {
            let output = Command::new("iwpriv")
                .arg(ifname)
                .arg("get_site_survey")
                .output();
            match output {
                Ok(o) if o.status.success() => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let rows = self.parse_survey(ifname, &stdout);
                    logging::debug(&format!(
                        "monitor_scan ifname={} rows={}",
                        ifname,
                        rows.len()
                    ));
                    all.extend(rows);
                }
                Ok(o) => {
                    let err = String::from_utf8_lossy(&o.stderr);
                    logging::debug(&format!(
                        "monitor_scan_failed ifname={} err={}",
                        ifname,
                        err.chars().take(80).collect::<String>()
                    ));
                }
                Err(e) => {
                    logging::debug(&format!("monitor_spawn_failed ifname={} err={}", ifname, e));
                }
            }
        }
        all
    }
    fn radio_stats(&mut self) -> Vec<crate::snapshot::RadioStats> {
        // Attempt the read per interface regardless of the cached availability
        // probe (`iwpriv <if>` no-arg), because `iwpriv <if> stat` itself is a
        // proven read-only command. Keep only successful outputs.
        let mut all = Vec::new();
        for ifname in &self.interfaces {
            let output = Command::new("iwpriv").arg(ifname).arg("stat").output();
            match output {
                Ok(o) if o.status.success() => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let stats = self.parse_radio_stats(ifname, &stdout);
                    logging::debug(&format!(
                        "radio_stats ifname={} fields={}",
                        ifname,
                        stats.is_empty()
                    ));
                    all.extend(stats);
                }
                Ok(o) => {
                    let err = String::from_utf8_lossy(&o.stderr);
                    logging::debug(&format!(
                        "radio_stats_failed ifname={} err={}",
                        ifname,
                        err.chars().take(80).collect::<String>()
                    ));
                }
                Err(e) => {
                    logging::debug(&format!(
                        "radio_stats_spawn_failed ifname={} err={}",
                        ifname, e
                    ));
                }
            }
        }
        all
    }
}

impl MediaTekMonitorProvider {
    /// Parse `iwpriv <ifname> stat` output into RadioStats.
    /// The output format varies by MediaTek driver version; we extract
    /// common fields conservatively and skip unknown lines.
    fn parse_radio_stats(&self, ifname: &str, output: &str) -> Vec<crate::snapshot::RadioStats> {
        let mut stats = crate::snapshot::RadioStats::default();
        stats.interface = ifname.to_string();
        stats.band = if ifname.contains("rax") {
            Some("5GHz".into())
        } else if ifname.contains("rai") {
            Some("2.4GHz".into())
        } else {
            None
        };

        for line in output.lines() {
            let line = line.trim();
            // Common MediaTek stat formats:
            //   "Temperature: 45"
            //   "Tx success: 12345"
            //   "Tx fail: 67"
            //   "Rx success: 67890"
            //   "Rx CRC: 12"
            //   "Noise Floor: -95"
            if let Some(v) = Self::extract_int(line, "Temperature") {
                stats.temperature = Some(v);
            } else if let Some(v) = Self::extract_u64(line, "Tx success") {
                stats.tx_success = Some(v);
            } else if let Some(v) = Self::extract_u64(line, "Tx fail") {
                stats.tx_fail = Some(v);
            } else if let Some(v) = Self::extract_u64(line, "Rx success") {
                stats.rx_success = Some(v);
            } else if let Some(v) = Self::extract_u64(line, "Rx CRC") {
                stats.rx_crc = Some(v);
            } else if let Some(v) = Self::extract_int(line, "Noise Floor") {
                stats.noise_floor_dbm = Some(v);
            } else if let Some(v) = Self::parse_rssi_per_chain(line) {
                stats.rssi_per_chain = v;
            }
        }

        // Only return if we found at least one field
        if stats.temperature.is_some()
            || stats.tx_success.is_some()
            || stats.rx_success.is_some()
            || stats.noise_floor_dbm.is_some()
            || !stats.rssi_per_chain.is_empty()
        {
            vec![stats]
        } else {
            Vec::new()
        }
    }

    /// Parse the `Rssi: -54 -42 -109 -109` line of `iwpriv <if> stat` into the
    /// per-chain RSSI vector. Tolerant of absent/malformed input: returns `None`
    /// (leaving the vector empty) unless at least one integer is parsed after the
    /// `Rssi:` field prefix.
    fn parse_rssi_per_chain(line: &str) -> Option<Vec<i64>> {
        let lower = line.to_lowercase();
        let idx = lower.find("rssi:")?;
        let rest = &line[idx + 5..];
        let values: Vec<i64> = rest
            .split_whitespace()
            .filter_map(|tok| tok.trim().parse::<i64>().ok())
            .collect();
        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }

    fn extract_int(line: &str, key: &str) -> Option<i64> {
        if line.to_lowercase().contains(&key.to_lowercase()) {
            line.split(':')
                .nth(1)
                .and_then(|s| s.trim().parse::<i64>().ok())
        } else {
            None
        }
    }

    fn extract_u64(line: &str, key: &str) -> Option<u64> {
        if line.to_lowercase().contains(&key.to_lowercase()) {
            line.split(':')
                .nth(1)
                .and_then(|s| s.trim().parse::<u64>().ok())
        } else {
            None
        }
    }
}

/// Null provider for environments without any monitor capability.
pub struct NullMonitorProvider;

impl MonitorProvider for NullMonitorProvider {
    fn name(&self) -> &str {
        "null"
    }
    fn available(&mut self) -> bool {
        false
    }
    fn scan(&mut self) -> Vec<NearbyObservation> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SURVEY: &str =
        "Ch   SSID             BSSID               Security        Signal(%)  W-Mode    ExtCH
 1   Juliana          64:61:40:41:e0:e0   WPA2PSK/AES     13         11b/g/n/ax NONE
 6   REYES            3c:6a:d2:5f:ab:c1   WPA2PSK/AES     87         11b/g/n/ax NONE
 40  REYES_5G         3c:6a:d2:5f:ab:c3   WPA2PSK/AES     92         11a/n/ac/ax NONE
";

    #[test]
    fn parses_site_survey_rows() {
        let p = MediaTekMonitorProvider::with_interfaces(vec!["rai0".into()]);
        let rows = p.parse_survey("rai0", SAMPLE_SURVEY);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].channel, 1);
        assert_eq!(rows[0].ssid, "Juliana");
        assert_eq!(rows[0].bssid, "64:61:40:41:e0:e0");
        assert_eq!(rows[0].band, "2.4GHz");
        assert_eq!(rows[0].source, NearbySource::Survey);
        // 13% -> (13/2)-100 = -94
        assert_eq!(rows[0].rssi, Some(-94));
        assert_eq!(rows[0].signal_pct, Some(13));
        assert_eq!(rows[0].security.as_deref(), Some("WPA2PSK/AES"));
        assert_eq!(rows[0].w_mode.as_deref(), Some("11b/g/n/ax"));
        assert_eq!(rows[0].extch.as_deref(), Some("NONE"));

        let five = rows.iter().find(|r| r.ssid == "REYES_5G").unwrap();
        // In the single-interface test we use rai0, so the parser tags the row
        // with that band even though channel 40 is normally 5 GHz.
        assert_eq!(five.channel, 40);
        assert_eq!(five.signal_pct, Some(92));
        assert_eq!(five.w_mode.as_deref(), Some("11a/n/ac/ax"));
    }

    #[test]
    fn skips_header_and_empty_lines() {
        let p = MediaTekMonitorProvider::with_interfaces(vec!["rai0".into()]);
        let rows = p.parse_survey("rai0", "Ch SSID BSSID\n\n");
        assert!(rows.is_empty());
    }

    const LIVE_SURVEY: &str = "No  Ch  SSID                             BSSID               Security        Siganl(%)  W-Mode      ExtCH  NT WPS WPS2 WSC\n\
0   1   REYES                            3c:6a:d2:5f:ab:c1   WPA2PSK/AES     15         11b/g/n     NONE   In 0   YES   NO   NO\n\
1   6   Guest                            11:22:33:44:55:66   WPA2/AES        87         11b/g/n/ax  ABOVE  In 9   YES   NO   NO\n";

    #[test]
    fn parses_live_ex520_site_survey_layout() {
        let p = MediaTekMonitorProvider::with_interfaces(vec!["rax0".into()]);
        let rows = p.parse_survey("rax0", LIVE_SURVEY);
        assert_eq!(rows.len(), 2);

        assert_eq!(rows[0].channel, 1);
        assert_eq!(rows[0].ssid, "REYES");
        assert_eq!(rows[0].bssid, "3c:6a:d2:5f:ab:c1");
        assert_eq!(rows[0].band, "5GHz"); // from interface name
        assert_eq!(rows[0].signal_pct, Some(15));
        assert_eq!(rows[0].rssi, Some(-93)); // 15/2 - 100 = -93
        assert_eq!(rows[0].security.as_deref(), Some("WPA2PSK/AES"));
        assert_eq!(rows[0].w_mode.as_deref(), Some("11b/g/n"));
        assert_eq!(rows[0].extch.as_deref(), Some("NONE"));

        assert_eq!(rows[1].channel, 6);
        assert_eq!(rows[1].ssid, "Guest");
        assert_eq!(rows[1].signal_pct, Some(87));
        assert_eq!(rows[1].w_mode.as_deref(), Some("11b/g/n/ax"));
        assert_eq!(rows[1].extch.as_deref(), Some("ABOVE"));
    }

    #[test]
    fn null_provider_returns_empty() {
        let mut p = NullMonitorProvider;
        assert!(!p.available());
        assert!(p.scan().is_empty());
    }

    #[test]
    fn parses_rssi_per_chain_four_chains() {
        let p = MediaTekMonitorProvider::with_interfaces(vec!["rai0".into()]);
        let stats = p.parse_radio_stats(
            "rai0",
            "CurrentTemperature              = 41\n\
             Tx success                      = 3448\n\
             Rssi: -54 -42 -109 -109\n",
        );
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].interface, "rai0");
        assert_eq!(stats[0].band.as_deref(), Some("2.4GHz"));
        assert_eq!(stats[0].rssi_per_chain, vec![-54, -42, -109, -109]);
    }

    #[test]
    fn parses_rssi_per_chain_variable_chains() {
        let p = MediaTekMonitorProvider::with_interfaces(vec!["rax0".into()]);
        let stats = p.parse_radio_stats("rax0", "Rssi: -51 -47\n");
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].band.as_deref(), Some("5GHz"));
        assert_eq!(stats[0].rssi_per_chain, vec![-51, -47]);
    }

    #[test]
    fn missing_or_malformed_rssi_line_leaves_empty() {
        let p = MediaTekMonitorProvider::with_interfaces(vec!["rai0".into()]);
        // A colon-based field present but no Rssi line -> returns the stats with
        // an empty per-chain vector (no fabrication), no crash.
        let stats = p.parse_radio_stats("rai0", "Noise Floor: -95\n");
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].noise_floor_dbm, Some(-95));
        assert!(stats[0].rssi_per_chain.is_empty());
        // Malformed Rssi line (no integers) + no other parseable field -> empty,
        // no crash.
        let stats2 = p.parse_radio_stats("rai0", "Rssi: abc\n");
        assert!(stats2.is_empty());
        // Partial malformed tokens: only the valid integers are kept.
        let stats3 = p.parse_radio_stats("rai0", "Rssi: xx -54 yy -42\n");
        assert_eq!(stats3[0].rssi_per_chain, vec![-54, -42]);
    }
}
