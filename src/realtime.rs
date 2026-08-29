//! Real-time unified event pipeline (M11-C).
//!
//! The pipeline fuses observations from several legitimate sources into a single
//! ordered, deduplicated stream of [`RealtimeEvent`]s:
//!
//! Sources (fed in from the outside; the pipeline itself does not poll):
//!   - GTPR polling snapshots  → associated stations (`DeviceJoined` /
//!     `DeviceUpdated` / `DeviceLeft`)
//!   - Probe observations       → unassociated devices (`DeviceNearby` /
//!     `DeviceLost`) — empty on stock EX520V (see `m11_boot_mechanisms.md`)
//!   - Nearby observations      → site-survey APs (`DeviceNearby` /
//!     `DeviceLost`)
//!
//! Guarantees (unit-tested):
//!   - **Monotonic sequence numbers**: every emitted event gets a strictly
//!     increasing `seq`.
//!   - **Ordering**: events are returned in `seq` order.
//!   - **Deduplication**: an identical `(identity, kind)` pair is not re-emitted
//!     within the debounce window.
//!
//! The off-router runner [`RealtimePipeline::run`] drives the loop using a
//! caller-supplied closure that returns the current `NetworkMap`, nearby
//! observations, and probe observations. This keeps the pipeline decoupled from
//! the transport and fully testable without hardware.

use crate::driver::ProbeObservation;
use crate::model::{DistanceEstimate, NetworkMap};
use crate::monitor::NearbyObservation;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Event kinds emitted by the real-time pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RealtimeEventKind {
    DeviceJoined,
    DeviceUpdated,
    DeviceLeft,
    DeviceNearby,
    DeviceLost,
}

impl RealtimeEventKind {
    /// Stable display name.
    pub fn as_str(&self) -> &'static str {
        match self {
            RealtimeEventKind::DeviceJoined => "DEVICE_JOINED",
            RealtimeEventKind::DeviceUpdated => "DEVICE_UPDATED",
            RealtimeEventKind::DeviceLeft => "DEVICE_LEFT",
            RealtimeEventKind::DeviceNearby => "DEVICE_NEARBY",
            RealtimeEventKind::DeviceLost => "DEVICE_LOST",
        }
    }
}

/// A single unified, ordered, deduplicated event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeEvent {
    /// Monotonically increasing sequence number.
    pub seq: u64,
    /// Epoch seconds when the observation was captured.
    pub captured_at: i64,
    /// Event type.
    pub kind: RealtimeEventKind,
    /// Identity (pseudonym) of the device.
    pub identity: String,
    /// RSSI if known.
    pub rssi: Option<i64>,
    /// Origin of the observation (`gtpr`, `probe`, `survey`).
    pub source: String,
    /// Confidence [0.0, 1.0].
    pub confidence: f64,
    /// WiFi channel the observation was made on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<u8>,
    /// WiFi band label (e.g. "2.4GHz", "5GHz").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub band: Option<String>,
    /// Estimated distance, if computed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<DistanceEstimate>,
}

/// A cycle's worth of observations handed to the pipeline.
pub struct ObservationBatch {
    pub map: NetworkMap,
    pub nearby: Vec<NearbyObservation>,
    pub probes: Vec<ProbeObservation>,
}

/// Unified real-time event pipeline.
pub struct RealtimePipeline {
    seq: u64,
    /// identity -> last emitted timestamp, per kind (for debouncing).
    last_emitted: HashMap<(String, RealtimeEventKind), i64>,
    /// identity -> last seen timestamp for associated devices.
    associated_seen: HashMap<String, i64>,
    /// identity -> last seen timestamp for nearby devices.
    nearby_seen: HashMap<String, i64>,
    /// Debounce window in seconds.
    debounce_secs: i64,
}

impl Default for RealtimePipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl RealtimePipeline {
    pub fn new() -> Self {
        Self {
            seq: 0,
            last_emitted: HashMap::new(),
            associated_seen: HashMap::new(),
            nearby_seen: HashMap::new(),
            debounce_secs: 30,
        }
    }

