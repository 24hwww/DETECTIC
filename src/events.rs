//! Event generation — turn NetworkMap snapshots into privacy-safe change events.
//!
//! Events are produced by comparing the current snapshot with the previous one.
//! Only a per-sensor pseudonym and a list of changed field names are exposed;
//! raw MACs, IPs and hostnames never appear in events.

use crate::model::{Device, MapDiff};
use serde::{Deserialize, Serialize};

/// Kinds of device-lifecycle events.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventKind {
    DeviceJoined,
    DeviceLeft,
    DeviceUpdated,
}

/// A single privacy-preserving change event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Epoch seconds when the snapshot was captured.
    pub captured_at: i64,
    pub kind: EventKind,
    /// Deterministic per-sensor pseudonym; never a raw MAC.
    pub pseudonym: String,
    /// Original device identity (MAC/IP/hostname) for local lookup.
    /// Never serialized to the upload payload (privacy: AGENTS.md §21/§39).
    #[serde(skip_serializing)]
    pub identity: String,
    /// Human-readable field names that changed (only for `DeviceUpdated`).
    pub changed_fields: Vec<String>,
}

/// Convert a `MapDiff` into a list of events, one per added/removed/changed device.
///
/// `pseudonym_fn` derives the pseudonym for a device.  It receives the device's
/// canonical identity (MAC preferred, then IP, then hostname).
pub fn diff_to_events<F>(diff: &MapDiff, captured_at: i64, mut pseudonym_fn: F) -> Vec<Event>
where
    F: FnMut(&str) -> String,
{
    let mut events = Vec::new();

    for d in &diff.added {
        events.push(Event {
            captured_at,
            kind: EventKind::DeviceJoined,
            pseudonym: pseudonym_fn(&d.identity()),
            identity: d.identity(),
            changed_fields: Vec::new(),
        });
    }

    for d in &diff.removed {
        events.push(Event {
            captured_at,
            kind: EventKind::DeviceLeft,
            pseudonym: pseudonym_fn(&d.identity()),
            identity: d.identity(),
            changed_fields: Vec::new(),
        });
    }

    for (before, after) in &diff.changed {
        let changed = changed_fields(before, after);
        if !changed.is_empty() {
            events.push(Event {
                captured_at,
                kind: EventKind::DeviceUpdated,
                pseudonym: pseudonym_fn(&after.identity()),
                identity: after.identity(),
                changed_fields: changed,
            });
        }
    }

    // Stable ordering: joined, then updated, then left, and within each group by
    // pseudonym so repeated diffs with the same devices are deterministic.
    fn kind_order(k: &EventKind) -> u8 {
        match k {
            EventKind::DeviceJoined => 0,
            EventKind::DeviceUpdated => 1,
            EventKind::DeviceLeft => 2,
        }
    }
    events.sort_by(|a, b| {
        kind_order(&a.kind)
            .cmp(&kind_order(&b.kind))
            .then_with(|| a.pseudonym.cmp(&b.pseudonym))
    });

    events
}

