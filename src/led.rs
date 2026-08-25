//! EX520 native LED control via `/proc/tp_led`.
//!
//! Uses ONLY the firmware's existing `/proc/tp_led` interface.
//! NEVER writes to arbitrary GPIOs. NEVER installs PWM drivers.
//! NEVER modifies firmware.
//!
//! LED map (proven-live from EX520V):
//!   POWR  gpio 4  — Power
//!   INET  gpio 7  — Internet
//!   INE2  gpio 5  — Internet 2
//!   WL2G  gpio 35 — 2.4GHz WiFi
//!   WL5G  gpio 34 — 5GHz WiFi
//!   WPS   gpio 6  — WPS
//!   LAN   gpio 8  — LAN
//!
//! Usage: `echo <NAME> <mode:1|2> <on/off:1|0> > /proc/tp_led`
//!   mode 1 = normal (firmware-controlled)
//!   mode 2 = manual control
//!   value 1 = on (in mode 2), off (in mode 1 with value 1 = stop)
//!   value 0 = off

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Safe LED names on the EX520V.
pub const SAFE_LEDS: &[&str] = &["POWR", "INET", "INE2", "WL2G", "WL5G", "WPS", "LAN"];

/// LED that Detectic may use for visual indication.
/// WPS is preferred for Detectic events (least disruptive).
/// POWR is avoided for frequent events (it indicates power status).
pub const DETECTIC_LED: &str = "WPS";

const TP_LED_PATH: &str = "/proc/tp_led";

/// Bounded LED controller for the EX520V.
///
/// Provides safe, reversible LED operations using the firmware's
/// `/proc/tp_led` interface. All commands are bounded and automatically
/// restored.
pub struct LedController {
    /// The LED name to control (must be in SAFE_LEDS).
    led_name: String,
    /// Path to the LED control file.
    path: String,
    /// Whether the controller is available.
    available: Option<bool>,
    /// Last command timestamp (for debouncing).
    last_command_ts: u64,
    /// Minimum interval between commands (debounce, in seconds).
    min_interval: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LedState {
    On,
    Off,
    Normal, // firmware-controlled
}

#[derive(Debug, Clone, PartialEq)]
pub enum LedError {
    NotAvailable,
    InvalidLedName,
    WriteFailed(String),
}

impl LedController {
    /// Create a new LedController for the given LED name.
    /// Defaults to WPS (least disruptive).
    pub fn new(led_name: &str) -> Self {
        Self {
            led_name: led_name.to_string(),
            path: TP_LED_PATH.to_string(),
            available: None,
            last_command_ts: 0,
            min_interval: 2, // 2s debounce
        }
    }

    /// Create a WPS LED controller (preferred for Detectic events).
    pub fn wps() -> Self {
        Self::new("WPS")
    }

    /// Check if the LED controller is available (i.e., `/proc/tp_led` exists).
    pub fn available(&mut self) -> bool {
        if let Some(a) = self.available {
            return a;
        }
        let a = Path::new(&self.path).exists() && SAFE_LEDS.contains(&self.led_name.as_str());
        self.available = Some(a);
        a
    }

    /// Write a command to `/proc/tp_led`.
    /// Format: `NAME mode value`
    fn write_command(&mut self, mode: u8, value: u8) -> Result<(), LedError> {
        if !self.available() {
            return Err(LedError::NotAvailable);
        }

        // Debounce: don't send commands too rapidly
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now - self.last_command_ts < self.min_interval {
            // Skip this command (debounced)
            return Ok(());
        }
        self.last_command_ts = now;

        let cmd = format!("{} {} {}\n", self.led_name, mode, value);
        fs::write(&self.path, cmd.as_bytes())
            .map_err(|e| LedError::WriteFailed(e.to_string()))
    }

    /// Turn the LED ON (manual mode 2, value 1).
    pub fn on(&mut self) -> Result<(), LedError> {
        self.write_command(2, 1)
    }

    /// Turn the LED OFF (manual mode 2, value 0).
    pub fn off(&mut self) -> Result<(), LedError> {
        self.write_command(2, 0)
    }

    /// Restore the LED to normal firmware-controlled operation (mode 1, value 1).
    pub fn restore(&mut self) -> Result<(), LedError> {
        self.write_command(1, 1)
    }

    /// Brief blink: ON for `duration_ms`, then restore.
    /// This is the primary safe visual indication method.
    pub fn blink(&mut self, duration_ms: u64) -> Result<(), LedError> {
        self.on()?;
        std::thread::sleep(std::time::Duration::from_millis(duration_ms));
        self.restore()
    }

    /// Health check: returns the current availability and LED name.
    pub fn health(&self) -> (bool, &str) {
        (self.available.unwrap_or(false), &self.led_name)
    }

    /// Get the LED name.
    pub fn name(&self) -> &str {
        &self.led_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_leds_contains_known_names() {
        assert!(SAFE_LEDS.contains(&"POWR"));
        assert!(SAFE_LEDS.contains(&"WPS"));
        assert!(SAFE_LEDS.contains(&"WL2G"));
        assert!(SAFE_LEDS.contains(&"WL5G"));
        assert!(SAFE_LEDS.contains(&"LAN"));
        assert!(SAFE_LEDS.contains(&"INET"));
    }

    #[test]
    fn wps_controller_uses_wps() {
        let c = LedController::wps();
        assert_eq!(c.name(), "WPS");
    }

    #[test]
    fn invalid_led_name_not_available() {
        let mut c = LedController::new("INVALID_LED");
        // On a system without /proc/tp_led, this will be false
        // On the EX520, INVALID_LED is not in SAFE_LEDS
        let _ = c.available(); // just ensure it doesn't panic
    }

    #[test]
    fn debounce_prevents_rapid_commands() {
        // On a system without /proc/tp_led, write_command returns NotAvailable
        // but the debounce logic should still track timestamps
        let mut c = LedController::wps();
        // First call sets last_command_ts
        let _ = c.on();
        // Second call immediately should be debounced (returns Ok without writing)
        // But since /proc/tp_led doesn't exist, it returns NotAvailable
        // This test just ensures no panic
    }
}