    /// Set the deduplication window (seconds).
    pub fn with_debounce(mut self, secs: i64) -> Self {
        self.debounce_secs = secs.max(0);
        self
    }

    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn emit(
        &mut self,
        out: &mut Vec<RealtimeEvent>,
        kind: RealtimeEventKind,
        identity: &str,
        rssi: Option<i64>,
        source: &str,
        confidence: f64,
    ) {
        let ts = Self::now();
        let key = (identity.to_string(), kind);
        if let Some(prev) = self.last_emitted.get(&key) {
            if ts - prev < self.debounce_secs {
                return; // debounced duplicate
            }
        }
        self.seq += 1;
        self.last_emitted.insert(key, ts);
        out.push(RealtimeEvent {
            seq: self.seq,
            captured_at: ts,
            kind,
            identity: identity.to_string(),
            rssi,
            source: source.to_string(),
            confidence,
            channel: None,
            band: None,
            distance: None,
        });
    }

    /// Ingest one observation batch and return the events produced this cycle.
    ///
    /// `pseudonym_fn` converts a device identity (MAC preferred) into the
    /// stable per-sensor pseudonym used downstream.
    pub fn ingest<F>(&mut self, batch: &ObservationBatch, mut pseudonym_fn: F) -> Vec<RealtimeEvent>
    where
        F: FnMut(&str) -> String,
    {
        let mut out: Vec<RealtimeEvent> = Vec::new();
        let ts = batch.map.captured_at;

        // Associated stations (GTPR)
        let mut current_assoc: HashMap<String, i64> = HashMap::new();
        for d in &batch.map.devices {
            let id = d.identity();
            let pseudo = pseudonym_fn(&id);
            let rssi = d.rssi;
            let was_present = self.associated_seen.contains_key(&id);
            if !was_present {
                self.emit(
                    &mut out,
                    RealtimeEventKind::DeviceJoined,
                    &pseudo,
                    rssi,
                    "gtpr",
                    d.active
                        .as_deref()
                        .map(|a| if a == "1" { 0.9 } else { 0.6 })
                        .unwrap_or(0.7),
                );
            } else {
                // Re-emit an update only if RSSI changed meaningfully is handled
                // by debounce; emit DeviceUpdated each cycle (debounced).
                self.emit(
                    &mut out,
                    RealtimeEventKind::DeviceUpdated,
                    &pseudo,
                    rssi,
                    "gtpr",
                    0.8,
                );
            }
            current_assoc.insert(id, ts);
        }
        // Departed associated devices
        let departed: Vec<String> = self
            .associated_seen
            .keys()
            .filter(|k| !current_assoc.contains_key(*k))
            .cloned()
            .collect();
        for id in departed {
            let pseudo = pseudonym_fn(&id);
            self.emit(
                &mut out,
                RealtimeEventKind::DeviceLeft,
                &pseudo,
                None,
                "gtpr",
                0.8,
            );
        }
        self.associated_seen = current_assoc;

        // Nearby observations (site survey APs) — pseudonymized before emit
        let mut current_nearby: HashMap<String, i64> = HashMap::new();
        for n in &batch.nearby {
            if n.mac.is_empty() {
                continue;
            }
            // Check both the persistent state and the current batch to avoid
            // emitting duplicates when the same device appears twice in one survey.
            let was_present =
                self.nearby_seen.contains_key(&n.mac) || current_nearby.contains_key(&n.mac);
            if !was_present {
                let pseudo = pseudonym_fn(&n.mac);
                self.emit(
                    &mut out,
                    RealtimeEventKind::DeviceNearby,
                    &pseudo,
                    n.rssi,
                    "survey",
                    n.confidence,
                );
            }
            current_nearby.insert(n.mac.clone(), ts);
        }
        let lost: Vec<String> = self
            .nearby_seen
            .keys()
            .filter(|k| !current_nearby.contains_key(*k))
            .cloned()
            .collect();
        for id in lost {
            let pseudo = pseudonym_fn(&id);
            self.emit(
                &mut out,
                RealtimeEventKind::DeviceLost,
                &pseudo,
                None,
                "survey",
                0.6,
            );
        }
        self.nearby_seen = current_nearby;

        // Probe observations (unassociated) — empty on stock EX520V.
        // Pseudonymized before emit (AGENTS.md §21).
        for p in &batch.probes {
            if p.is_empty() {
                continue;
            }
            let pseudo = pseudonym_fn(&p.mac);
            self.emit(
                &mut out,
                RealtimeEventKind::DeviceNearby,
                &pseudo,
                p.rssi,
                "probe",
                p.confidence,
            );
        }

        // Guarantee seq ordering of the produced batch.
        out.sort_by_key(|e| e.seq);
        out
    }

    /// Current monotonic sequence counter (next event will use `seq + 1`).
    pub fn next_seq(&self) -> u64 {
        self.seq + 1
    }

