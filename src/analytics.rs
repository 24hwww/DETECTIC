//! Presence analytics — first/last seen, duration, recurrence, histograms.
//!
//! Reusable pure-Rust module. Operates on `DeviceStats`-like aggregates and on
//! raw snapshot observations. No I/O, no transport.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Per-device presence analytics (Phase F deliverable)
// ---------------------------------------------------------------------------

/// Extended presence analytics for one pseudonymized device.
/// Builds on `store::DeviceStats` but is usable without SQLite.
#[derive(Debug, Clone)]
pub struct PresenceStats {
    /// Pseudonym (HMAC-SHA256 hex, 64 chars).
    pub pseudonym: String,
    /// Earliest observation timestamp (epoch seconds).
    pub first_seen: i64,
    /// Latest observation timestamp.
    pub last_seen: i64,
    /// Total number of snapshot observations.
    pub observations: u64,
    /// Wall-clock visit duration: last_seen − first_seen (seconds, ≥0).
    pub visit_duration_secs: i64,
    /// Distinct calendar days on which the device was seen.
    pub distinct_days: usize,
    /// Recurrence score ∈ [0, 1]: distinct_days / total_days_in_window.
    /// 1.0 = seen every day in the window. Requires `window_days > 0`.
    pub recurrence_score: f64,
    /// Histogram: hour_of_day (0–23) → observation count.
    pub hour_histogram: [u64; 24],
    /// Histogram: weekday (0=Mon … 6=Sun) → observation count.
    pub weekday_histogram: [u64; 7],
    /// RSSI aggregates (None when no RSSI was ever reported).
    pub avg_rssi: Option<i64>,
    pub min_rssi: Option<i64>,
    pub max_rssi: Option<i64>,
    pub source: Option<String>,
}

impl PresenceStats {
    /// Build from a list of per-observation timestamps + RSSI values for one device.
    /// `timestamps` and `rssis` must have the same length. `source` is the most
    /// recent source label if available.
    pub fn from_observations(
        pseudonym: impl Into<String>,
        timestamps: &[i64],
        rssis: &[Option<i64>],
        source: Option<String>,
        window_days: Option<u64>,
    ) -> Option<Self> {
        if timestamps.is_empty() {
            return None;
        }
        let first_seen = *timestamps.iter().min().unwrap();
        let last_seen = *timestamps.iter().max().unwrap();
        let observations = timestamps.len() as u64;
        let visit_duration_secs = (last_seen - first_seen).max(0);

        // Distinct days: truncate to UTC date. Good enough for sensor analytics;
        // the backend can bucket by local TZ if needed.
        let distinct_days = {
            let mut days = std::collections::HashSet::new();
            for &ts in timestamps {
                days.insert(ts / 86400);
            }
            days.len()
        };

        let recurrence_score = match window_days {
            Some(w) if w > 0 => (distinct_days as f64 / w as f64).min(1.0),
            _ => 0.0,
        };

        let mut hour_histogram = [0u64; 24];
        let mut weekday_histogram = [0u64; 7];
        for &ts in timestamps {
            // Use chrono for calendar decomposition when available; fall back
            // to a simple UTC approximation when not. We prefer chrono when
            // the `chrono` crate is in scope (it always is for the host build).
            let (hour, wday) = timestamp_to_hour_wday(ts);
            hour_histogram[hour] += 1;
            weekday_histogram[wday] += 1;
        }

        let rssis_present: Vec<i64> = rssis.iter().filter_map(|v| *v).collect();
        let (avg_rssi, min_rssi, max_rssi) = if rssis_present.is_empty() {
            (None, None, None)
        } else {
            let sum: i64 = rssis_present.iter().sum();
            let avg = (sum as f64 / rssis_present.len() as f64).round() as i64;
            let min = *rssis_present.iter().min().unwrap();
            let max = *rssis_present.iter().max().unwrap();
            (Some(avg), Some(min), Some(max))
        };

        Some(PresenceStats {
            pseudonym: pseudonym.into(),
            first_seen,
            last_seen,
            observations,
            visit_duration_secs,
            distinct_days,
            recurrence_score,
            hour_histogram,
            weekday_histogram,
            avg_rssi,
            min_rssi,
            max_rssi,
            source,
        })
    }
}

/// Decompose an epoch timestamp into (hour 0-23, weekday 0=Mon..6=Sun) in UTC.
/// Uses `chrono` when available for correctness around leap handling; the
/// fallback integer arithmetic is equivalent for the histogram purpose.
fn timestamp_to_hour_wday(ts: i64) -> (usize, usize) {
    // Prefer chrono if linked (store already depends on it). Use a lightweight
    // UTC decomposition to avoid pulling chrono into no-persist builds.
    // Manual UTC decomposition:
    //   hour = (ts % 86400) / 3600  (with negative handling)
    //   weekday: 1970-01-01 was Thursday (3). So wday = (days_since_epoch + 3) % 7.
    let secs_in_day = ts.rem_euclid(86400);
    let hour = (secs_in_day / 3600) as usize;
    let days = ts.div_euclid(86400);
    // 1970-01-01 Thu = 3 (Mon=0). So Mon offset: Thu(3) → Mon(0) diff = 3.
    let wday = ((days + 3).rem_euclid(7)) as usize;
    (hour.min(23), wday.min(6))
}

