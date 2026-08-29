//! Configuration for the Detectic sensor runtime (M5-G).
//!
//! All configuration is sourced from environment variables (and optionally a
//! minimal key=value config file). No credentials are hardcoded — the router
//! URL defaults to the validated EX520V management address but is overridable.

use crate::presence::PresenceConfig;
use std::path::PathBuf;
use std::time::Duration;

/// Default polling interval (30 seconds, per M5-B).
pub const DEFAULT_INTERVAL: u64 = 30;

/// Default router URL (validated EX520V LAN address, per M4.4).
pub const DEFAULT_URL: &str = "http://192.168.0.1";

/// Default bounded spool size (256 KB).
pub const DEFAULT_SPOOL_MAX: u64 = 256 * 1024;

/// Default max stations per snapshot (prevents unbounded vectors, M5-H).
pub const MAX_STATIONS: usize = 256;

/// Default max nearby APs in site survey (M5-H).
pub const MAX_NEARBY_APS: usize = 512;

/// Default HTTP request timeout for router GTPR calls.
pub const DEFAULT_ROUTER_TIMEOUT: Duration = Duration::from_secs(15);

/// Default HTTP request timeout for backend uploads.
pub const DEFAULT_BACKEND_TIMEOUT: Duration = Duration::from_secs(10);

/// Default max retry attempts for GTPR poll failures.
pub const MAX_POLL_RETRIES: usize = 3;

/// Default max retry attempts for backend upload failures.
pub const MAX_UPLOAD_RETRIES: usize = 3;

/// Maximum HTTP response body size we will read (1 MB, M5-H).
pub const MAX_RESPONSE_BODY: usize = 1024 * 1024;

/// Log levels for structured logging (M5-I).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "error" => LogLevel::Error,
            "warn" => LogLevel::Warn,
            "info" => LogLevel::Info,
            "debug" => LogLevel::Debug,
            _ => LogLevel::Info,
        }
    }
}

/// Sensor configuration — all fields are configurable via env vars or config file.
#[derive(Debug, Clone)]
pub struct SensorConfig {
    // --- Router connection ---
    pub router_url: String,
    pub router_user: String,
    pub router_password: String,
    pub router_timeout: Duration,

    // --- Sensor identity ---
    pub sensor_id: String,
    pub secret: String,

    // --- Polling ---
    pub interval: Duration,

    // --- Backend ---
    pub backend_url: Option<String>,
    pub backend_token: Option<String>,
    pub backend_timeout: Duration,

    // --- Spool / offline buffer ---
    pub spool_path: PathBuf,
    pub spool_max_bytes: u64,

    // --- Optional data sources ---
    pub enable_site_survey: bool,
    pub enable_radio_stats: bool,

    // --- Presence / proximity (M6) ---
    pub presence: PresenceConfig,
    /// Stable sensor UUID path.
    pub sensor_id_path: PathBuf,

    // --- Logging ---
    pub log_level: LogLevel,
    /// If true, MAC addresses may appear in logs. Default false (M5-I).
    pub log_macs: bool,

    // --- Resource limits (M5-H) ---
    pub max_stations: usize,
    pub max_nearby_aps: usize,
    pub max_response_body: usize,
    pub max_poll_retries: usize,
    pub max_upload_retries: usize,

    // --- Local control plane ---
    pub http_port: u16,
    pub enable_http_server: bool,
    pub enable_mdns: bool,
    pub mdns_hostname: String,
    pub arp_interval: Duration,
    pub enable_arp_fastpath: bool,
}

