//! RfPresenceSensor — turn raw 802.11 probe frames into canonical presence/proximity events.
//!
//! Lives between the packet sniffer (e.g. `extsensor`) and the event transport.
//! It combines a `ProximityEngine` (RSSI smoothing + zone classification) with a
//! `TemporalEngine` (RF_PRESENT ↔ ABSENT state machine) so the extsensor emits
//! `device.presence_changed`, `device.signal_changed` and `device.proximity_changed`
//! instead of raw `rf.probe_detected` frames.

use crate::calibrate::Band;
use crate::proximity::{ProximityConfig, ProximityEngine, SignalType};
use crate::temporal::{EventEnvelope, ProbeObservation, TemporalConfig, TemporalEngine};
use std::collections::HashMap;

/// Raw 802.11 probe as seen by the sniffer.
#[derive(Debug, Clone)]
pub struct RfProbe {
    pub device_id: String,
    pub timestamp: i64,
    pub band: String,
    pub channel: Option<u8>,
    pub frequency_mhz: Option<u32>,
    pub rssi_dbm: Option<i64>,
    pub per_chain_rssi: Vec<i64>,
    pub ssid: Option<String>,
    pub ht_vht_he: Option<String>,
    pub supported_rates: Vec<String>,
    pub vendor_ies: Vec<String>,
    pub randomized: bool,
    pub confidence: f64,
}

/// External RF sensor presence/proximity engine.
pub struct RfPresenceSensor {
    sensor_id: String,
    temporal: TemporalEngine,
    proximity: ProximityEngine,
    last_probe_ts: HashMap<String, i64>,
    tick_interval_ms: u64,
    last_tick: i64,
}

impl RfPresenceSensor {
    pub fn new(sensor_id: &str, temporal_cfg: TemporalConfig, proximity_cfg: ProximityConfig) -> Self {
        Self {
            sensor_id: sensor_id.to_string(),
            temporal: TemporalEngine::new(sensor_id, temporal_cfg),
            proximity: ProximityEngine::new(proximity_cfg),
            last_probe_ts: HashMap::new(),
            tick_interval_ms: 1000,
            last_tick: 0,
        }
    }

    pub fn with_tick_interval(mut self, ms: u64) -> Self {
        self.tick_interval_ms = ms;
        self
    }

    /// Feed one probe and return any canonical events produced.
    pub fn observe(&mut self, probe: &RfProbe, ts: i64) -> Vec<EventEnvelope> {
        self.last_probe_ts.insert(probe.device_id.clone(), ts);

        let band = band_from_str(&probe.band);
        let prox = probe
            .rssi_dbm
            .map(|r| self.proximity.update(&probe.device_id, Some(r), SignalType::Dbm, band, ts));

        let obs = ProbeObservation {
            device_id: probe.device_id.clone(),
            timestamp: ts,
            sensor_id: self.sensor_id.clone(),
            band: Some(probe.band.clone()),
            channel: probe.channel,
            frequency: probe.frequency_mhz,
            rssi: probe.rssi_dbm,
            per_chain_rssi: probe.per_chain_rssi.clone(),
            ssid: probe.ssid.clone(),
            ht_vht_he: probe.ht_vht_he.clone(),
            vendor_ies: probe.vendor_ies.clone(),
            supported_rates: probe.supported_rates.clone(),
            randomized: probe.randomized,
            confidence: probe.confidence,
            proximity: prox,
        };

        self.last_tick = ts;
        self.temporal.process_probes(ts, &[obs])
    }

    /// Periodic heartbeat. Must be called regularly (e.g. every second) so the
    /// temporal engine can transition RF_PRESENT devices to ABSENT after a
    /// configurable number of missed ticks.
    pub fn tick(&mut self, ts: i64) -> Vec<EventEnvelope> {
        self.last_tick = ts;
        self.temporal.process_probes(ts, &[])
    }

    pub fn last_tick(&self) -> i64 {
        self.last_tick
    }

    pub fn tick_interval_ms(&self) -> u64 {
        self.tick_interval_ms
    }
}

fn band_from_str(s: &str) -> Band {
    if s.eq_ignore_ascii_case("2.4GHz") {
        Band::Ghz2_4
    } else if s.eq_ignore_ascii_case("5GHz") {
        Band::Ghz5
    } else {
        Band::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proximity::ProximityConfig;
    use crate::temporal::{EventType, TemporalConfig};

    fn probe(id: &str, rssi: i64, band: &str) -> RfProbe {
        RfProbe {
            device_id: id.into(),
            timestamp: 0,
            band: band.into(),
            channel: Some(1),
            frequency_mhz: Some(2412),
            rssi_dbm: Some(rssi),
            per_chain_rssi: vec![rssi],
            ssid: None,
            ht_vht_he: None,
            supported_rates: vec![],
            vendor_ies: vec![],
            randomized: true,
            confidence: 0.5,
        }
    }

    fn sensor() -> RfPresenceSensor {
        RfPresenceSensor::new(
            "ext-001",
            TemporalConfig {
                polls_to_absent: 2,
                signal_delta_threshold: 5,
                ..Default::default()
            },
            ProximityConfig::default(),
        )
    }

    #[test]
    fn first_probe_emits_presence_changed() {
        let mut s = sensor();
        let ev = s.observe(&probe("AA", -40, "2.4GHz"), 1000);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].event_type, EventType::DevicePresenceChanged);
        assert_eq!(ev[0].payload["to_state"], "RF_PRESENT");
    }

    #[test]
    fn second_probe_with_big_rssi_delta_emits_signal_changed() {
        let mut s = sensor();
        s.observe(&probe("AA", -40, "2.4GHz"), 1000);
        let ev = s.observe(&probe("AA", -60, "2.4GHz"), 1010);
        let types: Vec<_> = ev.iter().map(|e| e.event_type).collect();
        assert!(types.contains(&EventType::DeviceSignalChanged));
    }

    #[test]
    fn missed_ticks_emit_absence() {
        let mut s = sensor();
        s.observe(&probe("AA", -40, "2.4GHz"), 1000);
        s.tick(1010);
        let ev = s.tick(1020);
        let types: Vec<_> = ev.iter().map(|e| e.event_type).collect();
        assert!(types.contains(&EventType::DevicePresenceChanged));
        assert!(ev.iter().any(|e| e.payload["to_state"] == "ABSENT"));
    }

    #[test]
    fn proximity_zone_change_emits_proximity_changed() {
        let mut s = sensor();
        // immediate -> far/edge (need a few samples for the median to move)
        let mut all = Vec::new();
        for t in 0..3 {
            all.extend(s.observe(&probe("AA", -40, "2.4GHz"), 1000 + t * 10));
        }
        for t in 0..4 {
            all.extend(s.observe(&probe("AA", -95, "2.4GHz"), 1030 + t * 10));
        }
        let types: Vec<_> = all.iter().map(|e| e.event_type).collect();
        assert!(types.contains(&EventType::DeviceProximityChanged));
    }
}