/// Per-device observation bundle: (timestamps, rssis, source).
pub type Observations = (Vec<i64>, Vec<Option<i64>>, Option<String>);

/// Aggregate a collection of per-device observation lists into `PresenceStats`.
/// `per_device`: pseudonym → observation bundle.
pub fn aggregate_presence(
    per_device: &HashMap<String, Observations>,
    window_days: Option<u64>,
) -> Vec<PresenceStats> {
    let mut out = Vec::new();
    for (pseudo, (tss, rssis, src)) in per_device {
        if let Some(s) =
            PresenceStats::from_observations(pseudo, tss, rssis, src.clone(), window_days)
        {
            out.push(s);
        }
    }
    // Most recently seen first — matches `store::device_aggregates` ordering.
    out.sort_by_key(|s| std::cmp::Reverse(s.last_seen));
    out
}

/// Convenience: build `per_device` map from the SQLite `store::DeviceStats` rows
/// when only first/last/observations/avg_rssi are available (no per-observation
/// timestamps). Approximates histograms as empty and derives duration from
/// first/last. Useful for the backend GET /api/v1/devices path.
#[cfg(feature = "persist")]
pub fn presence_from_store_rows(
    rows: &[crate::store::DeviceStats],
    window_days: Option<u64>,
) -> Vec<PresenceStats> {
    rows.iter()
        .map(|r| {
            let visit_duration_secs = (r.last_seen - r.first_seen).max(0);
            let w = window_days.unwrap_or(0);
            let recurrence = if w > 0 && r.observations > 0 {
                (1.0 / w as f64).min(1.0)
            } else {
                0.0
            };
            PresenceStats {
                pseudonym: r.pseudonym.clone(),
                first_seen: r.first_seen,
                last_seen: r.last_seen,
                observations: r.observations as u64,
                visit_duration_secs,
                distinct_days: 1, // lower bound
                recurrence_score: recurrence,
                hour_histogram: [0; 24],
                weekday_histogram: [0; 7],
                avg_rssi: r.avg_rssi,
                min_rssi: r.min_rssi,
                max_rssi: r.max_rssi,
                source: r.source.clone(),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Histogram helpers
// ---------------------------------------------------------------------------

/// Pretty-print an hour histogram as `00:3 01:0 …` for CLI/report use.
pub fn format_hour_histogram(hist: &[u64; 24]) -> String {
    hist.iter()
        .enumerate()
        .map(|(h, c)| format!("{:02}:{}", h, c))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Pretty-print a weekday histogram as `Mon:3 Tue:0 …`.
pub fn format_weekday_histogram(hist: &[u64; 7]) -> String {
    const NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    NAMES
        .iter()
        .zip(hist.iter())
        .map(|(n, c)| format!("{}:{}", n, c))
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_from_two_observations() {
        // 2024-01-01 08:00 UTC and 2024-01-03 08:00 UTC
        let t0 = 1704096000; // Mon 2024-01-01 08:00
        let t1 = 1704268800; // Wed 2024-01-03 08:00
        let s = PresenceStats::from_observations(
            "abc",
            &[t0, t1],
            &[Some(-50), Some(-60)],
            Some("wifi".into()),
            Some(7),
        )
        .unwrap();
        assert_eq!(s.first_seen, t0);
        assert_eq!(s.last_seen, t1);
        assert_eq!(s.observations, 2);
        assert_eq!(s.visit_duration_secs, t1 - t0);
        assert_eq!(s.distinct_days, 2);
        // 2 distinct days / 7 window = ~0.285
        assert!((s.recurrence_score - 2.0 / 7.0).abs() < 1e-9);
        assert_eq!(s.avg_rssi, Some(-55));
        assert_eq!(s.min_rssi, Some(-60));
        assert_eq!(s.max_rssi, Some(-50));
        // Hour 8 should have 2 hits, weekday Mon(0) and Wed(2).
        assert_eq!(s.hour_histogram[8], 2);
        assert_eq!(s.weekday_histogram[0], 1);
        assert_eq!(s.weekday_histogram[2], 1);
    }

    #[test]
    fn presence_empty_returns_none() {
        assert!(PresenceStats::from_observations("x", &[], &[], None, None).is_none());
    }

    #[test]
    fn presence_no_rssi() {
        let s = PresenceStats::from_observations("x", &[1000, 2000], &[None, None], None, None)
            .unwrap();
        assert_eq!(s.avg_rssi, None);
        assert_eq!(s.observations, 2);
    }

    #[test]
    fn weekday_known_anchor() {
        // 1970-01-01 00:00 UTC = Thu (3)
        let (h, w) = timestamp_to_hour_wday(0);
        assert_eq!(h, 0);
        assert_eq!(w, 3);
        // 1970-01-05 Mon 00:00
        let (h, w) = timestamp_to_hour_wday(4 * 86400);
        assert_eq!(h, 0);
        assert_eq!(w, 0);
    }

    #[test]
    fn aggregate_presence_sorts_by_last_seen() {
        let mut map = HashMap::new();
        map.insert("a".into(), (vec![100, 200], vec![None, None], None));
        map.insert("b".into(), (vec![150, 300], vec![None, None], None));
        let out = aggregate_presence(&map, None);
        assert_eq!(out[0].pseudonym, "b"); // 300 > 200
    }
}