/// Compare two device records and return the list of field names whose values
/// differ.  Only meaningful fields are compared; `None` vs `None` is equal.
fn changed_fields(before: &Device, after: &Device) -> Vec<String> {
    let mut out = Vec::new();
    if before.hostname != after.hostname {
        out.push("hostname".into());
    }
    if before.ip != after.ip {
        out.push("ip".into());
    }
    if before.mac != after.mac {
        out.push("mac".into());
    }
    if before.rssi != after.rssi {
        out.push("rssi".into());
    }
    if before.standard != after.standard {
        out.push("standard".into());
    }
    if before.onemesh_stack != after.onemesh_stack {
        out.push("onemesh_stack".into());
    }
    if before.assoc_time != after.assoc_time {
        out.push("assoc_time".into());
    }
    if before.radio_mac != after.radio_mac {
        out.push("radio_mac".into());
    }
    if before.source != after.source {
        out.push("source".into());
    }
    // M5 extended fields
    if before.tx_rate != after.tx_rate {
        out.push("tx_rate".into());
    }
    if before.rx_rate != after.rx_rate {
        out.push("rx_rate".into());
    }
    if before.noise != after.noise {
        out.push("noise".into());
    }
    if before.signal_level != after.signal_level {
        out.push("signal_level".into());
    }
    if before.max_link_rate != after.max_link_rate {
        out.push("max_link_rate".into());
    }
    if before.interface != after.interface {
        out.push("interface".into());
    }
    if before.ipv6 != after.ipv6 {
        out.push("ipv6".into());
    }
    if before.client_type != after.client_type {
        out.push("client_type".into());
    }
    if before.active != after.active {
        out.push("active".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Device, NetworkMap};

    fn dev(
        mac: &str,
        hostname: Option<&str>,
        ip: Option<&str>,
        rssi: Option<i64>,
        standard: Option<&str>,
    ) -> Device {
        Device {
            hostname: hostname.map(|s| s.into()),
            ip: ip.map(|s| s.into()),
            mac: Some(mac.into()),
            rssi,
            standard: standard.map(|s| s.into()),
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
            active: None,
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
        format!("pseudo:{}", s)
    }

    #[test]
    fn first_snapshot_generates_only_joined_events() {
        let m = map_at(
            1000,
            vec![dev(
                "AA:BB:CC:00:00:01",
                Some("phone"),
                Some("10.0.0.1"),
                Some(-50),
                Some("ax"),
            )],
        );
        let diff = NetworkMap::default().diff(&m);
        let events = diff_to_events(&diff, 1000, |id| pseudo(id));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::DeviceJoined);
        assert_eq!(events[0].pseudonym, "pseudo:AA:BB:CC:00:00:01");
    }

    #[test]
    fn identical_snapshots_generate_no_events() {
        let m = map_at(
            1000,
            vec![dev(
                "AA:BB:CC:00:00:01",
                Some("phone"),
                Some("10.0.0.1"),
                Some(-50),
                Some("ax"),
            )],
        );
        let diff = m.diff(&m);
        let events = diff_to_events(&diff, 1000, |id| pseudo(id));
        assert!(events.is_empty());
    }

    #[test]
    fn device_left_and_joined() {
        let prev = map_at(1000, vec![dev("AA:BB:CC:00:00:01", None, None, None, None)]);
        let curr = map_at(2000, vec![dev("AA:BB:CC:00:00:02", None, None, None, None)]);
        let diff = prev.diff(&curr);
        let events = diff_to_events(&diff, 2000, |id| pseudo(id));
        assert_eq!(events.len(), 2);
        assert!(events
            .iter()
            .any(|e| e.kind == EventKind::DeviceLeft && e.pseudonym == "pseudo:AA:BB:CC:00:00:01"));
        assert!(events.iter().any(
            |e| e.kind == EventKind::DeviceJoined && e.pseudonym == "pseudo:AA:BB:CC:00:00:02"
        ));
    }

    #[test]
    fn rssi_change_generates_updated_event() {
        let prev = map_at(
            1000,
            vec![dev("AA:BB:CC:00:00:01", None, None, Some(-50), Some("ax"))],
        );
        let curr = map_at(
            2000,
            vec![dev("AA:BB:CC:00:00:01", None, None, Some(-60), Some("ax"))],
        );
        let diff = prev.diff(&curr);
        let events = diff_to_events(&diff, 2000, |id| pseudo(id));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::DeviceUpdated);
        assert!(events[0].changed_fields.contains(&"rssi".to_string()));
    }

    #[test]
    fn ip_and_hostname_changes_detected() {
        let prev = map_at(
            1000,
            vec![dev(
                "AA:BB:CC:00:00:01",
                Some("old"),
                Some("10.0.0.1"),
                None,
                None,
            )],
        );
        let curr = map_at(
            2000,
            vec![dev(
                "AA:BB:CC:00:00:01",
                Some("new"),
                Some("10.0.0.2"),
                None,
                None,
            )],
        );
        let diff = prev.diff(&curr);
        let events = diff_to_events(&diff, 2000, |id| pseudo(id));
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert!(e.changed_fields.contains(&"hostname".to_string()));
        assert!(e.changed_fields.contains(&"ip".to_string()));
    }

    #[test]
    fn standard_and_radio_mac_changes_detected() {
        let prev = map_at(
            1000,
            vec![Device {
                radio_mac: Some("00:11:22:33:44:55".into()),
                standard: Some("n".into()),
                ..dev("AA:BB:CC:00:00:01", None, None, None, None)
            }],
        );
        let curr = map_at(
            2000,
            vec![Device {
                radio_mac: Some("00:11:22:33:44:66".into()),
                standard: Some("ax".into()),
                ..dev("AA:BB:CC:00:00:01", None, None, None, None)
            }],
        );
        let diff = prev.diff(&curr);
        let events = diff_to_events(&diff, 2000, |id| pseudo(id));
        assert_eq!(events.len(), 1);
        assert!(events[0].changed_fields.contains(&"standard".to_string()));
        assert!(events[0].changed_fields.contains(&"radio_mac".to_string()));
    }

    #[test]
    fn ordering_change_does_not_generate_event() {
        let a = dev("AA:BB:CC:00:00:01", None, None, Some(-50), None);
        let b = dev("AA:BB:CC:00:00:02", None, None, Some(-60), None);
        let prev = map_at(1000, vec![a.clone(), b.clone()]);
        let curr = map_at(2000, vec![b.clone(), a.clone()]);
        let diff = prev.diff(&curr);
        let events = diff_to_events(&diff, 2000, |id| pseudo(id));
        assert!(events.is_empty());
    }

    #[test]
    fn duplicate_devices_collapsed_before_events() {
        // Two devices with the same MAC should not happen after collector merge,
        // but the diff uses identity() and therefore treats them as one.
        let d1 = dev("AA:BB:CC:00:00:01", Some("a"), None, Some(-50), None);
        let d2 = dev("AA:BB:CC:00:00:01", Some("b"), None, Some(-60), None);
        let curr = map_at(1000, vec![d1, d2]);
        let diff = NetworkMap::default().diff(&curr);
        let events = diff_to_events(&diff, 1000, |id| pseudo(id));
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn events_sorted_by_kind_then_pseudonym() {
        let prev = map_at(
            1000,
            vec![
                dev("BB:BB:BB:00:00:01", None, None, None, None),
                dev("CC:CC:CC:00:00:01", None, None, None, None),
            ],
        );
        let curr = map_at(
            2000,
            vec![
                dev("AA:AA:AA:00:00:01", None, None, None, None), // joined
                dev("BB:BB:BB:00:00:01", None, None, Some(-55), None), // updated
            ],
        );
        let diff = prev.diff(&curr);
        let events = diff_to_events(&diff, 2000, |id| pseudo(id));
        assert_eq!(events[0].kind, EventKind::DeviceJoined);
        assert_eq!(events[0].pseudonym, "pseudo:AA:AA:AA:00:00:01");
        assert_eq!(events[1].kind, EventKind::DeviceUpdated);
        assert_eq!(events[2].kind, EventKind::DeviceLeft);
    }
}
