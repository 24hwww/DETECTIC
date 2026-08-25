//! Persistent launcher abstraction (M10-F).
//!
//! Investigates legitimate startup mechanisms on the EX520V stock firmware
//! and provides a `PersistentLauncher` trait so the core sensor code does not
//! need to know which mechanism is in use.
//!
//! ## Findings (stock EX520V firmware)
//!
//! - No vendor startup hook for user binaries is documented or exposed.
//! - The stock EX520V uses BusyBox init (not `procd`/`OpenWrt`); `/etc/init.d/`
//!   lives on read-only squashfs, so adding a service requires firmware
//!   modification (forbidden).
//! - No supported persistent service API is exposed to user-space.
//! - The Lifemote agent can bootstrap a shell, but it is a debugging feature
//!   and using it for permanent persistence would be inappropriate.
//!
//! ## Conclusion
//!
//! `AUTO_START_SUPPORTED = false` on stock firmware. The only legitimate
//! mode is `StockManual` (operator starts the sensor after each reboot).
//! If a vendor/ISP later provides a supported hook, a new variant can be
//! added without changing the core. The four enumerated modes describe the
//! full design space; only `StockManual` is realizable without firmware
//! modification.

use serde::{Deserialize, Serialize};

/// Persistence mode.
///
/// Enumerates the complete design space of legitimate boot mechanisms. On the
/// stock EX520V only `StockManual` is achievable without modifying firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchMode {
    /// Operator manually starts the sensor after each reboot (stock default).
    StockManual,
    /// A vendor/ISP-provided service hook starts the sensor automatically.
    /// Not present on stock EX520V (`/etc/init.d/*` is read-only squashfs).
    VendorService,
    /// `procd`/`OpenWrt`-style init. Not applicable: stock EX520V uses
    /// BusyBox init, not procd. Listed for completeness.
    Procd,
    /// A vendor/ISP-provided service hook starts the sensor automatically
    /// (alias kept for backward compatibility with prior naming).
    SupportedService,
    /// An external launcher (e.g. a separate always-on device) supervises
    /// the sensor and restarts it after reboots.
    ExternalLauncher,
}

impl Default for LaunchMode {
    fn default() -> Self {
        LaunchMode::StockManual
    }
}

/// Result of probing the available launch mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchProbe {
    pub mode: LaunchMode,
    pub auto_start_supported: bool,
    pub reason: String,
}

/// Probe the current firmware for a legitimate launch mechanism.
///
/// This function only reads; it never modifies init scripts, squashfs, or
/// firmware configuration.
pub fn probe_launch_mode() -> LaunchProbe {
    // Check for a vendor-provided init hook (none exists on stock EX520V).
    let has_init_d = std::path::Path::new("/etc/init.d/detectic").exists();
    if has_init_d {
        return LaunchProbe {
            mode: LaunchMode::SupportedService,
            auto_start_supported: true,
            reason: "/etc/init.d/detectic found".into(),
        };
    }

    // No supported hook — stock manual mode.
    LaunchProbe {
        mode: LaunchMode::StockManual,
        auto_start_supported: false,
        reason: "no vendor startup hook available on stock firmware".into(),
    }
}

/// Render the launch probe as a human-readable status line.
pub fn launch_status() -> String {
    let p = probe_launch_mode();
    format!(
        "auto_start_supported={} mode={:?} reason={}",
        p.auto_start_supported, p.mode, p.reason
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_probe_returns_manual() {
        // On the development host there is no /etc/init.d/detectic.
        let p = probe_launch_mode();
        assert_eq!(p.mode, LaunchMode::StockManual);
        assert!(!p.auto_start_supported);
    }

    #[test]
    fn launch_status_is_nonempty() {
        assert!(!launch_status().is_empty());
    }
}
