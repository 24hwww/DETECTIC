//! Permanent runtime service (M10-C).
//!
//! `DetecticService` wraps the polling loop with:
//! - bounded watchdog restarts (max 3 attempts, exponential backoff)
//! - graceful shutdown on SIGTERM/SIGINT
//! - spool recovery at startup
//! - optional update checks between polls
//!
//! Resource targets:
//! - ≤ 2 threads
//! - < 5 MB RSS
//! - idle CPU between polls

use crate::backend::{BackendTransport, NullBackend};
use crate::calibrate::Band;
use crate::config::SensorConfig;
use crate::crypto;
use crate::event_transport::{HttpEventTransport, ReliableQueue, SpoolEventTransport};
#[cfg(feature = "wss")]
use crate::wss_transport::WssEventTransport;
use crate::logging;
use crate::monitor::{MediaTekMonitorProvider, MonitorProvider, NullMonitorProvider};
use crate::presence::{PresenceEngine, PresenceObservation};
use crate::runtime::install_signal_handlers;
use crate::runtime::should_shutdown;
use crate::snapshot::{diff_snapshots, SensorSnapshot};
use crate::temporal::{DeviceObs, NetworkObs, TemporalConfig, TemporalEngine};
use crate::transport::{Dialect, GtprClient};
use std::thread::sleep;
use std::time::{Duration, Instant};

#[cfg(feature = "persist")]
use crate::notifier::{
    DetectionEvent, Notifier, SmtpConfig, SmtpNotifier, RustlsSmtpTransport,
};

/// Watchdog configuration.
pub const MAX_RESTART_ATTEMPTS: u32 = 3;
pub const BACKOFF_INITIAL: Duration = Duration::from_secs(5);
pub const BACKOFF_MAX: Duration = Duration::from_secs(60);

/// Health snapshot for the `detectic health` command.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthSnapshot {
    pub version: &'static str,
    pub architecture: &'static str,
    pub uptime_secs: u64,
    pub rss_kb: u64,
    pub thread_count: u32,
    pub poll_interval_secs: u64,
    pub backend: String,
    pub spool_size_bytes: u64,
    pub sensor_id: String,
    pub last_poll: Option<u64>,
    pub last_upload: Option<u64>,
    pub monitor_provider: String,
    pub gtpr_status: String,
}

impl HealthSnapshot {
    /// Build a health snapshot from the current process state.
    pub fn now(cfg: &SensorConfig, monitor_name: &str, gtpr_status: &str) -> Self {
        let rss_kb = read_rss_kb();
        let thread_count = 1; // single-threaded sensor
        let spool_size_bytes = if cfg.spool_path.exists() {
            std::fs::metadata(&cfg.spool_path)
                .map(|m| m.len())
                .unwrap_or(0)
        } else {
            0
        };
        Self {
            version: env!("CARGO_PKG_VERSION"),
            architecture: std::env::consts::ARCH,
            uptime_secs: read_uptime_secs(),
            rss_kb,
            thread_count,
            poll_interval_secs: cfg.interval.as_secs(),
            backend: cfg.backend_url.clone().unwrap_or_else(|| "none".into()),
            spool_size_bytes,
            sensor_id: cfg.sensor_id.clone(),
            last_poll: None,
            last_upload: None,
            monitor_provider: monitor_name.to_string(),
            gtpr_status: gtpr_status.to_string(),
        }
    }
}

fn read_rss_kb() -> u64 {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                if let Some(kb) = rest.trim().split_whitespace().next() {
                    if let Ok(n) = kb.parse::<u64>() {
                        return n;
                    }
                }
            }
        }
    }
    0
}

fn read_uptime_secs() -> u64 {
    // Process uptime via /proc/self/stat starttime vs /proc/uptime.
    if let (Ok(uptime_str), Ok(stat)) = (
        std::fs::read_to_string("/proc/uptime"),
        std::fs::read_to_string("/proc/self/stat"),
    ) {
        let uptime: f64 = uptime_str
            .split_whitespace()
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        // starttime is field 22 (1-indexed) in /proc/self/stat
        let fields: Vec<&str> = stat.split_whitespace().collect();
        if fields.len() >= 22 {
            let clk_tck = 100u64;
            if let Ok(starttime) = fields[21].parse::<u64>() {
                let start_secs = starttime as f64 / clk_tck as f64;
                return ((uptime - start_secs) as u64).max(0);
            }
        }
    }
    0
}