impl Default for SensorConfig {
    fn default() -> Self {
        Self {
            router_url: DEFAULT_URL.into(),
            router_user: "user".into(),
            router_password: String::new(),
            router_timeout: DEFAULT_ROUTER_TIMEOUT,
            sensor_id: "ex520-001".into(),
            secret: String::new(),
            interval: Duration::from_secs(DEFAULT_INTERVAL),
            backend_url: None,
            backend_token: None,
            backend_timeout: DEFAULT_BACKEND_TIMEOUT,
            spool_path: PathBuf::from("/var/run/misc/misc_rw/detectic/spool/detectic_buffer.jsonl"),
            spool_max_bytes: DEFAULT_SPOOL_MAX,
            enable_site_survey: false,
            enable_radio_stats: false,
            presence: PresenceConfig::default(),
            sensor_id_path: PathBuf::from("/var/run/misc/misc_rw/detectic/state/sensor_id"),
            log_level: LogLevel::Info,
            log_macs: false,
            max_stations: MAX_STATIONS,
            max_nearby_aps: MAX_NEARBY_APS,
            max_response_body: MAX_RESPONSE_BODY,
            max_poll_retries: MAX_POLL_RETRIES,
            max_upload_retries: MAX_UPLOAD_RETRIES,
            http_port: 8787,
            enable_http_server: true,
            enable_mdns: true,
            mdns_hostname: "detectic".into(),
            arp_interval: Duration::from_secs(10),
            enable_arp_fastpath: true,
        }
    }
}