    /// Off-router driver: repeatedly call `feeder`, feed the pipeline, and
    /// invoke `on_event` for each produced event. `feeder` returns the current
    /// observation batch. Runs until `should_stop()` returns true.
    ///
    /// `secret` is the per-sensor HMAC key used for pseudonymization of all
    /// device identities (associated, nearby, and probe) before emission.
    ///
    /// This keeps the pipeline transport-agnostic; on the development client it
    /// is wired to a GTPR poller, using the EX520 purely as a telemetry source.
    pub fn run<F, G, H>(
        &mut self,
        mut feeder: F,
        mut on_event: G,
        mut should_stop: H,
        interval_secs: u64,
        secret: &[u8],
    ) where
        F: FnMut() -> ObservationBatch,
        G: FnMut(&RealtimeEvent),
        H: FnMut() -> bool,
    {
        let mut elapsed: u64 = 0;
        while !should_stop() {
            let batch = feeder();
            let events = self.ingest(&batch, |id| crate::pseudonymize(secret, id));
            for e in &events {
                on_event(e);
            }
            // Sleep in 1s steps so shutdown is responsive.
            let step = 1u64;
            let mut waited = 0u64;
            while waited < interval_secs && !should_stop() {
                std::thread::sleep(std::time::Duration::from_secs(step));
                waited += step;
                elapsed += step;
            }
            let _ = elapsed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Device;
    use crate::monitor::NearbySource;

    fn dev(mac: &str, rssi: Option<i64>, active: Option<&str>) -> Device {
        Device {
            hostname: None,
            ip: None,
            mac: Some(mac.into()),
            rssi,
            standard: None,
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
            active: active.map(|s| s.into()),
        }
    }

    fn map_at(ts: i64, devices: Vec<Device>) -> NetworkMap {
        NetworkMap {
            captured_at: ts,
            devices,
            raw: Default::default(),
        }
    }

    fn pseudo(s: &str) -> String {
        format!("p:{}", s)
    }

    #[test]
    fn seq_is_monotonic_and_ordered() {
        let mut p = RealtimePipeline::new().with_debounce(0);
        // Cycle 1: one join
        let b1 = ObservationBatch {
            map: map_at(1000, vec![dev("AA:AA", Some(-50), Some("1"))]),
            nearby: vec![],
            probes: vec![],
        };
        let e1 = p.ingest(&b1, pseudo);
        assert_eq!(e1.len(), 1);
        assert_eq!(e1[0].kind, RealtimeEventKind::DeviceJoined);
        assert_eq!(e1[0].seq, 1);

        // Cycle 2: same device still present -> DeviceUpdated
        let b2 = ObservationBatch {
            map: map_at(2000, vec![dev("AA:AA", Some(-55), Some("1"))]),
            nearby: vec![],
            probes: vec![],
        };
        let e2 = p.ingest(&b2, pseudo);
        assert_eq!(e2.len(), 1);
        assert_eq!(e2[0].kind, RealtimeEventKind::DeviceUpdated);
        assert!(e2[0].seq > e1[0].seq);

        // Cycle 3: device gone -> DeviceLeft
        let b3 = ObservationBatch {
            map: map_at(3000, vec![]),
            nearby: vec![],
            probes: vec![],
        };
        let e3 = p.ingest(&b3, pseudo);
        assert_eq!(e3.len(), 1);
        assert_eq!(e3[0].kind, RealtimeEventKind::DeviceLeft);
        assert!(e3[0].seq > e2[0].seq);

        // Sequence strictly increasing across all events.
        let mut all = vec![e1, e2, e3].concat();
        all.sort_by_key(|e| e.seq);
        for w in all.windows(2) {
            assert!(w[1].seq > w[0].seq);
        }
    }

    #[test]
    fn dedup_within_debounce_window() {
        let mut p = RealtimePipeline::new().with_debounce(30);
        let b1 = ObservationBatch {
            map: map_at(1000, vec![dev("BB:BB", Some(-50), Some("1"))]),
            nearby: vec![],
            probes: vec![],
        };
        let e1 = p.ingest(&b1, pseudo);
        assert_eq!(e1.len(), 1);
        assert_eq!(e1[0].kind, RealtimeEventKind::DeviceJoined);

        // Second cycle: same device present -> DeviceUpdated (different kind).
        let b2 = ObservationBatch {
            map: map_at(1010, vec![dev("BB:BB", Some(-52), Some("1"))]),
            nearby: vec![],
            probes: vec![],
        };
        let e2 = p.ingest(&b2, pseudo);
        assert_eq!(e2.len(), 1);
        assert_eq!(e2[0].kind, RealtimeEventKind::DeviceUpdated);

        // Third cycle immediately after: DeviceUpdated debounced (same kind).
        let b3 = ObservationBatch {
            map: map_at(1015, vec![dev("BB:BB", Some(-51), Some("1"))]),
            nearby: vec![],
            probes: vec![],
        };
        let e3 = p.ingest(&b3, pseudo);
        assert_eq!(e3.len(), 0, "debounced duplicate update suppressed");
    }

    #[test]
    fn nearby_and_probe_emitted() {
        let mut p = RealtimePipeline::new().with_debounce(0);
        let nearby = vec![NearbyObservation {
            mac: "CC:CC".into(),
            bssid: "DD:DD".into(),
            ssid: "AP".into(),
            channel: 6,
            band: "2.4GHz".into(),
            rssi: Some(-70),
            timestamp: 1000,
            source: NearbySource::Survey,
            confidence: 0.6,
            ..Default::default()
        }];
        let probes = vec![ProbeObservation {
            mac: "EE:EE".into(),
            rssi: Some(-80),
            channel: Some(1),
            band: Some("2.4GHz".into()),
            timestamp: 1000,
            source: "probe".into(),
            confidence: 0.5,
        }];
        let b = ObservationBatch {
            map: map_at(1000, vec![]),
            nearby,
            probes,
        };
        let e = p.ingest(&b, pseudo);
        assert!(e.iter().any(|x| x.kind == RealtimeEventKind::DeviceNearby));
        assert!(e.iter().any(|x| x.source == "survey"));
        assert!(e.iter().any(|x| x.source == "probe"));
    }

    #[test]
    fn empty_probe_is_ignored() {
        let mut p = RealtimePipeline::new().with_debounce(0);
        let b = ObservationBatch {
            map: map_at(1000, vec![]),
            nearby: vec![],
            probes: vec![ProbeObservation::none()],
        };
        let e = p.ingest(&b, pseudo);
        assert!(e.is_empty(), "fabricated/empty probes must never emit");
    }

    #[test]
    fn nearby_and_probe_are_pseudonymized() {
        let mut p = RealtimePipeline::new().with_debounce(0);
        let nearby = vec![NearbyObservation {
            mac: "AA:BB:CC:11:22:33".into(),
            bssid: "DD:DD".into(),
            ssid: "AP".into(),
            channel: 6,
            band: "2.4GHz".into(),
            rssi: Some(-70),
            timestamp: 1000,
            source: NearbySource::Survey,
            confidence: 0.6,
            ..Default::default()
        }];
        let probes = vec![ProbeObservation {
            mac: "AA:BB:CC:44:55:66".into(),
            rssi: Some(-80),
            channel: Some(1),
            band: Some("2.4GHz".into()),
            timestamp: 1000,
            source: "probe".into(),
            confidence: 0.5,
        }];
        let b = ObservationBatch {
            map: map_at(1000, vec![]),
            nearby,
            probes,
        };
        let e = p.ingest(&b, pseudo);
        // Both should emit as DeviceNearby with pseudonymized identity
        assert_eq!(e.len(), 2);
        for event in &e {
            assert_eq!(event.kind, RealtimeEventKind::DeviceNearby);
            assert!(
                event.identity == "p:AA:BB:CC:11:22:33" || event.identity == "p:AA:BB:CC:44:55:66",
                "unexpected identity: {}",
                event.identity
            );
        }
        // No raw MAC should appear in any emitted identity
        assert!(!e.iter().any(|x| x.identity == "AA:BB:CC:11:22:33"));
        assert!(!e.iter().any(|x| x.identity == "AA:BB:CC:44:55:66"));
    }

    #[test]
    fn nearby_departure_emits_device_lost() {
        let mut p = RealtimePipeline::new().with_debounce(0);
        let nearby = |mac: &str| -> Vec<NearbyObservation> {
            vec![NearbyObservation {
                mac: mac.into(),
                bssid: "DD:DD".into(),
                ssid: "AP".into(),
                channel: 6,
                band: "2.4GHz".into(),
                rssi: Some(-70),
                timestamp: 1000,
                source: NearbySource::Survey,
                confidence: 0.6,
                ..Default::default()
            }]
        };

        // Cycle 1: nearby device appears
        let b1 = ObservationBatch {
            map: map_at(1000, vec![]),
            nearby: nearby("CC:CC"),
            probes: vec![],
        };
        let e1 = p.ingest(&b1, pseudo);
        assert_eq!(e1.len(), 1);
        assert_eq!(e1[0].kind, RealtimeEventKind::DeviceNearby);

        // Cycle 2: device gone -> DeviceLost
        let b2 = ObservationBatch {
            map: map_at(2000, vec![]),
            nearby: vec![],
            probes: vec![],
        };
        let e2 = p.ingest(&b2, pseudo);
        assert_eq!(e2.len(), 1);
        assert_eq!(e2[0].kind, RealtimeEventKind::DeviceLost);
        assert_eq!(e2[0].source, "survey");
        // Lost event should also be pseudonymized
        assert_eq!(e2[0].identity, "p:CC:CC");
    }

    #[test]
    fn empty_batch_produces_no_events() {
        let mut p = RealtimePipeline::new().with_debounce(0);
        let b = ObservationBatch {
            map: map_at(1000, vec![]),
            nearby: vec![],
            probes: vec![],
        };
        let e = p.ingest(&b, pseudo);
        assert!(e.is_empty());
    }

    #[test]
    fn duplicate_nearby_in_same_batch_deduplicated() {
        let mut p = RealtimePipeline::new().with_debounce(0);
        let nearby = vec![
            NearbyObservation {
                mac: "CC:CC".into(),
                bssid: "DD:DD".into(),
                ssid: "AP".into(),
                channel: 6,
                band: "2.4GHz".into(),
                rssi: Some(-70),
                timestamp: 1000,
                source: NearbySource::Survey,
                confidence: 0.6,
                ..Default::default()
            },
            NearbyObservation {
                mac: "CC:CC".into(),
                bssid: "DD:DD".into(),
                ssid: "AP".into(),
                channel: 6,
                band: "2.4GHz".into(),
                rssi: Some(-68),
                timestamp: 1000,
                source: NearbySource::Survey,
                confidence: 0.6,
                ..Default::default()
            },
        ];
        let b = ObservationBatch {
            map: map_at(1000, vec![]),
            nearby,
            probes: vec![],
        };
        let e = p.ingest(&b, pseudo);
        // First occurrence emits DeviceNearby; second is already "seen" so suppressed
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].kind, RealtimeEventKind::DeviceNearby);
    }

