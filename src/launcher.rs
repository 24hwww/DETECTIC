//! Detectic launcher abstraction (M11-E).
//!
//! Provides a single, legitimate entry point for installing/removing/verifying
//! the Detectic sensor as a persistent service. On the stock EX520V the **only**
//! supported mode is [`LaunchMode::StockManual`]: the operator starts the sensor
//! after each reboot. All other modes are described for completeness but their
//! `install()` is a safe no-op / explicit refusal because they require firmware
//! modification, which is forbidden by the project safety rules.
//!
//! The CLI default is **dev/client mode** (no persistence, no router changes).
//! Nothing here ever reboots, flashes, or alters partitions, init scripts, or
//! network configuration.

use crate::persistence::{probe_launch_mode, LaunchMode, LaunchProbe};
use std::fmt;

/// Result of a launcher operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LauncherResult {
    /// Operation succeeded and a persistent install exists.
    Installed(LaunchMode),
    /// Operation succeeded but no persistence was performed (manual mode).
    ManualOnly(LaunchMode),
    /// Operation refused because it would require firmware modification.
    Refused(LaunchMode, String),
    /// Operation removed an existing install.
    Removed(LaunchMode),
    /// Status query result.
    Status(LaunchProbe),
}

impl fmt::Display for LauncherResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LauncherResult::Installed(m) => write!(f, "installed ({:?})", m),
            LauncherResult::ManualOnly(m) => {
                write!(f, "manual-only ({:?}); no router persistence performed", m)
            }
            LauncherResult::Refused(m, why) => write!(f, "refused ({:?}): {}", m, why),
            LauncherResult::Removed(m) => write!(f, "removed ({:?})", m),
            LauncherResult::Status(p) => write!(
                f,
                "auto_start_supported={} mode={:?} reason={}",
                p.auto_start_supported, p.mode, p.reason
            ),
        }
    }
}

/// The Detectic launcher.
///
/// `install()` / `remove()` are intentionally conservative: they refuse any
/// action that would modify the router's firmware or startup configuration.
pub struct DetecticLauncher {
    desired: LaunchMode,
}

impl Default for DetecticLauncher {
    /// Default is stock manual mode (dev/client, no persistence).
    fn default() -> Self {
        Self::new()
    }
}

impl DetecticLauncher {
    pub fn new() -> Self {
        Self {
            desired: LaunchMode::StockManual,
        }
    }

    /// Select a desired launch mode (still safe: only StockManual is honored).
    pub fn with_mode(mut self, mode: LaunchMode) -> Self {
        self.desired = mode;
        self
    }

    /// Probe what the firmware actually supports.
    pub fn probe(&self) -> LaunchProbe {
        probe_launch_mode()
    }

    /// Attempt to install persistence for the desired mode.
    ///
    /// Safe behavior:
    /// - `StockManual` → no persistence, returns `ManualOnly`.
    /// - `VendorService` / `Procd` → refused (read-only squashfs, would need
    ///   firmware modification).
    /// - `SupportedService` / `ExternalLauncher` → on stock firmware these
    ///   resolve to manual-only (no hook exists).
    pub fn install(&self) -> LauncherResult {
        match self.desired {
            LaunchMode::StockManual => LauncherResult::ManualOnly(LaunchMode::StockManual),
            LaunchMode::VendorService | LaunchMode::Procd => LauncherResult::Refused(
                self.desired,
                "requires firmware modification (read-only squashfs)".into(),
            ),
            LaunchMode::SupportedService => {
                let probe = self.probe();
                if probe.auto_start_supported {
                    LauncherResult::Installed(LaunchMode::SupportedService)
                } else {
                    LauncherResult::ManualOnly(LaunchMode::StockManual)
                }
            }
            LaunchMode::ExternalLauncher => {
                LauncherResult::ManualOnly(LaunchMode::ExternalLauncher)
            }
        }
    }

    /// Remove any previously installed persistence (no-op on stock firmware).
    pub fn remove(&self) -> LauncherResult {
        // Nothing is ever installed on stock firmware; safe no-op.
        LauncherResult::Removed(self.desired)
    }

    /// Verify the install state without modifying anything.
    pub fn verify(&self) -> LauncherResult {
        let probe = self.probe();
        LauncherResult::Status(probe)
    }

    /// Status query.
    pub fn status(&self) -> LauncherResult {
        LauncherResult::Status(self.probe())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_stock_manual() {
        let l = DetecticLauncher::default();
        assert_eq!(l.desired, LaunchMode::StockManual);
    }

    #[test]
    fn stock_manual_never_persists() {
        let l = DetecticLauncher::new().with_mode(LaunchMode::StockManual);
        match l.install() {
            LauncherResult::ManualOnly(m) => assert_eq!(m, LaunchMode::StockManual),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn firmware_modifying_modes_are_refused() {
        for m in [LaunchMode::VendorService, LaunchMode::Procd] {
            let l = DetecticLauncher::new().with_mode(m);
            match l.install() {
                LauncherResult::Refused(got, why) => {
                    assert_eq!(got, m);
                    assert!(!why.is_empty());
                }
                other => panic!("should refuse {:?}, got {:?}", m, other),
            }
        }
    }

    #[test]
    fn remove_is_safe_noop() {
        let l = DetecticLauncher::new();
        match l.remove() {
            LauncherResult::Removed(_) => {}
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn status_reports_probe() {
        let l = DetecticLauncher::new();
        match l.status() {
            LauncherResult::Status(p) => assert!(!p.reason.is_empty()),
            other => panic!("unexpected: {:?}", other),
        }
    }
}