impl SensorConfig {
    /// Build configuration from environment variables, falling back to defaults
    /// and an optional key=value env file.
    /// Required: `DETECTIC_PASSWORD`, `DETECTIC_SECRET`.
    pub fn from_env() -> Self {
        let mut cfg = if let Ok(v) = std::env::var("DETECTIC_ENV_FILE") {
            Self::from_file(std::path::Path::new(&v))
        } else {
            Self::default()
        };

        if let Ok(v) = std::env::var("DETECTIC_URL") {
            cfg.router_url = v;
        }
        if let Ok(v) = std::env::var("DETECTIC_USER") {
            cfg.router_user = v;
        }
        if let Ok(v) = std::env::var("DETECTIC_PASSWORD") {
            cfg.router_password = v;
        }
        if let Ok(v) = std::env::var("DETECTIC_SECRET") {
            cfg.secret = v;
        }
        if let Ok(v) = std::env::var("DETECTIC_SENSOR_ID") {
            cfg.sensor_id = v;
        }
        if let Ok(v) = std::env::var("DETECTIC_INTERVAL") {
            if let Ok(secs) = v.parse::<u64>() {
                if secs > 0 {
                    cfg.interval = Duration::from_secs(secs);
                }
            }
        }
        if let Ok(v) = std::env::var("DETECTIC_ROUTER_TIMEOUT") {
            if let Ok(secs) = v.parse::<u64>() {
                cfg.router_timeout = Duration::from_secs(secs);
            }
        }
        cfg.backend_url = std::env::var("DETECTIC_BACKEND_URL").ok();
        cfg.backend_token = std::env::var("DETECTIC_BACKEND_TOKEN").ok();
        if let Ok(v) = std::env::var("DETECTIC_BACKEND_TIMEOUT") {
            if let Ok(secs) = v.parse::<u64>() {
                cfg.backend_timeout = Duration::from_secs(secs);
            }
        }
        // Legacy env var compatibility (DETECTIC_UPLOAD_URL → backend_url).
        // Treat an empty DETECTIC_BACKEND_URL the same as unset so the
        // legacy alias is honored (an empty value used to silently disable
        // backend uploads).
        if cfg.backend_url.as_deref().unwrap_or("").is_empty() {
            cfg.backend_url = std::env::var("DETECTIC_UPLOAD_URL").ok();
        }
        if let Ok(v) = std::env::var("DETECTIC_BUFFER") {
            cfg.spool_path = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("DETECTIC_BUFFER_MAX") {
            if let Ok(n) = v.parse::<u64>() {
                cfg.spool_max_bytes = n;
            }
        }
        if let Ok(v) = std::env::var("DETECTIC_SITE_SURVEY") {
            cfg.enable_site_survey = v == "1" || v.eq_ignore_ascii_case("true");
        }
        if let Ok(v) = std::env::var("DETECTIC_RADIO_STATS") {
            cfg.enable_radio_stats = v == "1" || v.eq_ignore_ascii_case("true");
        }
        if let Ok(v) = std::env::var("DETECTIC_PROXIMITY_HISTORY_WINDOW") {
            if let Ok(n) = v.parse::<usize>() {
                if n > 0 {
                    cfg.presence.proximity.history_window = n;
                }
            }
        }
        if let Ok(v) = std::env::var("DETECTIC_PROXIMITY_EMA_ALPHA") {
            if let Ok(f) = v.parse::<f64>() {
                if (0.0..=1.0).contains(&f) {
                    cfg.presence.proximity.ema_alpha = f;
                }
            }
        }
        if let Ok(v) = std::env::var("DETECTIC_PROXIMITY_TREND_DELTA") {
            if let Ok(f) = v.parse::<f64>() {
                if f >= 0.0 {
                    cfg.presence.proximity.trend_delta_db = f;
                }
            }
        }
        if let Ok(v) = std::env::var("DETECTIC_PROXIMITY_TREND_MIN_SAMPLES") {
            if let Ok(n) = v.parse::<usize>() {
                if n > 0 {
                    cfg.presence.proximity.trend_min_samples = n;
                }
            }
        }
        if let Ok(v) = std::env::var("DETECTIC_LOG_LEVEL") {
            cfg.log_level = LogLevel::from_str(&v);
        }
        if let Ok(v) = std::env::var("DETECTIC_LOG_MACS") {
            cfg.log_macs = v == "1" || v.eq_ignore_ascii_case("true");
        }
        if let Ok(v) = std::env::var("DETECTIC_HTTP_PORT") {
            if let Ok(p) = v.parse::<u16>() {
                if p > 0 {
                    cfg.http_port = p;
                }
            }
        }
        if let Ok(v) = std::env::var("DETECTIC_HTTP_SERVER") {
            cfg.enable_http_server = v == "1" || v.eq_ignore_ascii_case("true");
        }
        if let Ok(v) = std::env::var("DETECTIC_MDNS") {
            cfg.enable_mdns = v == "1" || v.eq_ignore_ascii_case("true");
        }
        if let Ok(v) = std::env::var("DETECTIC_MDNS_HOSTNAME") {
            if !v.is_empty() {
                cfg.mdns_hostname = v;
            }
        }
        if let Ok(v) = std::env::var("DETECTIC_ARP") {
            cfg.enable_arp_fastpath = v == "1" || v.eq_ignore_ascii_case("true");
        }
        if let Ok(v) = std::env::var("DETECTIC_ARP_INTERVAL") {
            if let Ok(secs) = v.parse::<u64>() {
                if secs > 0 {
                    cfg.arp_interval = Duration::from_secs(secs);
                }
            }
        }

        cfg
    }

    /// Load a minimal key=value config file (one per line, `key=value`).
    /// Env vars take precedence over file values.
    pub fn from_file(path: &std::path::Path) -> Self {
        let mut cfg = Self::default();
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, val)) = line.split_once('=') {
                    let key = key.trim();
                    let val = val.trim();
                    match key {
                        "router_url" => cfg.router_url = val.into(),
                        "router_user" => cfg.router_user = val.into(),
                        "router_password" => cfg.router_password = val.into(),
                        "secret" => cfg.secret = val.into(),
                        "sensor_id" => cfg.sensor_id = val.into(),
                        "interval" => {
                            if let Ok(secs) = val.parse::<u64>() {
                                if secs > 0 {
                                    cfg.interval = Duration::from_secs(secs);
                                }
                            }
                        }
                        "backend_url" => cfg.backend_url = Some(val.into()),
                        "backend_token" => cfg.backend_token = Some(val.into()),
                        "spool_path" => cfg.spool_path = PathBuf::from(val),
                        "spool_max_bytes" => {
                            if let Ok(n) = val.parse::<u64>() {
                                cfg.spool_max_bytes = n;
                            }
                        }
                        "enable_site_survey" => {
                            cfg.enable_site_survey = val == "1" || val.eq_ignore_ascii_case("true");
                        }
                        "enable_radio_stats" => {
                            cfg.enable_radio_stats = val == "1" || val.eq_ignore_ascii_case("true");
                        }
                        "log_level" => cfg.log_level = LogLevel::from_str(val),
                        "log_macs" => {
                            cfg.log_macs = val == "1" || val.eq_ignore_ascii_case("true");
                        }
                        "http_port" => {
                            if let Ok(p) = val.parse::<u16>() {
                                cfg.http_port = p;
                            }
                        }
                        "enable_http_server" => {
                            cfg.enable_http_server = val == "1" || val.eq_ignore_ascii_case("true");
                        }
                        "enable_mdns" => {
                            cfg.enable_mdns = val == "1" || val.eq_ignore_ascii_case("true");
                        }
                        "mdns_hostname" => {
                            if !val.is_empty() {
                                cfg.mdns_hostname = val.into();
                            }
                        }
                        "enable_arp_fastpath" => {
                            cfg.enable_arp_fastpath =
                                val == "1" || val.eq_ignore_ascii_case("true");
                        }
                        "arp_interval" => {
                            if let Ok(secs) = val.parse::<u64>() {
                                cfg.arp_interval = Duration::from_secs(secs);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        cfg
    }

    /// Validate that required fields are present.
    pub fn validate(&self) -> Result<(), String> {
        if self.router_password.is_empty() {
            return Err("router password is required (set DETECTIC_PASSWORD)".into());
        }
        if self.secret.is_empty() {
            return Err("per-sensor secret is required (set DETECTIC_SECRET)".into());
        }
        if self.interval.as_secs() == 0 {
            return Err("interval must be > 0".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let cfg = SensorConfig::default();
        assert_eq!(cfg.router_url, "http://192.168.0.1");
        assert_eq!(cfg.interval, Duration::from_secs(30));
        assert_eq!(cfg.max_stations, 256);
        assert!(!cfg.log_macs);
    }

    #[test]
    fn env_overrides_defaults() {
        std::env::set_var("DETECTIC_URL", "http://10.0.0.1");
        std::env::set_var("DETECTIC_INTERVAL", "60");
        std::env::set_var("DETECTIC_PASSWORD", "pw");
        std::env::set_var("DETECTIC_SECRET", "sk");
        let cfg = SensorConfig::from_env();
        assert_eq!(cfg.router_url, "http://10.0.0.1");
        assert_eq!(cfg.interval, Duration::from_secs(60));
        std::env::remove_var("DETECTIC_URL");
        std::env::remove_var("DETECTIC_INTERVAL");
        std::env::remove_var("DETECTIC_PASSWORD");
        std::env::remove_var("DETECTIC_SECRET");
    }

    #[test]
    fn validate_rejects_missing_credentials() {
        let cfg = SensorConfig::default();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_accepts_complete_config() {
        let cfg = SensorConfig {
            router_password: "pw".into(),
            secret: "sk".into(),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn log_level_parses() {
        assert_eq!(LogLevel::from_str("error"), LogLevel::Error);
        assert_eq!(LogLevel::from_str("WARN"), LogLevel::Warn);
        assert_eq!(LogLevel::from_str("info"), LogLevel::Info);
        assert_eq!(LogLevel::from_str("debug"), LogLevel::Debug);
        assert_eq!(LogLevel::from_str("garbage"), LogLevel::Info);
    }

    #[test]
    fn config_file_parses() {
        // Isolate from parallel tests that may set environment variables.
        std::env::remove_var("DETECTIC_URL");
        std::env::remove_var("DETECTIC_INTERVAL");
        let dir = std::env::temp_dir();
        let path = dir.join("detectic_test.cfg");
        std::fs::write(
            &path,
            "# comment\nrouter_url=http://172.16.0.1\nsensor_id=test-002\ninterval=45\n",
        )
        .unwrap();
        let cfg = SensorConfig::from_file(&path);
        assert_eq!(cfg.router_url, "http://172.16.0.1");
        assert_eq!(cfg.sensor_id, "test-002");
        assert_eq!(cfg.interval, Duration::from_secs(45));
        let _ = std::fs::remove_file(&path);
    }
}