/// Permanent Detectic service.
pub struct DetecticService {
    pub config: SensorConfig,
    pub dialect: Dialect,
    pub restart_attempts: u32,
    pub poll_count: u64,
    pub last_poll: Option<Instant>,
    pub last_upload: Option<Instant>,
    pub last_snapshot: Option<SensorSnapshot>,
    pub presence: PresenceEngine,
    pub temporal: TemporalEngine,
    pub event_queue: ReliableQueue,
    #[cfg(feature = "persist")]
    pub notifier: Option<SmtpNotifier>,
}

impl DetecticService {
    pub fn new(config: SensorConfig) -> Self {
        let presence_cfg = config.presence.clone();
        let temporal = TemporalEngine::new(&config.sensor_id, TemporalConfig::default());
        let event_queue = ReliableQueue::new(4096, 4096);
        Self {
            config,
            dialect: Dialect::GdprJson,
            restart_attempts: 0,
            poll_count: 0,
            last_poll: None,
            last_upload: None,
            last_snapshot: None,
            presence: PresenceEngine::new(presence_cfg),
            temporal,
            event_queue,
            #[cfg(feature = "persist")]
            notifier: None,
        }
    }

    /// Build and attach an SMTP notifier from environment variables.
    /// Returns Ok(true) if a notifier was configured, Ok(false) if disabled.
    #[cfg(feature = "persist")]
    pub fn attach_notifier(&mut self) -> Result<bool, String> {
        let smtp_config = SmtpConfig::from_env().map_err(|e| format!("smtp config: {e}"))?;
        if !smtp_config.enabled {
            logging::info("smtp_disabled");
            return Ok(false);
        }
        let transport = Box::new(RustlsSmtpTransport::new(&smtp_config).map_err(|e| format!("smtp transport: {e}"))?);
        let queue_path = std::env::var("DETECTIC_SMTP_QUEUE")
            .unwrap_or_else(|_| "/var/run/misc/misc_rw/detectic/state/smtp_queue.db".into());
        let notifier = SmtpNotifier::new(smtp_config, &queue_path, transport)
            .map_err(|e| format!("smtp notifier: {e}"))?;
        self.notifier = Some(notifier);
        logging::info("smtp_notifier_attached");
        Ok(true)
    }

    /// Convert a detection Event to a DetectionEvent for the notifier.
    #[cfg(feature = "persist")]
    fn event_to_detection(
        event: &crate::events::Event,
        snapshot: &SensorSnapshot,
        presence: &PresenceEngine,
    ) -> DetectionEvent {
        // Look up the device in the snapshot for raw data
        let device = snapshot
            .stations
            .iter()
            .find(|d| d.identity() == event.identity);

        // Look up presence observation for proximity/classification
        let obs = presence.lookup(&event.identity);

        let hostname = device
            .and_then(|d| d.hostname.clone())
            .or_else(|| obs.as_ref().and_then(|o| o.hostname.clone()));
        let ip = device
            .and_then(|d| d.ip.clone())
            .or_else(|| obs.as_ref().and_then(|o| o.ip.clone()));
        let mac = device
            .and_then(|d| d.mac.clone())
            .or_else(|| obs.as_ref().and_then(|o| o.mac.clone()));
        let rssi_dbm: Option<i32> = device
            .and_then(|d| d.rssi)
            .map(|r| r as i32)
            .or_else(|| obs.as_ref().and_then(|o| o.rssi_smoothed.map(|r| r as i32)));
        let band = device.and_then(|d| d.standard.clone());
        let channel = None;
        let source = device.and_then(|d| d.source.clone());

        // Presence / proximity from engine
        let connected = matches!(event.kind, crate::events::EventKind::DeviceJoined | crate::events::EventKind::DeviceUpdated);
        let active = obs
            .as_ref()
            .map(|o| o.presence == crate::presence::PresenceState::Present)
            .unwrap_or(connected);
        let proximity = obs
            .as_ref()
            .map(|o| match o.proximity {
                crate::presence::Proximity::VeryNear => "Muito perto",
                crate::presence::Proximity::Near => "Perto",
                crate::presence::Proximity::Medium => "Distancia media",
                crate::presence::Proximity::Far => "Longe",
                crate::presence::Proximity::Unknown => "Incerto",
            })
            .unwrap_or("Incerto");
        let signal_quality = match rssi_dbm {
            Some(r) if r >= -50 => "Excelente",
            Some(r) if r >= -60 => "Bom",
            Some(r) if r >= -70 => "Razoavel",
            Some(r) if r >= -80 => "Fraco",
            _ => "N/A",
        };

        // Aggregate counts from the snapshot
        let total_devices = snapshot.stations.len() as u32;
        let connected_count = snapshot
            .stations
            .iter()
            .filter(|d| d.source.as_deref() == Some("wifi"))
            .count() as u32;
        let not_connected_count = total_devices.saturating_sub(connected_count);

        DetectionEvent {
            captured_at: event.captured_at,
            kind: event.kind.clone(),
            pseudonym: event.pseudonym.clone(),
            changed_fields: event.changed_fields.clone(),
            hostname,
            ip,
            mac,
            rssi_dbm,
            rcpi: None,
            band,
            channel,
            source,
            distance_m: None,
            connected,
            active,
            proximity: proximity.into(),
            signal_quality: signal_quality.into(),
            total_devices,
            connected_count,
            not_connected_count,
        }
    }

