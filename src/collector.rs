//! Collector layer — OIDs → NetworkMap.
//!
//! Pure transformation: no HTTP, no crypto, no I/O. Depends only on the
//! `Transport` trait for fetching raw OID JSON strings. This lets the
//! collector be tested with a fake transport and swapped for alternative
//! providers (mock, OpenWrt, etc.) without touching the transport.

use crate::model::{Device, NetworkMap};
use crate::oids::*;
use crate::transport::{GtprError, Transport};
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Public API — trait-based collection
// ---------------------------------------------------------------------------

/// Collect the full network map via any `Transport` implementation.
/// Fetches the three OIDs (Wi-Fi assoc, DHCP leases, host table) and merges
/// them into a unified `NetworkMap`.
pub fn collect(transport: &dyn Transport) -> Result<NetworkMap, GtprError> {
    let assoc_json = transport.gl(oid::WIFI_APDEV_ASSOCDEV)?;
    let dhcp_json = transport.gl(oid::DHCPV4_CLIENT)?;
    let host_json = transport.gl(oid::HOST_ENTRY)?;
    parse_network_map(&assoc_json, &dhcp_json, &host_json)
}

// ---------------------------------------------------------------------------
// Pure merge — testable without I/O
// ---------------------------------------------------------------------------

/// Pure merge of the three OID responses into a unified device list.
/// Separated from I/O so it can be unit-tested against captured JSON.
pub fn parse_network_map(
    assoc_json: &str,
    dhcp_json: &str,
    host_json: &str,
) -> Result<NetworkMap, GtprError> {
    let assoc: AssocDevResponse = serde_json::from_str(assoc_json).map_err(|e| {
        GtprError::Protocol(format!("assocdev parse: {} | json: {}", e, assoc_json))
    })?;
    let dhcp: DhcpClientResponse = serde_json::from_str(dhcp_json)
        .map_err(|e| GtprError::Protocol(format!("dhcp parse: {} | json: {}", e, dhcp_json)))?;
    let host: HostEntryResponse = serde_json::from_str(host_json)
        .map_err(|e| GtprError::Protocol(format!("host parse: {} | json: {}", e, host_json)))?;

    let mut by_mac: std::collections::HashMap<String, Device> = std::collections::HashMap::new();
    for c in &dhcp.data {
        if let Some(m) = &c.mac {
            by_mac.entry(canon_mac(m)).or_insert_with(|| Device {
                hostname: c.hostname.clone(),
                ip: c.ip.clone(),
                mac: Some(m.clone()),
                rssi: None,
                standard: None,
                onemesh_stack: None,
                assoc_time: None,
                radio_mac: None,
                source: Some("dhcp".into()),
                tx_rate: None,
                rx_rate: None,
                noise: None,
                signal_level: None,
                max_link_rate: None,
                interface: None,
                ipv6: None,
                client_type: None,
                active: None,
            });
        }
    }
    for h in &host.data {
        if let Some(m) = &h.mac {
            by_mac.entry(canon_mac(m)).or_insert_with(|| Device {
                hostname: h.hostname.clone(),
                ip: h.ip.clone(),
                mac: Some(m.clone()),
                rssi: None,
                standard: None,
                onemesh_stack: None,
                assoc_time: None,
                radio_mac: None,
                source: Some("host".into()),
                tx_rate: None,
                rx_rate: None,
                noise: None,
                signal_level: None,
                max_link_rate: None,
                interface: h
                    .layer2_interface
                    .clone()
                    .or_else(|| h.interface_type.clone()),
                ipv6: h.ipv6.clone(),
                client_type: h.client_type.clone(),
                active: h.active.clone(),
            });
        }
    }

    let mut devices: Vec<Device> = assoc
        .data
        .into_iter()
        .map(|e| {
            let rssi_i64 = e.rssi.as_ref().and_then(|s| s.parse::<i64>().ok());
            let assoc_i64 = e.assoc_time.as_ref().and_then(|s| parse_timestamp(s));
            let mut d = Device {
                hostname: e.hostname,
                ip: e.ip,
                mac: e.mac,
                rssi: rssi_i64,
                standard: e.standard,
                onemesh_stack: e.stack,
                assoc_time: assoc_i64,
                radio_mac: e.radio_mac,
                source: Some("wifi".into()),
                tx_rate: e
                    .last_data_downlink_rate
                    .as_deref()
                    .and_then(|s| s.parse::<u64>().ok()),
                rx_rate: e
                    .last_data_uplink_rate
                    .as_deref()
                    .and_then(|s| s.parse::<u64>().ok()),
                noise: e.noise.as_deref().and_then(|s| s.parse::<u64>().ok()),
                signal_level: e
                    .signal_strength_level
                    .as_deref()
                    .and_then(|s| s.parse::<u8>().ok()),
                max_link_rate: e
                    .max_link_rate
                    .as_deref()
                    .and_then(|s| s.parse::<u64>().ok()),
                interface: None,
                ipv6: None,
                client_type: None,
                active: e.active,
            };
            if let Some(m) = &d.mac {
                if let Some(enr) = by_mac.get(&canon_mac(m)) {
                    d.ip = d.ip.clone().or_else(|| enr.ip.clone());
                    d.hostname = d.hostname.clone().or_else(|| enr.hostname.clone());
                    d.client_type = d.client_type.clone().or_else(|| enr.client_type.clone());
                    d.ipv6 = d.ipv6.clone().or_else(|| enr.ipv6.clone());
                    d.interface = d.interface.clone().or_else(|| enr.interface.clone());
                }
            }
            d
        })
        .collect();

    for (_, enr) in by_mac {
        let already = devices.iter().any(|d| {
            d.mac.as_ref().map(|m| canon_mac(m)) == enr.mac.as_ref().map(|m| canon_mac(m))
        });
        if !already {
            devices.push(enr);
        }
    }

    let mut raw = std::collections::HashMap::new();
    for (k, v) in [
        (oid::WIFI_APDEV_ASSOCDEV, assoc_json),
        (oid::DHCPV4_CLIENT, dhcp_json),
        (oid::HOST_ENTRY, host_json),
    ] {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(v) {
            raw.insert(k.to_string(), val);
        }
    }

    Ok(NetworkMap {
        captured_at: now(),
        devices,
        raw,
    })
}

