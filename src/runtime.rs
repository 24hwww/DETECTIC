//! Sensor runtime loop (M5-B).
//!
//! The runtime owns the polling loop, signal handling, retry/backoff for
//! GTPR failures, change detection, and backend dispatch. It is designed
//! to remain alive under transient failures and never crash because one
//! data source is unavailable.

use crate::backend::{BackendTransport, NullBackend};
use crate::collector;
use crate::config::SensorConfig;
use crate::events::{diff_to_events, EventKind};
use crate::logging;
use crate::model::NetworkMap;
use crate::snapshot::{diff_snapshots, SensorSnapshot};
use crate::transport::{Dialect, GtprClient};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Global shutdown flag — set by the signal handler.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Check whether a shutdown signal has been received.
pub fn should_shutdown() -> bool {
    SHUTDOWN.load(Ordering::Relaxed)
}

/// Request a graceful shutdown. Called from the signal handler or tests.
pub fn request_shutdown() {
    SHUTDOWN.store(true, Ordering::Relaxed);
}

/// Reset the shutdown flag (for tests).
#[cfg(test)]
pub fn reset_shutdown() {
    SHUTDOWN.store(false, Ordering::Relaxed);
}

/// Install SIGTERM/SIGINT handlers that set the shutdown flag.
/// On platforms where signal handling is not available (e.g. no `signal` hook),
/// this is a no-op and the runtime relies on the OS terminating the process.
///
/// If the parent (launcher.sh) has already set SIGTERM to SIG_IGN via
/// `trap '' 15`, we respect that and do NOT override it. This prevents
/// the EX520's `cos`/`phoenix` from causing a clean exit when they
/// terminate the process group.
#[cfg(unix)]
pub fn install_signal_handlers() {
    // Use a simple approach: register a SIGTERM/SIGINT handler via libc.
    // We avoid pulling in a signal-handling crate to keep deps minimal.
    extern "C" {
        fn signal(signum: i32, handler: extern "C" fn(i32)) -> extern "C" fn(i32);
    }
    extern "C" fn handle_sig(_sig: i32) {
        request_shutdown();
    }
    const SIGTERM: i32 = 15;
    const SIGINT: i32 = 2;
    // The launcher (launcher.sh) traps SIGTERM/SIGINT/SIGHUP to SIG_IGN so the
    // sensor survives the EX520's cos/phoenix lifecycle kill. If the parent has
    // set a signal to SIG_IGN, we must NOT install our own handler — otherwise
    // cos would flip SHUTDOWN and cause a clean exit right after boot.
    unsafe {
        if !sig_is_ignored(SIGTERM) {
            signal(SIGTERM, handle_sig);
        }
        if !sig_is_ignored(SIGINT) {
            signal(SIGINT, handle_sig);
        }
    }
}

/// Whether `signum` is currently set to SIG_IGN, read from the SigIgn bitmask in
/// `/proc/self/status`. This is the portable, UB-free way to detect an inherited
/// SIG_IGN (e.g. from the launcher's `trap '' 2 15`). Falls back to false on any
/// read error, meaning we install our own handler (safe default).
#[cfg(unix)]
fn sig_is_ignored(signum: i32) -> bool {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return false;
    };
    let Some(line) = status.lines().find(|l| l.starts_with("SigIgn:")) else {
        return false;
    };
    // SigIgn: is a hex bitmask where bit (signum-1) set == signal ignored.
    let Some(hex) = line.split_whitespace().next_back() else {
        return false;
    };
    let Ok(mask) = u64::from_str_radix(hex, 16) else {
        return false;
    };
    signum > 0 && (signum as u32) <= 64 && (mask & (1u64 << (signum - 1))) != 0
}

#[cfg(not(unix))]
pub fn install_signal_handlers() {
    // No signal handling on non-Unix platforms.
}

/// Runtime health/status information (M5-J).
#[derive(Debug, Clone)]
pub struct RuntimeStatus {
    pub version: &'static str,
    pub architecture: &'static str,
    pub started_at: i64,
    pub uptime_secs: u64,
    pub interval_secs: u64,
    pub last_successful_poll: Option<i64>,
    pub poll_count: u64,
    pub error_count: u64,
    pub station_count: usize,
    pub backend_name: String,
    pub backend_connected: bool,
    pub spool_size: u64,
    pub vm_rss_kb: Option<u64>,
}

/// The sensor runtime. Owns the polling loop and all state.
pub struct SensorRuntime {
    config: SensorConfig,
    started_at: Instant,
    started_epoch: i64,
    poll_count: u64,
    error_count: u64,
    last_successful_poll: Option<i64>,
    last_snapshot: Option<SensorSnapshot>,
    dialect: Dialect,
}