    /// Send events to the SMTP notifier (if attached).
    #[cfg(feature = "persist")]
    fn notify_events(
        &self,
        events: &[crate::events::Event],
        snapshot: &SensorSnapshot,
        presence: &PresenceEngine,
    ) {
        if let Some(ref notifier) = self.notifier {
            for event in events {
                let detection = Self::event_to_detection(event, snapshot, presence);
                if let Err(e) = notifier.send(&detection) {
                    logging::warn(&format!("smtp_send_error err={}", e));
                }
            }
            // Flush queued emails (non-blocking best-effort)
            match notifier.flush() {
                Ok(n) if n > 0 => logging::info(&format!("smtp_flushed count={}", n)),
                Err(e) => logging::warn(&format!("smtp_flush_error err={}", e)),
                _ => {}
            }
        }
    }

    /// Run a single poll cycle and exit.
    pub fn run_once(&mut self) {
        install_signal_handlers();
        logging::set_level(self.config.log_level);
        logging::info(&format!(
            "service_once sensor={} interval={}s",
            self.config.sensor_id,
            self.config.interval.as_secs()
        ));

        // Attach SMTP notifier if configured
        #[cfg(feature = "persist")]
        if let Err(e) = self.attach_notifier() {
            logging::warn(&format!("smtp_attach_failed err={}", e));
        }

        let mut backend = self.build_backend();
        let secret = self.config.secret.as_bytes().to_vec();
        let mut monitor: Box<dyn MonitorProvider> = if self.config.enable_site_survey {
            Box::new(MediaTekMonitorProvider::new())
        } else {
            Box::new(NullMonitorProvider)
        };
        logging::info(&format!("monitor_provider={}", monitor.name()));

        match self.poll_once(&mut *backend, &secret, &mut *monitor) {
            Ok(()) => logging::info("service_once_ok"),
            Err(e) => logging::error(&format!("service_once_error err={}", e)),
        }

        // Final flush of any queued emails
        #[cfg(feature = "persist")]
        if let Some(ref notifier) = self.notifier {
            match notifier.flush() {
                Ok(n) if n > 0 => logging::info(&format!("smtp_final_flush count={}", n)),
                Err(e) => logging::warn(&format!("smtp_final_flush_error err={}", e)),
                _ => {}
            }
        }

        logging::info("service_stopped");
    }

    /// Run the service loop with watchdog semantics.
    pub fn run(&mut self) {
        install_signal_handlers();
        logging::set_level(self.config.log_level);
        logging::info(&format!(
            "service_started sensor={} interval={}s",
            self.config.sensor_id,
            self.config.interval.as_secs()
        ));

        // Attach SMTP notifier if configured
        #[cfg(feature = "persist")]
        if let Err(e) = self.attach_notifier() {
            logging::warn(&format!("smtp_attach_failed err={}", e));
        }

        let mut backend = self.build_backend();
        let secret = self.config.secret.as_bytes().to_vec();
        let mut monitor: Box<dyn MonitorProvider> = if self.config.enable_site_survey {
            Box::new(MediaTekMonitorProvider::new())
        } else {
            Box::new(NullMonitorProvider)
        };
        logging::info(&format!("monitor_provider={}", monitor.name()));

        while !should_shutdown() {
            match self.poll_once(&mut *backend, &secret, &mut *monitor) {
                Ok(()) => {
                    self.restart_attempts = 0;
                }
                Err(e) => {
                    self.restart_attempts += 1;
                    logging::error(&format!(
                        "poll_error attempt={} err={}",
                        self.restart_attempts, e
                    ));
                    if self.restart_attempts >= MAX_RESTART_ATTEMPTS {
                        logging::error("watchdog_failed entering_failed_state");
                        // In a real deployment, an external launcher would
                        // restart us. Here we exit to avoid a crash loop.
                        return;
                    }
                    let backoff = backoff_for(self.restart_attempts);
                    logging::warn(&format!("backoff_sleep secs={}", backoff.as_secs()));
                    sleep_with_shutdown(backoff);
                }
            }
            // Flush any queued SMTP retries even when no new events
            #[cfg(feature = "persist")]
            if let Some(ref notifier) = self.notifier {
                match notifier.flush() {
                    Ok(n) if n > 0 => logging::info(&format!("smtp_retry_flushed count={}", n)),
                    Err(e) => logging::warn(&format!("smtp_retry_flush_error err={}", e)),
                    _ => {}
                }
            }
            // Sleep until next poll, interruptible by shutdown
            sleep_with_shutdown(self.config.interval);
        }
        logging::info("service_stopped");
    }