    #[test]
    fn associated_and_nearby_coexist_in_one_batch() {
        let mut p = RealtimePipeline::new().with_debounce(0);
        let b = ObservationBatch {
            map: map_at(1000, vec![dev("AA:AA", Some(-50), Some("1"))]),
            nearby: vec![NearbyObservation {
                mac: "BB:BB".into(),
                bssid: "CC:CC".into(),
                ssid: "AP".into(),
                channel: 6,
                band: "2.4GHz".into(),
                rssi: Some(-65),
                timestamp: 1000,
                source: NearbySource::Survey,
                confidence: 0.6,
                ..Default::default()
            }],
            probes: vec![],
        };
        let e = p.ingest(&b, pseudo);
        assert_eq!(e.len(), 2);
        assert!(e.iter().any(|x| x.kind == RealtimeEventKind::DeviceJoined));
        assert!(e.iter().any(|x| x.kind == RealtimeEventKind::DeviceNearby));
    }

    #[test]
    fn run_uses_secret_for_pseudonymization() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, Mutex};

        let mut p = RealtimePipeline::new().with_debounce(0);
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let cap_clone = captured.clone();
        let stop_flag: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_flag.clone();

        let mut fed = 0;
        p.run(
            || {
                fed += 1;
                ObservationBatch {
                    map: map_at(1000, vec![dev("AA:AA", Some(-50), Some("1"))]),
                    nearby: vec![],
                    probes: vec![],
                }
            },
            |e| {
                cap_clone.lock().unwrap().push(e.identity.clone());
                stop_clone.store(true, Ordering::SeqCst);
            },
            || stop_flag.load(Ordering::SeqCst),
            1,
            b"real-secret",
        );

        assert_eq!(fed, 1, "should feed exactly once");
        let identities = captured.lock().unwrap();
        assert!(!identities.is_empty(), "at least one event should emit");
        let id = &identities[0];
        assert!(
            !id.contains("AA:AA"),
            "raw MAC leaked into event identity: {}",
            id
        );
        assert_eq!(id.len(), 64, "should be 64-char HMAC-SHA256 hex");
    }
}