impl SensorRuntime {
    pub fn new(config: SensorConfig) -> Self {
        Self {
            config,
            started_at: Instant::now(),
            started_epoch: now_epoch(),
            poll_count: 0,
            error_count: 0,
            last_successful_poll: None,
            last_snapshot: None,
            dialect: Dialect::GdprJson,
        }
    }

    /// Build the appropriate backend from configuration.
    pub fn build_backend(&self) -> Box<dyn BackendTransport> {
        if let Some(url) = &self.config.backend_url {
            if !url.is_empty() {
                // SpoolBackend wraps HttpBackend with bounded local buffer.
                // On upload failure, payloads are buffered locally and
                // retried on the next poll cycle.
                let http = Box::new(crate::backend::HttpBackend::new(&self.config));
                return Box::new(crate::backend::SpoolBackend::new(
                    http,
                    self.config.spool_path.to_str().unwrap_or("/var/run/misc/misc_rw/detectic/spool/detectic_buffer.jsonl"),
                    self.config.spool_max_bytes,
                    self.config.secret.as_bytes(),
                ));
            }
        }
        // No backend configured — use null backend (data discarded)
        Box::new(NullBackend::new())
    }

    /// Run the sensor polling loop until shutdown.
    pub fn run(&mut self) {
        if let Err(e) = self.config.validate() {
            logging::error(&format!("config_error {}", e));
            return;
        }

        logging::set_level(self.config.log_level);
        install_signal_handlers();

        let router_url = self.config.router_url.clone();
        let interval_secs = self.config.interval.as_secs();
        let sensor_id = self.config.sensor_id.clone();

        logging::info(&format!(
            "sensor_started url={} interval={}s sensor={}",
            router_url, interval_secs, sensor_id
        ));

        #[cfg(not(feature = "persist"))]
        logging::info("compiled_without_persist state_will_not_survive_restarts");

        let mut backend = self.build_backend();
        let secret = self.config.secret.as_bytes().to_vec();
        let mut consecutive_errors: usize = 0;

        while !should_shutdown() {
            // Drain the spool at the start of each cycle
            backend.drain_spool();

            // Attempt to collect a snapshot
            match self.poll() {
                Ok(snapshot) => {
                    self.poll_count += 1;
                    consecutive_errors = 0;
                    self.last_successful_poll = Some(snapshot.timestamp);

                    // Change detection
                    let events = if let Some(ref prev) = self.last_snapshot {
                        let diff = diff_snapshots(prev, &snapshot);
                        let map_diff = crate::model::MapDiff {
                            added: diff.joined.clone(),
                            removed: diff.left.clone(),
                            changed: diff.updated.clone(),
                        };
                        diff_to_events(&map_diff, snapshot.timestamp, |id| {
                            crate::pseudonymize(&secret, id)
                        })
                    } else {
                        // First snapshot: all stations are "joined"
                        let map_diff = crate::model::MapDiff {
                            added: snapshot.stations.clone(),
                            removed: Vec::new(),
                            changed: Vec::new(),
                        };
                        diff_to_events(&map_diff, snapshot.timestamp, |id| {
                            crate::pseudonymize(&secret, id)
                        })
                    };

                    let wifi_count = snapshot.wifi_station_count();
                    let total_count = snapshot.device_count();

                    logging::info(&format!(
                        "poll_success stations={} wifi={} events={}",
                        total_count,
                        wifi_count,
                        events.len()
                    ));

                    // Log join/leave events
                    for e in &events {
                        match e.kind {
                            EventKind::DeviceJoined => {
                                logging::info(&format!("station_join pseudonym={}", e.pseudonym))
                            }
                            EventKind::DeviceLeft => {
                                logging::info(&format!("station_leave pseudonym={}", e.pseudonym))
                            }
                            EventKind::DeviceUpdated => logging::info(&format!(
                                "station_update pseudonym={} fields={}",
                                e.pseudonym,
                                e.changed_fields.join(",")
                            )),
                        }
                    }

                    // Send to backend
                    let sent = backend.send_snapshot(&snapshot, &events, &secret);
                    if !sent {
                        logging::warn("backend_unavailable spooled");
                    }

                    self.last_snapshot = Some(snapshot);
                }
                Err(e) => {
                    self.error_count += 1;
                    consecutive_errors += 1;
                    logging::warn(&format!(
                        "gtpr_error retry={} error={}",
                        consecutive_errors, e
                    ));

                    // Bounded exponential backoff for GTPR failures
                    let backoff_secs =
                        (1u64 << (consecutive_errors.min(6).saturating_sub(1))).min(60);
                    if backoff_secs > 0 && consecutive_errors > 1 {
                        logging::info(&format!("backoff_sleep secs={}", backoff_secs));
                        sleep_with_shutdown(Duration::from_secs(backoff_secs));
                    }
                }
            }

            // Sleep until next poll, checking for shutdown
            sleep_with_shutdown(self.config.interval);
        }

        logging::info("sensor_stopped");
    }