    /// Perform one poll cycle.
    pub fn poll_once(
        &mut self,
        backend: &mut dyn BackendTransport,
        secret: &[u8],
        monitor: &mut dyn MonitorProvider,
    ) -> Result<(), String> {
        // Drain spool first
        backend.drain_spool();

        // Collect snapshot
        let snapshot = self.collect_snapshot()?;
        self.poll_count += 1;
        self.last_poll = Some(Instant::now());

        // Update presence engine
        let _obs: Vec<PresenceObservation> =
            self.presence.update(&snapshot.stations, snapshot.timestamp);

        // Build canonical temporal event envelopes from the snapshot.
        let mut device_obs = Vec::with_capacity(snapshot.stations.len());
        for d in &snapshot.stations {
            let identity = d.identity();
            let pseudo =
                crypto::pseudonymize(secret, d.mac.as_deref().unwrap_or(&identity));
            let band = d
                .radio_mac
                .as_deref()
                .map(Band::from_radio_mac)
                .and_then(|b| match b {
                    Band::Ghz2_4 => Some("2.4GHz".to_string()),
                    Band::Ghz5 => Some("5GHz".to_string()),
                    _ => d.standard.clone(),
                });
            let noise = d.noise.map(|n| n as i64);
            device_obs.push(DeviceObs {
                identity: identity.clone(),
                pseudonym: pseudo,
                rssi: d.rssi,
                noise,
                band,
                interface: d.interface.clone().or(d.radio_mac.clone()),
            });
        }
        let canonical = self
            .temporal
            .process_associated(snapshot.timestamp, &device_obs);
        self.event_queue.submit(canonical);

        // Collect nearby observations if monitor is available
        let nearby = monitor.scan();
        if !nearby.is_empty() {
            logging::info(&format!("nearby_observations count={}", nearby.len()));
            let mut network_obs = Vec::with_capacity(nearby.len());
            for n in &nearby {
                let pseudo = crypto::pseudonymize(secret, &n.bssid);
                network_obs.push(NetworkObs {
                    bssid_pseudonym: pseudo,
                    band: if n.band.is_empty() {
                        None
                    } else {
                        Some(n.band.clone())
                    },
                    channel: if n.channel == 0 {
                        None
                    } else {
                        Some(n.channel as u8)
                    },
                    signal: n.rssi,
                    ssid: if n.ssid.is_empty() {
                        None
                    } else {
                        Some(n.ssid.clone())
                    },
                    security: n.security.clone(),
                    w_mode: n.w_mode.clone(),
                    extch: n.extch.clone(),
                });
            }
            let net_events = self
                .temporal
                .process_networks(snapshot.timestamp, &network_obs);
            self.event_queue.submit(net_events);

            if let Some(env) = self.temporal.rf_environment_snapshot(snapshot.timestamp) {
                self.event_queue.submit(vec![env]);
            }
        }

        // Flush canonical events to the backend if configured.
        // SpoolEventTransport persists undelivered events to disk and drains
        // any previous spool before attempting the current batch.
        if let Some(url) = self.config.backend_url.as_deref() {
            let inner: Box<dyn crate::event_transport::EventTransport> = if url.starts_with("wss://") || url.starts_with("ws://") {
                #[cfg(feature = "wss")]
                { Box::new(WssEventTransport::new(url, &self.config.sensor_id)) }
                #[cfg(not(feature = "wss"))]
                { Box::new(HttpEventTransport::new(url, &self.config.sensor_id, secret, Duration::from_secs(30))) }
            } else {
                Box::new(HttpEventTransport::new(url, &self.config.sensor_id, secret, Duration::from_secs(30)))
            };
            // Use a separate events spool so the legacy snapshot spool is not
            // corrupted by the new canonical event format.
            let events_spool = self
                .config
                .spool_path
                .with_file_name("detectic_events.jsonl");
            let mut transport =
                SpoolEventTransport::new(inner, events_spool, 65536);
            let drained = transport.drain();
            if drained > 0 {
                logging::info(&format!("events_spool_drained count={}", drained));
            }
            let report = self.event_queue.flush(&mut transport);
            if report.sent > 0 {
                self.last_upload = Some(Instant::now());
            }
            logging::info(&format!(
                "events_flush sent={} kept={} dropped={} spool_drained={}",
                report.sent,
                report.kept,
                self.event_queue.dropped_total(),
                drained
            ));
        }

        // Change detection
        let events = if let Some(ref prev) = self.last_snapshot {
            let diff = diff_snapshots(prev, &snapshot);
            let map_diff = crate::model::MapDiff {
                added: diff.joined.clone(),
                removed: diff.left.clone(),
                changed: diff.updated.clone(),
            };
            crate::events::diff_to_events(&map_diff, snapshot.timestamp, |id| {
                crate::pseudonymize(secret, id)
            })
        } else {
            let map_diff = crate::model::MapDiff {
                added: snapshot.stations.clone(),
                removed: Vec::new(),
                changed: Vec::new(),
            };
            crate::events::diff_to_events(&map_diff, snapshot.timestamp, |id| {
                crate::pseudonymize(secret, id)
            })
        };

        logging::info(&format!(
            "poll_success stations={} events={}",
            snapshot.stations.len(),
            events.len()
        ));

        // Send notifications for device events
        #[cfg(feature = "persist")]
        self.notify_events(&events, &snapshot, &self.presence);

        // Send to backend
        let sent = backend.send_snapshot(&snapshot, &events, secret);
        if sent {
            self.last_upload = Some(Instant::now());
        } else {
            logging::warn("backend_unavailable spooled");
        }

        self.last_snapshot = Some(snapshot);
        Ok(())
    }

