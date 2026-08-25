//! GTPR OID definitions and the raw response shapes returned by the EX520.

use serde::Deserialize;

/// OIDs known to work on the EX520 firmware (see ex520-network-map-gdpr.md).
pub mod oid {
    pub const WIFI_APDEV_ASSOCDEV: &str = "DEV2_WIFI_APDEV_ASSOCDEV";
    pub const DHCPV4_CLIENT: &str = "DEV2_DHCPV4_CLIENT";
    pub const HOST_ENTRY: &str = "DEV2_HOST_ENTRY";
    pub const HOSTS: &str = "DEV2_HOSTS";
}

/// Raw shape of one entry inside `DEV2_WIFI_APDEV_ASSOCDEV`.
///
/// The live EX520V response uses `X_TP_*` keys and string values.  Aliases
/// keep the struct compatible with older fixtures and mock data.
#[derive(Debug, Clone, Deserialize)]
pub struct AssocDevEntry {
    #[serde(rename = "X_TP_HostName", alias = "hostname", default)]
    pub hostname: Option<String>,
    #[serde(rename = "X_TP_IPAddress", alias = "IPAddress", default)]
    pub ip: Option<String>,
    #[serde(rename = "MACAddress", default)]
    pub mac: Option<String>,
    #[serde(rename = "X_TP_RadioMac", alias = "radioMAC", default)]
    pub radio_mac: Option<String>,
    #[serde(rename = "X_TP_BssMac", default)]
    pub bss_mac: Option<String>,
    #[serde(rename = "X_TP_ApDeviceMac", default)]
    pub ap_device_mac: Option<String>,
    #[serde(rename = "operatingStandard", alias = "opStandard", default)]
    pub standard: Option<String>,
    #[serde(rename = "signalStrength", default)]
    pub rssi: Option<String>,
    #[serde(rename = "associationTime", alias = "assocTime", default)]
    pub assoc_time: Option<String>,
    #[serde(rename = "lastDataDownlinkRate", default)]
    pub last_data_downlink_rate: Option<String>,
    #[serde(rename = "lastDataUplinkRate", default)]
    pub last_data_uplink_rate: Option<String>,
    #[serde(rename = "X_TP_SignalStrengthLevel", default)]
    pub signal_strength_level: Option<String>,
    #[serde(rename = "X_TP_MaxLinkRate", default)]
    pub max_link_rate: Option<String>,
    #[serde(rename = "noise", default)]
    pub noise: Option<String>,
    #[serde(rename = "stack", default)]
    pub stack: Option<String>,
    #[serde(rename = "active", default)]
    pub active: Option<String>,
}

/// Top-level decrypted payload for a `gl` of `DEV2_WIFI_APDEV_ASSOCDEV`.
///
/// The live response has `data` as an array of entries, not as a map.
#[derive(Debug, Clone, Deserialize)]
pub struct AssocDevResponse {
    #[serde(rename = "data", default)]
    pub data: Vec<AssocDevEntry>,
    #[serde(rename = "operation", default)]
    pub operation: Option<String>,
    #[serde(rename = "oid", default)]
    pub oid: Option<String>,
    #[serde(rename = "success", default)]
    pub success: Option<bool>,
}

/// One entry from `DEV2_DHCPV4_CLIENT` (DHCP lease table / WAN client).
#[derive(Debug, Clone, Deserialize)]
pub struct DhcpClientEntry {
    #[serde(rename = "MACAddress", alias = "physAddress", default)]
    pub mac: Option<String>,
    #[serde(rename = "IPAddress", alias = "X_TP_IPAddress", default)]
    pub ip: Option<String>,
    #[serde(
        rename = "hostname",
        alias = "X_TP_Hostname",
        alias = "hostName",
        default
    )]
    pub hostname: Option<String>,
}

/// Top-level decrypted payload for a `gl` of `DEV2_DHCPV4_CLIENT`.
///
/// Live captures show `data` as an array.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct DhcpClientResponse {
    #[serde(rename = "data", default)]
    pub data: Vec<DhcpClientEntry>,
    #[serde(rename = "operation", default)]
    pub operation: Option<String>,
    #[serde(rename = "oid", default)]
    pub oid: Option<String>,
    #[serde(rename = "success", default)]
    pub success: Option<bool>,
}

/// One entry from `DEV2_HOST_ENTRY` (ARP/host table).
#[derive(Debug, Clone, Deserialize)]
pub struct HostEntry {
    #[serde(rename = "hostName", alias = "hostname", default)]
    pub hostname: Option<String>,
    #[serde(rename = "physAddress", alias = "MACAddress", default)]
    pub mac: Option<String>,
    #[serde(rename = "IPAddress", default)]
    pub ip: Option<String>,
    // --- M5 extended fields from DEV2_HOST_ENTRY ---
    #[serde(rename = "X_TP_ClientType", default)]
    pub client_type: Option<String>,
    #[serde(rename = "X_TP_IPv6Address", default)]
    pub ipv6: Option<String>,
    #[serde(rename = "X_TP_Layer2Interface", default)]
    pub layer2_interface: Option<String>,
    #[serde(rename = "interfaceType", default)]
    pub interface_type: Option<String>,
    #[serde(rename = "active", default)]
    pub active: Option<String>,
}

/// Top-level decrypted payload for a `gl` of `DEV2_HOST_ENTRY`.
///
/// Live captures show `data` as an array.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct HostEntryResponse {
    #[serde(rename = "data", default)]
    pub data: Vec<HostEntry>,
    #[serde(rename = "operation", default)]
    pub operation: Option<String>,
    #[serde(rename = "oid", default)]
    pub oid: Option<String>,
    #[serde(rename = "success", default)]
    pub success: Option<bool>,
}
