use serde::{Deserialize, Serialize};
use std::fmt;

pub mod config;
pub mod queue;
pub mod rate_limit;
pub mod smtp;
pub mod template;

pub use config::SmtpConfig;
pub use queue::{Email, PendingEmail, SmtpQueue};
pub use rate_limit::{RateLimitConfig, RateLimiter};
pub use smtp::{RustlsSmtpTransport, SmtpNotifier, SmtpTransport};
pub use template::{mask_mac, rcpi_to_dbm, EmailTemplate};

pub use crate::events::EventKind;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DetectionEvent {
    pub captured_at: i64,
    pub kind: EventKind,
    pub pseudonym: String,
    pub changed_fields: Vec<String>,
    pub hostname: Option<String>,
    pub ip: Option<String>,
    pub mac: Option<String>,
    pub rssi_dbm: Option<i32>,
    pub rcpi: Option<u32>,
    pub band: Option<String>,
    pub channel: Option<u8>,
    pub source: Option<String>,
    pub distance_m: Option<f32>,
    /// Whether the device is currently associated to the EX520.
    #[serde(default)]
    pub connected: bool,
    /// Whether the device is currently active/present.
    #[serde(default)]
    pub active: bool,
    /// Proximity classification: "Muito perto", "Perto", "Distancia media", "Longe", "Incerto", "Cabo".
    #[serde(default)]
    pub proximity: String,
    /// Signal quality label: "Excelente", "Bom", "Regular", "Fraco", "N/A".
    #[serde(default)]
    pub signal_quality: String,
    /// Thermal heat 0-100 (cold = far/weak, hot = near/strong).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heat: Option<u8>,
    /// Number of devices detected in this capture (for dashboard).
    #[serde(default)]
    pub total_devices: u32,
    /// Number of connected devices.
    #[serde(default)]
    pub connected_count: u32,
    /// Number of not-connected but detected devices.
    #[serde(default)]
    pub not_connected_count: u32,
}

#[derive(Debug)]
pub enum SmtpError {
    Io(std::io::Error),
    Tls(rustls::Error),
    InvalidHost,
    Smtp(String),
    Disabled,
    Queue(rusqlite::Error),
    Serialization(serde_json::Error),
    Config(String),
}

impl fmt::Display for SmtpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SmtpError::Io(e) => write!(f, "io error: {e}"),
            SmtpError::Tls(e) => write!(f, "tls error: {e}"),
            SmtpError::InvalidHost => write!(f, "invalid smtp host"),
            SmtpError::Smtp(s) => write!(f, "smtp error: {s}"),
            SmtpError::Disabled => write!(f, "smtp is disabled"),
            SmtpError::Queue(e) => write!(f, "queue error: {e}"),
            SmtpError::Serialization(e) => write!(f, "serialization error: {e}"),
            SmtpError::Config(s) => write!(f, "config error: {s}"),
        }
    }
}

impl std::error::Error for SmtpError {}

impl From<std::io::Error> for SmtpError {
    fn from(e: std::io::Error) -> Self {
        SmtpError::Io(e)
    }
}

impl From<rustls::Error> for SmtpError {
    fn from(e: rustls::Error) -> Self {
        SmtpError::Tls(e)
    }
}

impl From<rusqlite::Error> for SmtpError {
    fn from(e: rusqlite::Error) -> Self {
        SmtpError::Queue(e)
    }
}

impl From<serde_json::Error> for SmtpError {
    fn from(e: serde_json::Error) -> Self {
        SmtpError::Serialization(e)
    }
}

pub trait Notifier {
    fn send(&self, event: &DetectionEvent) -> Result<(), SmtpError>;
}

pub struct NullNotifier;

impl Notifier for NullNotifier {
    fn send(&self, _event: &DetectionEvent) -> Result<(), SmtpError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_event_roundtrips() {
        let e = DetectionEvent {
            captured_at: 1,
            kind: EventKind::DeviceJoined,
            pseudonym: "p1".into(),
            changed_fields: vec![],
            hostname: Some("phone".into()),
            ip: None,
            mac: Some("AA:BB:CC:DD:EE:FF".into()),
            rssi_dbm: Some(-55),
            rcpi: Some(104),
            band: Some("2.4G".into()),
            channel: Some(6),
            source: Some("wifi".into()),
            distance_m: Some(2.5),
            connected: true,
            active: true,
            proximity: "Perto".into(),
            heat: Some(75),
            signal_quality: "Bom".into(),
            total_devices: 5,
            connected_count: 5,
            not_connected_count: 0,
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: DetectionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn null_notifier_is_noop() {
        let n = NullNotifier;
        let e = DetectionEvent {
            captured_at: 1,
            kind: EventKind::DeviceJoined,
            pseudonym: "p1".into(),
            changed_fields: vec![],
            hostname: None,
            ip: None,
            mac: None,
            rssi_dbm: None,
            rcpi: None,
            band: None,
            channel: None,
            source: None,
            distance_m: None,
            connected: false,
            active: false,
            proximity: "Incerto".into(),
            heat: None,
            signal_quality: "N/A".into(),
            total_devices: 0,
            connected_count: 0,
            not_connected_count: 0,
        };
        assert!(n.send(&e).is_ok());
    }
}
