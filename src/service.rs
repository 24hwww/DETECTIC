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

use crate::arp::ArpWatcher;
use crate::backend::{BackendTransport, NullBackend};
use crate::calibrate::Band;
use crate::config::SensorConfig;
use crate::crypto;
use crate::event_transport::{HttpEventTransport, ReliableQueue, SpoolEventTransport};
use crate::http_server::{HttpServer, SensorState};
use crate::logging;
use crate::mdns::{guess_local_ipv4, MdnsResponder};
use crate::monitor::{MediaTekMonitorProvider, MonitorProvider, NullMonitorProvider};
use crate::presence::{PresenceEngine, PresenceObservation};
use crate::proximity::{ProximityResult, SignalType};
use crate::runtime::install_signal_handlers;
use crate::runtime::should_shutdown;
use crate::snapshot::{diff_snapshots, SensorSnapshot};
use crate::temporal::{DeviceObs, NetworkObs, TemporalConfig, TemporalEngine};
use crate::transport::{Dialect, GtprClient};
#[cfg(feature = "wss")]
use crate::wss_transport::WssEventTransport;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

#[cfg(feature = "persist")]
use crate::notifier::{DetectionEvent, Notifier, RustlsSmtpTransport, SmtpConfig, SmtpNotifier};

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
    pub last_poll_at: Option<Instant>,
    pub last_event_at: Option<Instant>,
    pub last_upload: Option<Instant>,
    pub last_snapshot: Option<SensorSnapshot>,
    pub presence: PresenceEngine,
    pub temporal: TemporalEngine,
    pub event_queue: ReliableQueue,
    /// Persistent canonical-events transport (SpoolEventTransport wrapping a
    /// WSS/HTTP transport). Cached across poll cycles so the WSS socket stays
    /// alive instead of reconnecting on every flush — this is what makes the
    /// immediate device-event flush near-zero latency (no per-flush handshake).
    pub events_transport: Option<SpoolEventTransport>,
    /// Reused GTPR session across polls. The handshake (RSA + TokenID) is the
    /// dominant per-poll latency; keeping the client alive makes each poll a
    /// single encrypted `gl` call instead of a full reconnect.
    pub gtpr: Option<crate::transport::GtprClient>,
    pub state: Arc<Mutex<SensorState>>,
    pub arp: Option<ArpWatcher>,
    #[cfg(feature = "persist")]
    pub notifier: Option<SmtpNotifier>,
}

impl DetecticService {
    pub fn new(config: SensorConfig) -> Self {
        let presence_cfg = config.presence.clone();
        let temporal = TemporalEngine::new(
            &config.sensor_id,
            TemporalConfig {
                missing_polls_to_disconnect: 1,
                ..Default::default()
            },
        );
        let event_queue = ReliableQueue::new(4096, 4096);
        let mut state = SensorState::new();
        state.sensor_id = config.sensor_id.clone();
        state.version = env!("CARGO_PKG_VERSION").into();
        state.secret = config.secret.clone();
        state.started_at = Some(Instant::now());
        state.healthy = true;
        state.ready = false;
        let arp = if config.enable_arp_fastpath {
            Some(ArpWatcher::new(config.arp_interval))
        } else {
            None
        };
        Self {
            config,
            dialect: Dialect::GdprJson,
            restart_attempts: 0,
            poll_count: 0,
            last_poll: None,
            last_poll_at: None,
            last_event_at: None,
            last_upload: None,
            last_snapshot: None,
            presence: PresenceEngine::new(presence_cfg),
            temporal,
            event_queue,
            events_transport: None,
            gtpr: None,
            state: Arc::new(Mutex::new(state)),
            arp,
            #[cfg(feature = "persist")]
            notifier: None,
        }
    }