/// Parse an association timestamp. The live EX520V returns RFC3339 strings such
/// as `2026-08-22T17:16:34-03:00`.  If RFC3339 parsing fails, fall back to a
/// plain Unix timestamp string so older fixtures and mocks keep working.  Returns
/// `None` when the field is missing or unparseable.
fn parse_timestamp(s: &str) -> Option<i64> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp());
    }
    s.parse::<i64>().ok()
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Canonicalize a MAC address for matching: lowercase, colon-separated.
pub fn canon_mac(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if cleaned.len() == 12 {
        let bytes: Vec<&str> = cleaned
            .as_bytes()
            .chunks(2)
            .map(|b| std::str::from_utf8(b).unwrap())
            .collect();
        return bytes.join(":");
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_malformed_assoc_data() {
        // The live response has `data` as an array; a legacy/wrong `data` map
        // must fail cleanly rather than be misinterpreted.
        let assoc = r#"{"data":{"ASSOCDEV":[{"MACAddress":"AA:BB:CC:11:22:33"}]},"operation":"gl","oid":"DEV2_WIFI_APDEV_ASSOCDEV","success":true}"#;
        let dhcp = r#"{"data":[],"operation":"gl","oid":"DEV2_DHCPV4_CLIENT","success":true}"#;
        let host = r#"{"data":[],"operation":"gl","oid":"DEV2_HOST_ENTRY","success":true}"#;
        let err = parse_network_map(assoc, dhcp, host)
            .unwrap_err()
            .to_string();
        assert!(err.contains("assocdev"));
    }

    #[test]
    fn canon_mac_normalizes_variants() {
        assert_eq!(canon_mac("AA:BB:CC:11:22:33"), "aa:bb:cc:11:22:33");
        assert_eq!(canon_mac("aabbcc112233"), "aa:bb:cc:11:22:33");
        assert_eq!(canon_mac("AA-BB-CC-11-22-33"), "aa:bb:cc:11:22:33");
    }

    #[test]
    fn parse_merges_three_oids() {
        let assoc = r#"{"data":[{"X_TP_HostName":"phone","X_TP_IPAddress":"192.168.0.20","MACAddress":"AA:BB:CC:11:22:33","X_TP_RadioMac":"00:11:22:33:44:55","X_TP_BssMac":"00:11:22:33:44:66","X_TP_ApDeviceMac":"00:11:22:33:44:77","operatingStandard":"ax","signalStrength":"50","active":"1","associationTime":"2026-08-22T17:16:34-03:00","lastDataDownlinkRate":"26000","lastDataUplinkRate":"52000","X_TP_SignalStrengthLevel":"4","X_TP_MaxLinkRate":"72000","noise":"50","steeringHistoryNumberOfEntries":"0","stack":"1,1,2,1,0,0"}],"operation":"gl","oid":"DEV2_WIFI_APDEV_ASSOCDEV","success":true}"#;
        let dhcp = r#"{"data":[{"MACAddress":"AA:BB:CC:11:22:33","IPAddress":"192.168.0.20","hostname":"phone-dhcp"}],"operation":"gl","oid":"DEV2_DHCPV4_CLIENT","success":true}"#;
        let host = r#"{"data":[],"operation":"gl","oid":"DEV2_HOST_ENTRY","success":true}"#;
        let m = parse_network_map(assoc, dhcp, host).unwrap();
        assert_eq!(m.devices.len(), 1);
        let d = &m.devices[0];
        assert_eq!(d.source.as_deref(), Some("wifi"));
        assert_eq!(d.rssi, Some(50));
        assert_eq!(d.mac.as_deref(), Some("AA:BB:CC:11:22:33"));
        assert_eq!(d.hostname.as_deref(), Some("phone"));
        assert_eq!(d.radio_mac.as_deref(), Some("00:11:22:33:44:55"));
        assert_eq!(d.assoc_time, Some(1_787_429_794));
    }

    #[test]
    fn parse_enriches_wifi_from_host() {
        // Wi-Fi has the MAC and RSSI but no hostname/IP; host provides them.
        let assoc = r#"{"data":[{"MACAddress":"AA:BB:CC:11:22:33","signalStrength":"55","associationTime":"2026-08-22T17:16:34-03:00","stack":"1,1,2,1,0,0"}],"operation":"gl","oid":"DEV2_WIFI_APDEV_ASSOCDEV","success":true}"#;
        let dhcp = r#"{"data":[],"operation":"gl","oid":"DEV2_DHCPV4_CLIENT","success":true}"#;
        let host = r#"{"data":[{"hostName":"merged-host","physAddress":"AA:BB:CC:11:22:33","IPAddress":"192.168.0.25"}],"operation":"gl","oid":"DEV2_HOST_ENTRY","success":true}"#;
        let m = parse_network_map(assoc, dhcp, host).unwrap();
        assert_eq!(m.devices.len(), 1);
        let d = &m.devices[0];
        assert_eq!(d.source.as_deref(), Some("wifi"));
        assert_eq!(d.rssi, Some(55));
        assert_eq!(d.hostname.as_deref(), Some("merged-host"));
        assert_eq!(d.ip.as_deref(), Some("192.168.0.25"));
    }

    #[test]
    fn parse_surfaces_dhcp_only_device() {
        let assoc =
            r#"{"data":[],"operation":"gl","oid":"DEV2_WIFI_APDEV_ASSOCDEV","success":true}"#;
        let dhcp = r#"{"data":[{"MACAddress":"AA:BB:CC:00:00:01","IPAddress":"192.168.0.30","hostname":"laptop"}],"operation":"gl","oid":"DEV2_DHCPV4_CLIENT","success":true}"#;
        let host = r#"{"data":[],"operation":"gl","oid":"DEV2_HOST_ENTRY","success":true}"#;
        let m = parse_network_map(assoc, dhcp, host).unwrap();
        assert_eq!(m.devices.len(), 1);
        assert_eq!(m.devices[0].source.as_deref(), Some("dhcp"));
    }

    #[test]
    fn parse_surfaces_ethernet_only_host() {
        let assoc =
            r#"{"data":[],"operation":"gl","oid":"DEV2_WIFI_APDEV_ASSOCDEV","success":true}"#;
        let dhcp = r#"{"data":[],"operation":"gl","oid":"DEV2_DHCPV4_CLIENT","success":true}"#;
        let host = r#"{"data":[{"hostName":"desktop","physAddress":"DD:EE:FF:00:00:01","IPAddress":"192.168.0.40"}],"operation":"gl","oid":"DEV2_HOST_ENTRY","success":true}"#;
        let m = parse_network_map(assoc, dhcp, host).unwrap();
        assert_eq!(m.devices.len(), 1);
        assert_eq!(m.devices[0].source.as_deref(), Some("host"));
        assert_eq!(m.devices[0].mac.as_deref(), Some("DD:EE:FF:00:00:01"));
    }

    #[test]
    fn parse_rfc3339_association_time() {
        let assoc = r#"{"data":[{"MACAddress":"AA:BB:CC:11:22:33","signalStrength":"60","associationTime":"2026-08-22T17:16:34-03:00","stack":"1,1,2,1,0,0"}],"operation":"gl","oid":"DEV2_WIFI_APDEV_ASSOCDEV","success":true}"#;
        let dhcp = r#"{"data":[],"operation":"gl","oid":"DEV2_DHCPV4_CLIENT","success":true}"#;
        let host = r#"{"data":[],"operation":"gl","oid":"DEV2_HOST_ENTRY","success":true}"#;
        let m = parse_network_map(assoc, dhcp, host).unwrap();
        assert_eq!(m.devices[0].assoc_time, Some(1_787_429_794));
    }

    #[test]
    fn parse_missing_and_invalid_association_time() {
        let assoc = r#"{"data":[{"MACAddress":"AA:BB:CC:11:22:33","signalStrength":"60","associationTime":"not-a-timestamp","stack":"1,1,2,1,0,0"},{"MACAddress":"AA:BB:CC:11:22:44","signalStrength":"70","stack":"1,1,2,1,0,0"}],"operation":"gl","oid":"DEV2_WIFI_APDEV_ASSOCDEV","success":true}"#;
        let dhcp = r#"{"data":[],"operation":"gl","oid":"DEV2_DHCPV4_CLIENT","success":true}"#;
        let host = r#"{"data":[],"operation":"gl","oid":"DEV2_HOST_ENTRY","success":true}"#;
        let m = parse_network_map(assoc, dhcp, host).unwrap();
        assert_eq!(m.devices.len(), 2);
        // First device has an unparseable timestamp -> None, no panic.
        assert_eq!(m.devices[0].assoc_time, None);
        // Second device is missing the field -> None.
        assert_eq!(m.devices[1].assoc_time, None);
    }

    #[test]
    fn parse_multiple_wifi_devices_no_duplicates() {
        let assoc = r#"{"data":[{"X_TP_HostName":"phone1","X_TP_IPAddress":"192.168.0.21","MACAddress":"AA:BB:CC:00:00:01","signalStrength":"40","associationTime":"2026-08-22T17:16:34-03:00","stack":"1,1,2,1,0,0"},{"X_TP_HostName":"phone2","X_TP_IPAddress":"192.168.0.22","MACAddress":"AA:BB:CC:00:00:02","signalStrength":"70","associationTime":"2026-08-22T17:16:34-03:00","stack":"1,1,2,1,0,0"}],"operation":"gl","oid":"DEV2_WIFI_APDEV_ASSOCDEV","success":true}"#;
        let dhcp = r#"{"data":[],"operation":"gl","oid":"DEV2_DHCPV4_CLIENT","success":true}"#;
        // One host matches a Wi-Fi MAC; another is Ethernet-only.
        let host = r#"{"data":[{"hostName":"host1","physAddress":"AA:BB:CC:00:00:01","IPAddress":"192.168.0.21"},{"hostName":"eth-printer","physAddress":"11:22:33:00:00:99","IPAddress":"192.168.0.50"}],"operation":"gl","oid":"DEV2_HOST_ENTRY","success":true}"#;
        let m = parse_network_map(assoc, dhcp, host).unwrap();
        assert_eq!(m.devices.len(), 3);

        let by_mac: std::collections::HashMap<_, _> = m
            .devices
            .iter()
            .map(|d| (d.mac.as_deref().unwrap(), d))
            .collect();
        assert_eq!(by_mac.len(), 3);
        assert_eq!(
            by_mac.get("AA:BB:CC:00:00:01").unwrap().hostname.as_deref(),
            Some("phone1")
        );
        assert_eq!(
            by_mac.get("AA:BB:CC:00:00:02").unwrap().hostname.as_deref(),
            Some("phone2")
        );
        assert_eq!(
            by_mac.get("11:22:33:00:00:99").unwrap().source.as_deref(),
            Some("host")
        );
        assert_eq!(
            by_mac.get("11:22:33:00:00:99").unwrap().hostname.as_deref(),
            Some("eth-printer")
        );
    }

    #[test]
    fn collect_via_fake_transport() {
        struct Fake;
        impl Transport for Fake {
            fn gl(&self, oid: &str) -> Result<String, GtprError> {
                match oid {
                    oid::WIFI_APDEV_ASSOCDEV => Ok(
                        r#"{"data":[{"MACAddress":"AA:BB:CC:00:00:01","signalStrength":"42","stack":"1,1,2,1,0,0"}],"operation":"gl","oid":"DEV2_WIFI_APDEV_ASSOCDEV","success":true}"#
                            .into(),
                    ),
                    oid::DHCPV4_CLIENT => Ok(r#"{"data":[],"operation":"gl","oid":"DEV2_DHCPV4_CLIENT","success":true}"#.into()),
                    oid::HOST_ENTRY => Ok(r#"{"data":[],"operation":"gl","oid":"DEV2_HOST_ENTRY","success":true}"#.into()),
                    _ => Err(GtprError::Protocol("unknown oid".into())),
                }
            }
        }
        let m = collect(&Fake).unwrap();
        assert_eq!(m.devices.len(), 1);
        assert_eq!(m.devices[0].rssi, Some(42));
    }
}