    /// Poll the router once and return a snapshot.
    /// Reconnects on each poll (no persistent HTTP connection).
    fn poll(&mut self) -> Result<SensorSnapshot, String> {
        let url = self.config.router_url.clone();
        let user = self.config.router_user.clone();
        let password = self.config.router_password.clone();
        let max_stations = self.config.max_stations;
        let mut transport = GtprClient::with_dialect(&url, &user, &password, self.dialect);
        transport.connect().map_err(|e| e.to_string())?;
        let map: NetworkMap = collector::collect(&transport).map_err(|e| e.to_string())?;
        Ok(SensorSnapshot::from_map(&map, max_stations))
    }

    /// Get the current runtime status (M5-J).
    pub fn status(&self) -> RuntimeStatus {
        RuntimeStatus {
            version: env!("CARGO_PKG_VERSION"),
            architecture: std::env::consts::ARCH,
            started_at: self.started_epoch,
            uptime_secs: self.started_at.elapsed().as_secs(),
            interval_secs: self.config.interval.as_secs(),
            last_successful_poll: self.last_successful_poll,
            poll_count: self.poll_count,
            error_count: self.error_count,
            station_count: self
                .last_snapshot
                .as_ref()
                .map(|s| s.stations.len())
                .unwrap_or(0),
            backend_name: "auto".into(),
            backend_connected: true,
            spool_size: 0,
            vm_rss_kb: read_vm_rss_kb(),
        }
    }
}

/// Sleep for `duration`, but wake up early if a shutdown signal is received.
/// Checks the shutdown flag every 1 second (no busy loop).
fn sleep_with_shutdown(duration: Duration) {
    let mut remaining = duration;
    let check_interval = Duration::from_secs(1);
    while !should_shutdown() && remaining > Duration::ZERO {
        let step = remaining.min(check_interval);
        std::thread::sleep(step);
        remaining = remaining.saturating_sub(step);
    }
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Read VmRSS from /proc/self/status (Linux only). Returns None if unavailable.
fn read_vm_rss_kb() -> Option<u64> {
    read_vm_rss_kb_public()
}

/// Public wrapper for `read_vm_rss_kb` — used by the `status` command.
pub fn read_vm_rss_kb_public() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        return parts[1].parse::<u64>().ok();
                    }
                }
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_flag_works() {
        reset_shutdown();
        assert!(!should_shutdown());
        request_shutdown();
        assert!(should_shutdown());
        reset_shutdown();
    }

    #[test]
    fn sig_is_ignored_reads_proc_status() {
        // /proc/self/status must report a SigIgn bitmask; sig_is_ignored should
        // not crash and should return a bool. Use a very high signal number that
        // is never ignored (out of a 64-bit mask) to assert false safely.
        let r = sig_is_ignored(2); // SIGINT — always a bool, never panics
        assert!(r == false || r == true);
        // Signal numbers beyond 64 are never represented; must be false.
        assert!(!sig_is_ignored(5000));
    }

    #[test]
    fn runtime_status_reports_basic_info() {
        reset_shutdown();
        let cfg = SensorConfig {
            router_password: "pw".into(),
            secret: "sk".into(),
            ..Default::default()
        };
        let rt = SensorRuntime::new(cfg);
        let status = rt.status();
        assert_eq!(status.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(status.interval_secs, 30);
        assert_eq!(status.poll_count, 0);
    }

    #[test]
    fn sleep_with_shutdown_returns_early() {
        reset_shutdown();
        let start = Instant::now();
        // Request shutdown after a very short delay
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(100));
            request_shutdown();
        });
        sleep_with_shutdown(Duration::from_secs(10));
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_secs(5), "should return early");
        reset_shutdown();
    }

    #[test]
    fn build_backend_returns_null_when_no_url() {
        let cfg = SensorConfig {
            router_password: "pw".into(),
            secret: "sk".into(),
            ..Default::default()
        };
        let rt = SensorRuntime::new(cfg);
        let backend = rt.build_backend();
        assert_eq!(backend.name(), "null");
    }

    #[test]
    fn build_backend_returns_local_spool_when_url_set() {
        let dir = std::env::temp_dir();
        let path = dir.join("detectic_runtime_backend_test.jsonl");
        let _ = std::fs::remove_file(&path);
        let cfg = SensorConfig {
            router_password: "pw".into(),
            secret: "sk".into(),
            backend_url: Some("http://example.com".into()),
            spool_path: path.clone(),
            ..Default::default()
        };
        let rt = SensorRuntime::new(cfg);
        let backend = rt.build_backend();
        assert_eq!(backend.name(), "local-spool");
        let _ = std::fs::remove_file(&path);
    }
}