    /// Spawn the HTTP control server and mDNS responder, if enabled.
    pub fn start_control_plane(&mut self) {
        if self.config.enable_http_server {
            let state = Arc::clone(&self.state);
            // Retry the HTTP bind a few times — on the EX520V the network
            // stack may not be fully ready when phoenix launches the sensor
            // very early in the boot sequence.
            let mut bound = false;
            for attempt in 1..=5u32 {
                match HttpServer::spawn(Arc::clone(&state), self.config.http_port) {
                    Ok(()) => {
                        logging::info(&format!(
                            "http_server_started port={} attempt={}",
                            self.config.http_port, attempt
                        ));
                        bound = true;
                        break;
                    }
                    Err(e) => {
                        logging::warn(&format!(
                            "http_server_bind_failed attempt={} err={}",
                            attempt, e
                        ));
                        std::thread::sleep(std::time::Duration::from_secs(2));
                    }
                }
            }
            if !bound {
                logging::warn("http_server_disabled_after_retries");
            }
        }

        // mDNS is disabled on-router: the EX520V firmware may not support
        // multicast group join on 127.0.0.1 (which is what guess_local_ipv4
        // returns when DETECTIC_URL=http://127.0.0.1).  The HTTP control
        // plane is sufficient for health monitoring.
        if self.config.enable_mdns {
            let ip = guess_local_ipv4().unwrap_or(std::net::Ipv4Addr::new(192, 168, 0, 1));
            // Skip mDNS if the resolved IP is loopback — it will always fail.
            if ip.is_loopback() {
                logging::info("mdns_skipped_loopback");
            } else {
                let txt = vec![
                    format!("version={}", env!("CARGO_PKG_VERSION")),
                    format!("sensor_id={}", self.config.sensor_id),
                    "service=detectic".into(),
                ];
                if let Err(e) =
                    MdnsResponder::spawn(&self.config.mdns_hostname, ip, self.config.http_port, txt)
                {
                    logging::warn(&format!("mdns_start_failed err={}", e));
                } else {
                    logging::info("mdns_started");
                }
            }
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
        let transport = Box::new(
            RustlsSmtpTransport::new(&smtp_config).map_err(|e| format!("smtp transport: {e}"))?,
        );
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
        let proximity = obs.as_ref().and_then(|o| o.proximity.as_ref()).cloned();
        let rssi_dbm: Option<i32> = proximity
            .as_ref()
            .and_then(|p| p.rssi_dbm.map(|r| r as i32));
        let heat = proximity.as_ref().map(|p| p.heat);
        let band = device.and_then(|d| d.standard.clone());
        let channel = None;
        let source = device.and_then(|d| d.source.clone());

        // Presence / proximity from engine
        let connected = matches!(
            event.kind,
            crate::events::EventKind::DeviceJoined | crate::events::EventKind::DeviceUpdated
        );
        let active = obs
            .as_ref()
            .map(|o| o.presence == crate::presence::PresenceState::Present)
            .unwrap_or(connected);
        let proximity_label = proximity
            .as_ref()
            .map(|p| p.label())
            .unwrap_or_else(|| "Incerto".into());
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
            rcpi: proximity
                .as_ref()
                .and_then(|p| p.raw_signal.map(|r| r as u32)),
            band,
            channel,
            source,
            distance_m: proximity.as_ref().and_then(|p| p.distance_m),
            connected,
            active,
            proximity: proximity_label,
            signal_quality: signal_quality.into(),
            heat,
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
        self.start_control_plane();
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
        let mut monitor: Box<dyn MonitorProvider> =
            if self.config.enable_site_survey || self.config.enable_radio_stats {
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
        self.start_control_plane();
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
        let mut monitor: Box<dyn MonitorProvider> =
            if self.config.enable_site_survey || self.config.enable_radio_stats {
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
                    {
                        let mut state = self.state.lock().unwrap();
                        state.gtpr_status = "error".into();
                        state.healthy = false;
                        state.last_gtpr_failure = Some(Instant::now());
                    }
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

        // Stage clock for latency instrumentation: T2 = snapshot received,
        // T3 = event generated, T4 = transport initiated (in flush_events).
        let t2 = Instant::now();

        // Collect snapshot
        let snapshot = self.collect_snapshot()?;
        self.poll_count += 1;
        self.last_poll = Some(Instant::now());
        logging::info(&format!(
            "T2_SNAPSHOT received_at={}ts stations={}",
            snapshot.timestamp,
            snapshot.stations.len()
        ));
        // Record the snapshot receipt time for T3->T4 measurement.
        self.last_poll_at = Some(t2);

        // Update presence engine and collect proximity per identity.
        let mut snapshot = snapshot;
        let presence_obs: Vec<PresenceObservation> =
            self.presence.update(&snapshot.stations, snapshot.timestamp);
        let mut proximity_by_id: HashMap<String, ProximityResult> = HashMap::new();
        for o in &presence_obs {
            if let Some(p) = o.proximity.clone() {
                proximity_by_id.insert(o.identity.clone(), p);
            }
        }
        snapshot.station_proximity = proximity_by_id.clone();

        // Phase 4: collect per-radio per-chain RSSI (read-only `iwpriv <if> stat`)
        // when enabled, and attach to the snapshot so /metrics exposes the time
        // series. Do NOT change any radio state; this is only a read of existing
        // firmware telemetry.
        if self.config.enable_radio_stats {
            let rs = monitor.radio_stats();
            if !rs.is_empty() {
                for s in &rs {
                    logging::info(&format!(
                        "radio_stats iface={} band={:?} chains={:?}",
                        s.interface, s.band, s.rssi_per_chain
                    ));
                }
                snapshot.radio_stats = rs;
            }
        }

        // Push the device snapshot to the control plane NOW, BEFORE the (slow)
        // site survey below, so /devices and the dashboard reflect the latest
        // proximity immediately instead of waiting for the AP scan to finish.
        {
            let mut state = self.state.lock().unwrap();
            state.last_poll = self.last_poll;
            state.gtpr_status = "ok".into();
            state.device_count = snapshot.stations.len();
            *state.snapshot.lock().unwrap() = Some(snapshot.clone());
            state.healthy = true;
            state.ready = true;
        }

        // Build canonical temporal event envelopes from the snapshot.
        // Only devices with active != "0" are considered "associated".
        // The EX520 keeps inactive devices in the table with active=0;
        // filtering them here makes the temporal engine treat active=0
        // as a disconnection, generating DeviceDisconnected events.
        let mut device_obs = Vec::with_capacity(snapshot.stations.len());
        for d in &snapshot.stations {
            // Skip inactive devices — the EX520 GTPR table retains them
            // with active=0, but they are NOT associated.
            if d.active.as_deref() == Some("0") {
                continue;
            }
            let identity = d.identity();
            let pseudo = crypto::pseudonymize(secret, d.mac.as_deref().unwrap_or(&identity));
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
            let proximity = proximity_by_id.get(&identity).cloned();
            device_obs.push(DeviceObs {
                identity: identity.clone(),
                pseudonym: pseudo,
                rssi: d.rssi,
                noise,
                band,
                interface: d.interface.clone().or(d.radio_mac.clone()),
                hostname: d.hostname.clone(),
                proximity,
            });
        }
        let canonical = self
            .temporal
            .process_associated(snapshot.timestamp, &device_obs);
        let t3 = Instant::now();
        logging::info(&format!(
            "T3_GENERATED events={} t2_to_t3_ms={}",
            canonical.len(),
            t3.duration_since(t2).as_nanos() as u64 / 1_000_000
        ));
        for env in &canonical {
            logging::info(&format!(
                "T3_ENVELOPE event_id={} device_id={} event_type={:?} envelope_ts={}",
                env.event_id,
                env.device_id.as_deref().unwrap_or(""),
                env.event_type,
                env.timestamp
            ));
        }

        // Latency between event generation (T3) and transport initiation (T4)
        // is what the immediate flush removes. Record T3 so flush_events can
        // report the exact T3->T4 gap.
        self.last_event_at = Some(t3);

        // Feed RF probe observations (POST /probes) into the temporal engine so
        // movement-detected (possibly non-associated) devices also produce
        // presence events and reach the backend stream.
        {
            let queued: Vec<crate::temporal::ProbeObservation> = {
                let mut state = self.state.lock().unwrap();
                std::mem::take(&mut state.pending_probes)
            };
            if !queued.is_empty() {
                let probe_events = self.temporal.process_probes(snapshot.timestamp, &queued);
                for env in &probe_events {
                    logging::info(&format!(
                        "T3_ENVELOPE event_id={} device_id={} event_type={:?} envelope_ts={}",
                        env.event_id,
                        env.device_id.as_deref().unwrap_or(""),
                        env.event_type,
                        env.timestamp
                    ));
                }
                self.event_queue.submit(probe_events);
            }
        }

        self.event_queue.submit(canonical);

        // IMMEDIATE flush of latency-sensitive device events. This is the T3->T4
        // fix: do NOT wait for the (slow, blocking) site survey below to finish
        // before transmitting an already-created DeviceConnected/DeviceDisconnected
        // event. Batching is still preserved for the non-urgent AP/RF events that
        // are flushed again after the survey.
        self.flush_events(secret);

        // Collect nearby observations if monitor is available
        let nearby = monitor.scan();
        if !nearby.is_empty() {
            logging::info(&format!("nearby_observations count={}", nearby.len()));
            let mut network_obs = Vec::with_capacity(nearby.len());
            for n in &nearby {
                let pseudo = crypto::pseudonymize(secret, &n.bssid);
                let band = if n.band.is_empty() {
                    None
                } else {
                    Some(n.band.clone())
                };
                let band_enum = band
                    .as_deref()
                    .map(Band::from_radio_mac)
                    .unwrap_or(Band::Ghz2_4);
                let proximity = self.presence.compute_proximity(
                    &n.bssid,
                    n.rssi,
                    SignalType::Dbm,
                    band_enum,
                    snapshot.timestamp,
                );
                network_obs.push(NetworkObs {
                    bssid_pseudonym: pseudo,
                    band,
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
                    proximity: Some(proximity),
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

        // Flush any (non-latency-critical) AP/RF events added by the site survey
        // above. These are batched; the latency-sensitive device events were
        // already flushed immediately after submit(canonical).
        self.flush_events(secret);

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
        for e in &events {
            logging::info(&format!(
                "T2_DETECTED pseudonym={} kind={:?} captured_at={}",
                e.pseudonym, e.kind, e.captured_at
            ));
        }

        // ARP fast-path: read once per poll for every device in the snapshot.
        // This only accelerates already-known devices; it does not authoritatively
        // claim Wi-Fi association and never creates a new device from ARP alone.
        if let Some(ref mut arp) = self.arp {
            let _entries = arp.read();
            // Future: merge arp_last_seen into presence hints.
        }

        // Update shared sensor state for the HTTP/mDNS control plane.
        {
            let mut state = self.state.lock().unwrap();
            state.last_poll = self.last_poll;
            state.last_upload = self.last_upload;
            state.gtpr_status = "ok".into();
            state.device_count = snapshot.stations.len();
            *state.snapshot.lock().unwrap() = Some(snapshot.clone());
            state.healthy = true;
            state.ready = true;
            if !events.is_empty() {
                let summary = format!(
                    "{{\"ts\":{},\"type\":\"poll_events\",\"count\":{}}}",
                    snapshot.timestamp,
                    events.len()
                );
                let mut recent = state.recent_events.lock().unwrap();
                recent.push(summary);
                while recent.len() > 64 {
                    recent.remove(0);
                }
            }
        }

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

    /// Convert a WebSocket backend URL (ws:// or wss://) into the equivalent
    /// HTTPS snapshot ingest endpoint. The path /ws is stripped and replaced
    /// with /api/v1/events so the periodic snapshot upload reaches the HTTP
    /// ingest handler while canonical events continue to flow over WSS.
    fn derive_http_backend_url(wss_url: &str) -> Option<String> {
        let stripped = if let Some(rest) = wss_url.strip_prefix("wss://") {
            format!("https://{}", rest)
        } else if let Some(rest) = wss_url.strip_prefix("ws://") {
            format!("http://{}", rest)
        } else {
            return None;
        };
        let mut url = stripped.trim_end_matches('/').to_string();
        if url.ends_with("/ws") {
            url.truncate(url.len() - 3);
        }
        if !url.ends_with("/api/v1/events") {
            url.push_str("/api/v1/events");
        }
        Some(url)
    }

    fn collect_snapshot(&mut self) -> Result<SensorSnapshot, String> {
        let url = self.config.router_url.clone();
        let user = self.config.router_user.clone();
        let password = self.config.router_password.clone();
        let max_stations = self.config.max_stations;

        // Reuse the GTPR session across polls: the handshake (RSA params + login
        // + TokenID) is the dominant per-poll latency. Only establish it when
        // there is no live client (first poll, or after a failure drops it).
        if self.gtpr.is_none() {
            let mut transport = GtprClient::with_dialect(&url, &user, &password, self.dialect);
            transport.connect().map_err(|e| e.to_string())?;
            self.gtpr = Some(transport);
        }
        let client = self.gtpr.as_ref().expect("gtpr client initialized");
        match crate::collector::collect(client) {
            Ok(map) => Ok(SensorSnapshot::from_map(&map, max_stations)),
            Err(e) => {
                // Session likely stale (the EX520 may expire JSESSIONID or the
                // socket dropped). Drop it so the next poll does a fresh connect.
                self.gtpr = None;
                Err(e.to_string())
            }
        }
    }

    fn build_backend(&self) -> Box<dyn BackendTransport> {
        if let Some(url) = &self.config.backend_url {
            if !url.is_empty() && !url.starts_with("wss://") && !url.starts_with("ws://") {
                return Box::new(crate::backend::HttpBackend::new(&self.config));
            }
            // WebSocket event transport is handled separately by flush_events().
            // For the periodic snapshot upload, derive an HTTPS endpoint so the
            // full snapshot + diff events still reach the worker's HTTP ingest.
            if !url.is_empty() && (url.starts_with("wss://") || url.starts_with("ws://")) {
                if let Some(http_url) = Self::derive_http_backend_url(url) {
                    let mut cfg = self.config.clone();
                    cfg.backend_url = Some(http_url);
                    return Box::new(crate::backend::HttpBackend::new(&cfg));
                }
            }
        }
        Box::new(NullBackend::new())
    }

    /// Flush pending canonical events (ReliableQueue) to the configured backend
    /// via the events spool transport. This is what actually transmits the
    /// events; previously it only ran once at the END of poll_once — AFTER the
    /// (slow, blocking) site survey — which delayed an already-created event by
    /// the full scan time (~1s on the EX520). Now callers can flush immediately
    /// after submitting latency-sensitive device events.
    fn flush_events(&mut self, secret: &[u8]) {
        let Some(url) = self.config.backend_url.clone() else {
            return;
        };
        // Memoize the (spool-wrapped) transport so the underlying WSS socket
        // stays alive across poll cycles (reconnect only on IDLE_TIMEOUT/errors).
        // This is essential: a fresh WssEventTransport per flush would pay a full
        // TLS + hello/ack handshake every poll, adding tens of ms to T3->T4.
        if self.events_transport.is_none() {
            let inner: Box<dyn crate::event_transport::EventTransport> =
                if url.starts_with("wss://") || url.starts_with("ws://") {
                    #[cfg(feature = "wss")]
                    {
                        Box::new(WssEventTransport::new(
                            &url,
                            &self.config.sensor_id,
                            &self.config.secret,
                        ))
                    }
                    #[cfg(not(feature = "wss"))]
                    {
                        Box::new(HttpEventTransport::new(
                            &url,
                            &self.config.sensor_id,
                            secret,
                            Duration::from_secs(30),
                        ))
                    }
                } else {
                    Box::new(HttpEventTransport::new(
                        &url,
                        &self.config.sensor_id,
                        secret,
                        Duration::from_secs(30),
                    ))
                };
            let events_spool = self
                .config
                .spool_path
                .with_file_name("detectic_events.jsonl");
            self.events_transport = Some(SpoolEventTransport::new(inner, events_spool, 65536));
        }
        let mut transport = self.events_transport.take().unwrap();
        let drained = transport.drain();
        if drained > 0 {
            logging::info(&format!("events_spool_drained count={}", drained));
        }
        let t4 = Instant::now();
        // Clear stage origin before flushing so the sent-count math below uses
        // THIS cycle's timestamps only (the T3 origin is reset each poll).
        let t2_origin = self.last_poll_at;
        let t3_origin = self.last_event_at;
        self.last_event_at = None;
        let report = self.event_queue.flush(&mut transport);
        // Stash the transport back for reuse next cycle.
        self.events_transport = Some(transport);
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
        if report.sent > 0 {
            // T4->T5: local transmission/send+ack window captured here.
            logging::info(&format!(
                "T4_T5 flush_ms={}",
                t4.elapsed().as_nanos() as u64 / 1_000_000
            ));
            if let Some(t2) = t2_origin {
                logging::info(&format!(
                    "T2_TO_T4_ms={}",
                    t4.duration_since(t2).as_nanos() as u64 / 1_000_000
                ));
            }
            if let Some(t3) = t3_origin {
                logging::info(&format!(
                    "T3_TO_T4_ms={}",
                    t4.duration_since(t3).as_nanos() as u64 / 1_000_000
                ));
            }
        }
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
    use crate::event_transport::EventTransport;
    use crate::temporal::{EventEnvelope, EventType};

    #[test]
    fn derive_http_backend_url_converts_wss_workers_dev() {
        assert_eq!(
            DetecticService::derive_http_backend_url("wss://detectic.24hwww.workers.dev/ws"),
            Some("https://detectic.24hwww.workers.dev/api/v1/events".into())
        );
    }

    #[test]
    fn derive_http_backend_url_converts_wss_root() {
        assert_eq!(
            DetecticService::derive_http_backend_url("wss://detectic.24hwww.workers.dev"),
            Some("https://detectic.24hwww.workers.dev/api/v1/events".into())
        );
    }

    #[test]
    fn derive_http_backend_url_converts_ws_trailing_slash() {
        assert_eq!(
            DetecticService::derive_http_backend_url("ws://example.com/ws/"),
            Some("http://example.com/api/v1/events".into())
        );
    }

    #[test]
    fn derive_http_backend_url_returns_none_for_http() {
        assert_eq!(DetecticService::derive_http_backend_url("https://example.com/api/v1/events"), None);
    }

    // A cooperative fake transport that "acks" every event it receives, and
    // records how many batches were sent (to prove delivery semantics).
    #[derive(Default)]
    struct FakeTransport {
        sent_batches: Vec<usize>,
        connected: bool,
    }

    impl EventTransport for FakeTransport {
        fn send_events(&mut self, events: &[EventEnvelope]) -> Vec<String> {
            self.sent_batches.push(events.len());
            self.connected = true;
            events.iter().map(|e| e.event_id.clone()).collect()
        }
        fn name(&self) -> &str {
            "fake"
        }
        fn is_connected(&self) -> bool {
            self.connected
        }
    }

    fn make_event(id: &str, seq: u64) -> EventEnvelope {
        EventEnvelope {
            event_id: id.to_string(),
            sequence: seq,
            sensor_id: "ex520-test".into(),
            timestamp: 1_700_000_000,
            event_type: EventType::DeviceConnected,
            device_id: None,
            payload: serde_json::json!({}),
        }
    }

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

    #[test]
    fn immediate_flush_sends_without_next_poll() {
        // Verify the T3->T4 fix: submitting an event then flushing in the SAME
        // poll cycle transmits it immediately (no dependence on the next cycle).
        let mut q = ReliableQueue::new(4096, 4096);
        q.submit(vec![make_event("e1", 1), make_event("e2", 2)]);
        assert_eq!(q.pending_len(), 2);

        let mut t = FakeTransport::default();
        let report = q.flush(&mut t);
        assert_eq!(report.sent, 2);
        assert_eq!(report.kept, 0);
        assert_eq!(q.pending_len(), 0);
        assert_eq!(t.sent_batches, vec![2]);
    }

    #[test]
    fn memoized_transport_reused_across_flushes() {
        // flush_events must memoize the transport so a fresh WSS socket is not
        // recreated per flush (which would add a per-poll handshake to T3->T4).
        let mut cfg = SensorConfig::default();
        cfg.backend_url = Some("http://example.invalid/events".into());
        let mut svc = DetecticService::new(cfg);
        // Inject a fake transport directly into the memo slot.
        let fake = Box::new(FakeTransport {
            sent_batches: Vec::new(),
            connected: false,
        });
        let spool_path = svc
            .config
            .spool_path
            .with_file_name("detectic_events.jsonl");
        // First "creation" — detic's created counter is intentionally not the
        // memoization signal; we simply assert the slot is retained after a flush.
        svc.events_transport = Some(SpoolEventTransport::new(fake, spool_path, 65536));
        svc.event_queue.submit(vec![make_event("a", 1)]);
        svc.flush_events(b"secret");
        assert!(
            svc.events_transport.is_some(),
            "transport stashed back for reuse"
        );
        assert_eq!(svc.event_queue.pending_len(), 0, "event acked and removed");
    }
}