    fn collect_snapshot(&self) -> Result<SensorSnapshot, String> {
        let url = self.config.router_url.clone();
        let user = self.config.router_user.clone();
        let password = self.config.router_password.clone();
        let max_stations = self.config.max_stations;
        let mut transport = GtprClient::with_dialect(&url, &user, &password, self.dialect);
        transport.connect().map_err(|e| e.to_string())?;
        let map = crate::collector::collect(&transport).map_err(|e| e.to_string())?;
        Ok(SensorSnapshot::from_map(&map, max_stations))
    }

    fn build_backend(&self) -> Box<dyn BackendTransport> {
        if let Some(url) = &self.config.backend_url {
            if !url.is_empty() && !url.starts_with("wss://") && !url.starts_with("ws://") {
                return Box::new(crate::backend::HttpBackend::new(&self.config));
            }
        }
        Box::new(NullBackend::new())
    }

    /// Return a health snapshot for the `detectic health` command.
    pub fn health(&self, gtpr_status: &str) -> HealthSnapshot {
        let mut h = HealthSnapshot::now(&self.config, "mediatek_iwpriv_site_survey", gtpr_status);
        if let Some(t) = self.last_poll {
            h.last_poll = Some(t.elapsed().as_secs());
        }
        if let Some(t) = self.last_upload {
            h.last_upload = Some(t.elapsed().as_secs());
        }
        h
    }
}

fn backoff_for(attempt: u32) -> Duration {
    let secs = 5u64 * (1u64 << (attempt - 1).min(4));
    Duration::from_secs(secs.min(BACKOFF_MAX.as_secs()))
}

fn sleep_with_shutdown(d: Duration) {
    let step = Duration::from_secs(1);
    let mut remaining = d;
    while remaining > Duration::ZERO && !should_shutdown() {
        let s = if remaining < step { remaining } else { step };
        sleep(s);
        remaining -= s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff_for(1), Duration::from_secs(5));
        assert_eq!(backoff_for(2), Duration::from_secs(10));
        assert_eq!(backoff_for(3), Duration::from_secs(20));
        assert_eq!(backoff_for(4), Duration::from_secs(40));
        assert_eq!(backoff_for(5), Duration::from_secs(60)); // capped
        assert_eq!(backoff_for(99), Duration::from_secs(60));
    }

    #[test]
    fn health_snapshot_has_version() {
        let cfg = SensorConfig::default();
        let h = HealthSnapshot::now(&cfg, "test", "ok");
        assert_eq!(h.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(h.thread_count, 1);
    }
}
