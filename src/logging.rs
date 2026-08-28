//! Lightweight structured logging (M5-I).
//!
//! Never logs passwords, API secrets, authentication tokens, or private
//! credentials. MAC addresses are only logged when explicitly enabled
//! (`config.log_macs`).

use crate::config::LogLevel;
use chrono::{SecondsFormat, Utc};
use std::sync::atomic::{AtomicU8, Ordering};

static LOG_LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Info as u8);

/// Set the global log level.
pub fn set_level(level: LogLevel) {
    LOG_LEVEL.store(level as u8, Ordering::Relaxed);
}

fn current_level() -> LogLevel {
    match LOG_LEVEL.load(Ordering::Relaxed) {
        0 => LogLevel::Error,
        1 => LogLevel::Warn,
        2 => LogLevel::Info,
        _ => LogLevel::Debug,
    }
}

/// Log an info-level message.
pub fn info(msg: &str) {
    if current_level() >= LogLevel::Info {
        eprintln!(
            "{} INFO {}",
            Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true),
            msg
        );
    }
}

/// Log a warn-level message.
pub fn warn(msg: &str) {
    if current_level() >= LogLevel::Warn {
        eprintln!(
            "{} WARN {}",
            Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true),
            msg
        );
    }
}

/// Log an error-level message.
pub fn error(msg: &str) {
    if current_level() >= LogLevel::Error {
        eprintln!(
            "{} ERROR {}",
            Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true),
            msg
        );
    }
}

/// Log a debug-level message.
pub fn debug(msg: &str) {
    if current_level() >= LogLevel::Debug {
        eprintln!(
            "{} DEBUG {}",
            Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true),
            msg
        );
    }
}

/// Format a MAC for logging, respecting the `log_macs` privacy flag.
/// When `log_macs` is false, returns a pseudonymized prefix.
pub fn fmt_mac(mac: &str, log_macs: bool) -> String {
    if log_macs {
        mac.to_string()
    } else {
        // Show only the OUI prefix (first 3 octets) for diagnostics;
        // mask the remaining octets with the same count as the input.
        let parts: Vec<&str> = mac.split(|c| c == ':' || c == '-').collect();
        if parts.len() >= 3 {
            let masked: Vec<String> = (3..parts.len()).map(|_| "**".to_string()).collect();
            let mut out: Vec<&str> = parts[0..3].iter().copied().collect();
            for m in &masked {
                out.push(m.as_str());
            }
            out.join(":")
        } else {
            "**:**:**:**:**:**".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_mac_redacts_when_disabled() {
        assert_eq!(fmt_mac("AA:BB:CC:11:22:33", false), "AA:BB:CC:**:**:**");
    }

    #[test]
    fn fmt_mac_shows_when_enabled() {
        assert_eq!(fmt_mac("AA:BB:CC:11:22:33", true), "AA:BB:CC:11:22:33");
    }

    #[test]
    fn fmt_mac_handles_short_input() {
        assert_eq!(fmt_mac("short", false), "**:**:**:**:**:**");
    }
}
