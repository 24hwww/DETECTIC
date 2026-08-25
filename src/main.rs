//! Detectic CLI — layered architecture (M5):
//!   Transport (GDPR) → Collector (OIDs→NetworkMap) → Snapshot → Backend
//!
//! The sensor remains a single static binary; the layering is compile-time.

use clap::{Parser, Subcommand, ValueEnum};
#[cfg(feature = "persist")]
use detectic::publisher::{
    append_bounded, drain_buffer, upload_with_retry, UploadPayload, UPLOAD_TIMEOUT,
};
use detectic::transport::{Dialect, GtprClient};
#[cfg(feature = "persist")]
use detectic::Store;
#[cfg(feature = "persist")]
use std::thread::sleep;
#[cfg(feature = "persist")]
use std::time::Duration;

// Re-export for `super::*` in tests (backward compat).
#[allow(unused_imports)]
use detectic::publisher::{backoff_delay, parse_buffer_line};

#[derive(Parser)]
#[command(name = "detectic", about = "TP-Link EX520 network-map sensor")]
struct Cli {
    #[arg(long, env = "DETECTIC_URL", default_value = "http://192.168.0.1")]
    url: String,
    #[arg(long, env = "DETECTIC_USER", default_value = "user")]
    user: String,
    /// Router password (required for map/sensor/query commands).
    #[arg(long, env = "DETECTIC_PASSWORD")]
    password: Option<String>,
    #[arg(long, value_enum, env = "DETECTIC_DIALECT", default_value_t = DialectArg::Json)]
    dialect: DialectArg,
    #[arg(long, default_value = "detectic.db")]
    db: String,
    /// Per-sensor HMAC secret (required for sensor/upload commands).
    #[arg(long, env = "DETECTIC_SECRET")]
    secret: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum DialectArg {
    Json,
    Text,
}

impl From<DialectArg> for Dialect {
    fn from(d: DialectArg) -> Dialect {
        match d {
            DialectArg::Json => Dialect::GdprJson,
            DialectArg::Text => Dialect::GdprText,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    #[cfg(feature = "persist")]
    Capture,
    Map,
    /// Compute and print presence/proximity observations (M6).
    Presence,
    #[cfg(feature = "persist")]
    Stats,
    #[cfg(feature = "persist")]
    Report,
    /// Print per-device presence analytics (Phase F).
    #[cfg(feature = "persist")]
    Analytics,
    /// Run the sensor loop. Use --once for a single poll.
    Sensor {
        /// Poll once and exit (M9).
        #[arg(long)]
        once: bool,
    },
    /// Print sensor health/status (M5-J).
    Status,
    /// Print version and build information.
    Version,
    /// Quick health check: configuration, binary integrity, router reachability.
    Health,
    /// Print effective configuration (secrets redacted).
    Config,
    /// Show current offline spool size and last lines.
    Spool,
    /// Check for available updates (M7).
    Update {
        /// Only check, do not download or install.
        #[arg(long)]
        check: bool,
    },
    /// Roll back to the previous release (M7).
    Rollback,
    /// Remove Detectic from the router and show cleanup steps.
    Uninstall,
    /// Query a single OID via `go` (get-single) and print the raw JSON.
    Query {
        /// OID to retrieve (e.g. DEV2_TELNET_CFG, DEV2_USER_CFG).
        oid: String,
    },
    /// Set fields on an OID via `so` (set-object).
    Set {
        /// OID to set (e.g. DEV2_TELNET_CFG).
        oid: String,
        /// JSON object for the data fields (e.g. '{"telnetLocalEnabled":1}').
        data: String,
    },
    /// Send a CGI request via `cgi` operation (e.g. /cgi/auth, /cgi/setPwd).
    Cgi {
        /// CGI OID (e.g. /cgi/auth).
        oid: String,
        /// JSON object for the data fields.
        data: String,
    },
    /// Trigger an action via `op` operation (e.g. ACT_REBOOT).
    Op {
        /// Action OID (e.g. ACT_REBOOT).
        oid: String,
    },
    /// Print driver capability matrix (M11-A): which driver backends are
    /// usable on the stock EX520V without firmware modification.
    Driver,
    /// Run the realtime unified event pipeline (M11-C) for one cycle and print
    /// the events. Default dev/client mode: reads from the GTPR source only.
    Realtime {
        /// Poll once and exit (default: single cycle).
        #[arg(long)]
        once: bool,
    },
    /// Launcher operations (M11-E). Default mode is stock-manual (no router
    /// persistence). Sub-actions: install | remove | status.
    Launcher {
        #[command(subcommand)]
        action: LauncherAction,
    },
}

#[derive(Subcommand)]
enum LauncherAction {
    /// Attempt to install persistence (safe no-op / refusal on stock firmware).
    Install,
    /// Remove any installed persistence (safe no-op on stock firmware).
    Remove,
    /// Print the current launch probe / status.
    Status,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    // Resolve password/secret into owned Strings for commands that need them.
    // Commands that don't (status, help) can ignore these.
    let password = cli.password.clone().unwrap_or_default();
    let secret_str = cli.secret.clone().unwrap_or_default();
    let secret: &[u8] = secret_str.as_bytes();

    match cli.command {
        Command::Map => {
            if password.is_empty() {
                return Err("router password is required (set DETECTIC_PASSWORD)".into());
            }
            // Transport → Collector
            let mut transport =
                GtprClient::with_dialect(&cli.url, &cli.user, &password, cli.dialect.into());
            transport.connect().map_err(|e| e.to_string())?;
            let map = detectic::collector::collect(&transport).map_err(|e| e.to_string())?;
            println!("{}", serde_json::to_string_pretty(&map)?);
        }
        #[cfg(feature = "persist")]
        Command::Capture => {
            if password.is_empty() {
                return Err("router password is required (set DETECTIC_PASSWORD)".into());
            }
            let mut transport =
                GtprClient::with_dialect(&cli.url, &cli.user, &password, cli.dialect.into());
            transport.connect().map_err(|e| e.to_string())?;
            let map = detectic::collector::collect(&transport).map_err(|e| e.to_string())?;
            let mut store = Store::open(&cli.db, secret)?;
            let diff = store.diff_with_previous(&map)?;
            let snap_id = store.save(&map)?;
            println!("snapshot {} | devices: {}", snap_id, map.devices.len());
            if !diff.added.is_empty() {
                println!("+ added ({}):", diff.added.len());
                for d in &diff.added {
                    println!(
                        "    {} {} {:?}",
                        d.identity(),
                        d.ip.as_deref().unwrap_or(""),
                        d.hostname.as_deref().unwrap_or("")
                    );
                }
            }
            if !diff.removed.is_empty() {
                println!("- removed ({}):", diff.removed.len());
                for d in &diff.removed {
                    println!("    {}", d.identity());
                }
            }
            if !diff.changed.is_empty() {
                println!("~ changed ({}):", diff.changed.len());
                for (b, a) in &diff.changed {
                    println!(
                        "    {} rssi {} -> {}",
                        a.identity(),
                        b.rssi.unwrap_or(-1),
                        a.rssi.unwrap_or(-1)
                    );
                }
            }
            if diff.added.is_empty() && diff.removed.is_empty() && diff.changed.is_empty() {
                println!("no changes");
            }
        }
        #[cfg(feature = "persist")]
        Command::Stats => {
            let store = Store::open(&cli.db, secret)?;
            println!("snapshots stored:        {}", store.snapshot_count()?);
            println!("distinct devices:        {}", store.distinct_devices()?);
        }
        #[cfg(feature = "persist")]
        Command::Report => {
            let store = Store::open(&cli.db, secret)?;
            let rows = store.device_aggregates()?;
            if rows.is_empty() {
                println!("no data yet");
                return Ok(());
            }
            println!(
                "{:<12} {:>10} {:>10} {:>6} {:>8} {:>7} {:>7} {:>7}  src",
                "pseudonym", "first", "last", "obs", "avg_rssi", "min", "max", "src"
            );
            for d in rows {
                println!(
                    "{:<12} {:>10} {:>10} {:>6} {:>8} {:>7} {:>7} {:>7}",
                    &d.pseudonym[..d.pseudonym.len().min(12)],
                    d.first_seen,
                    d.last_seen,
                    d.observations,
                    d.avg_rssi.map(|v| v.to_string()).unwrap_or("-".into()),
                    d.min_rssi.map(|v| v.to_string()).unwrap_or("-".into()),
                    d.max_rssi.map(|v| v.to_string()).unwrap_or("-".into()),
                    d.source.as_deref().unwrap_or("-"),
                );
            }
        }
        #[cfg(feature = "persist")]
        Command::Analytics => {
            let store = Store::open(&cli.db, secret)?;
            let rows = store.device_aggregates()?;
            if rows.is_empty() {
                println!("no data yet");
                return Ok(());
            }
            // Window = span of stored data in days, at least 1.
            let t_min = rows.iter().map(|r| r.first_seen).min().unwrap_or(0);
            let t_max = rows.iter().map(|r| r.last_seen).max().unwrap_or(0);
            let window_days = ((t_max - t_min) / 86400).max(1) as u64;
            let presence = detectic::analytics::presence_from_store_rows(&rows, Some(window_days));
            println!(
                "{:<12} {:>10} {:>10} {:>8} {:>6} {:>4} {:>6} {:>8} {:>5} src",
                "pseudonym",
                "first",
                "last",
                "dur(s)",
                "obs",
                "days",
                "recur",
                "avg_rssi",
                "min/max"
            );
            for p in &presence {
                println!(
                    "{:<12} {:>10} {:>10} {:>8} {:>6} {:>4} {:>6.2} {:>8} {:>3}/{:<3} {}",
                    &p.pseudonym[..p.pseudonym.len().min(12)],
                    p.first_seen,
                    p.last_seen,
                    p.visit_duration_secs,
                    p.observations,
                    p.distinct_days,
                    p.recurrence_score,
                    p.avg_rssi.map(|v| v.to_string()).unwrap_or("-".into()),
                    p.min_rssi.map(|v| v.to_string()).unwrap_or("-".into()),
                    p.max_rssi.map(|v| v.to_string()).unwrap_or("-".into()),
                    p.source.as_deref().unwrap_or("-"),
                );
                // Hour histogram (only non-zero buckets)
                let hline: Vec<String> = p
                    .hour_histogram
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| **c > 0)
                    .map(|(h, c)| format!("{:02}h:{}", h, c))
                    .collect();
                if !hline.is_empty() {
                    println!("             hours: {}", hline.join(" "));
                }
            }
        }
        Command::Sensor { once } => {
            if password.is_empty() {
                return Err("router password is required (set DETECTIC_PASSWORD)".into());
            }
            if secret.is_empty() {
                return Err("per-sensor secret is required (set DETECTIC_SECRET)".into());
            }

            #[cfg(not(feature = "persist"))]
            {
                let cfg = detectic::config::SensorConfig::from_env();
                let mut svc = detectic::service::DetecticService::new(cfg);
                if once {
                    svc.run_once();
                } else {
                    svc.run();
                }
                return Ok(());
            }

            #[cfg(feature = "persist")]
            {
                let url = std::env::var("DETECTIC_URL").unwrap_or_else(|_| cli.url.clone());
                let user = std::env::var("DETECTIC_USER").unwrap_or_else(|_| cli.user.clone());
                let password =
                    std::env::var("DETECTIC_PASSWORD").unwrap_or_else(|_| password.clone());
                let dialect = match std::env::var("DETECTIC_DIALECT").as_deref() {
                    Ok("text") => Dialect::GdprText,
                    _ => cli.dialect.into(),
                };
                let interval: u64 = std::env::var("DETECTIC_INTERVAL")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(30);
                let upload = std::env::var("DETECTIC_UPLOAD_URL").ok();
                let sensor_id =
                    std::env::var("DETECTIC_SENSOR_ID").unwrap_or_else(|_| "home-001".into());
                let buffer = std::env::var("DETECTIC_BUFFER")
                    .unwrap_or_else(|_| "/var/run/misc/misc_rw/detectic/spool/detectic_buffer.jsonl".into());
                let buf_max: u64 = std::env::var("DETECTIC_BUFFER_MAX")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(65_536);
                let db_path = std::env::var("DETECTIC_DB").unwrap_or_else(|_| cli.db.clone());

                let upload_agent = ureq::AgentBuilder::new().timeout(UPLOAD_TIMEOUT).build();
                let mut store = Store::open(&db_path, secret)?;

                println!(
                    "[detectic] sensor started url={} dialect={:?} interval={}s upload=?{:?} buffer={} sensor={}",
                    url, dialect, interval, upload, buffer, sensor_id
                );

                loop {
                    if let Some(u) = &upload {
                        drain_buffer(&buffer, |body: &[u8]| {
                            upload_with_retry(&upload_agent, u.as_str(), &sensor_id, secret, body)
                        });
                    }
                    // Transport → Collector → Diff/Events → Store → Publisher
                    let mut transport = GtprClient::with_dialect(&url, &user, &password, dialect);
                    let map = match transport
                        .connect()
                        .and_then(|_| detectic::collector::collect(&transport))
                    {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("[detectic] map error: {}", e);
                            if once {
                                return Ok(());
                            }
                            sleep(Duration::from_secs(interval));
                            continue;
                        }
                    };

                    let diff = store.diff_with_previous(&map)?;
                    let events = detectic::events::diff_to_events(&diff, map.captured_at, |id| {
                        detectic::pseudonymize(secret, id)
                    });
                    let snap_id = store.save(&map)?;
                    if !events.is_empty() {
                        println!(
                            "[detectic] snapshot {} events: joined={} left={} updated={}",
                            snap_id,
                            events
                                .iter()
                                .filter(|e| e.kind == detectic::events::EventKind::DeviceJoined)
                                .count(),
                            events
                                .iter()
                                .filter(|e| e.kind == detectic::events::EventKind::DeviceLeft)
                                .count(),
                            events
                                .iter()
                                .filter(|e| e.kind == detectic::events::EventKind::DeviceUpdated)
                                .count(),
                        );
                    }
                    let payload =
                        UploadPayload::from_map_with_events(&map, &events, &sensor_id, secret);
                    let line = match serde_json::to_string(&payload) {
                        Ok(s) => s,
                        Err(_) => {
                            if once {
                                return Ok(());
                            }
                            sleep(Duration::from_secs(interval));
                            continue;
                        }
                    };
                    let sent = match &upload {
                        Some(u) => {
                            let body = payload.to_json_bytes();
                            upload_with_retry(&upload_agent, u.as_str(), &sensor_id, secret, &body)
                        }
                        None => false,
                    };
                    if !sent {
                        append_bounded(&buffer, &line, buf_max);
                    }

                    if once {
                        return Ok(());
                    }
                    sleep(Duration::from_secs(interval));
                }
            }
        }
        Command::Status => {
            // M5-J: Print sensor health/status.
            // This is a static status (no running sensor to query).
            // For a running sensor, use `detectic sensor` and check logs.
            let cfg = detectic::config::SensorConfig::from_env();
            println!("detectic {}", env!("CARGO_PKG_VERSION"));
            println!("architecture:    {}", std::env::consts::ARCH);
            println!("router_url:      {}", cfg.router_url);
            println!("interval:        {}s", cfg.interval.as_secs());
            println!("sensor_id:       {}", cfg.sensor_id);
            println!("backend_url:     {:?}", cfg.backend_url);
            println!("spool_path:      {:?}", cfg.spool_path);
            println!("spool_max:       {} bytes", cfg.spool_max_bytes);
            println!("site_survey:     {}", cfg.enable_site_survey);
            println!("radio_stats:     {}", cfg.enable_radio_stats);
            println!("log_level:       {:?}", cfg.log_level);
            println!("max_stations:    {}", cfg.max_stations);
            println!("max_nearby_aps:  {}", cfg.max_nearby_aps);
            println!("router_timeout:  {:?}", cfg.router_timeout);
            println!("backend_timeout: {:?}", cfg.backend_timeout);
            if let Some(rss) = detectic::runtime::read_vm_rss_kb_public() {
                println!("vm_rss:          {} kB", rss);
            }
            // Show spool file size if it exists
            if cfg.spool_path.exists() {
                let size = std::fs::metadata(&cfg.spool_path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                println!("spool_size:      {} bytes", size);
            } else {
                println!("spool_size:      0 bytes (no spool file)");
            }
        }
        Command::Presence => {
            if password.is_empty() {
                return Err("router password is required (set DETECTIC_PASSWORD)".into());
            }
            let mut transport =
                GtprClient::with_dialect(&cli.url, &cli.user, &password, cli.dialect.into());
            transport.connect().map_err(|e| e.to_string())?;
            let map = detectic::collector::collect(&transport).map_err(|e| e.to_string())?;
            let mut engine = detectic::presence::PresenceEngine::new(
                detectic::presence::PresenceConfig::default(),
            );
            let obs = engine.update(&map.devices, map.captured_at);
            println!("{}", serde_json::to_string_pretty(&obs)?);
        }
        Command::Version => {
            println!("detectic {}", env!("CARGO_PKG_VERSION"));
            println!("architecture: {}", std::env::consts::ARCH);
            let profile = if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            };
            println!("build profile: {}", profile);
        }
        Command::Health => {
            let cfg = detectic::config::SensorConfig::from_env();
            let probe = detectic::persistence::probe_launch_mode();
            let gtpr_status = if password.is_empty() {
                "credentials_missing".to_string()
            } else {
                match ureq::get(&cfg.router_url)
                    .timeout(std::time::Duration::from_secs(5))
                    .call()
                {
                    Ok(_) => "reachable".to_string(),
                    Err(_) => "unreachable".to_string(),
                }
            };
            let h = detectic::service::HealthSnapshot::now(
                &cfg,
                if cfg.enable_site_survey {
                    "mediatek_iwpriv_site_survey"
                } else {
                    "disabled"
                },
                &gtpr_status,
            );
            // Print human-readable by default; JSON if --json was passed (not
            // available here, so we print both fields plainly).
            println!("detectic {}", h.version);
            println!("architecture: {}", h.architecture);
            println!("uptime_secs: {}", h.uptime_secs);
            println!("rss_kb: {}", h.rss_kb);
            println!("thread_count: {}", h.thread_count);
            println!("poll_interval_secs: {}", h.poll_interval_secs);
            println!("backend: {}", h.backend);
            println!("spool_size_bytes: {}", h.spool_size_bytes);
            println!("sensor_id: {}", h.sensor_id);
            println!("monitor_provider: {}", h.monitor_provider);
            println!("gtpr_status: {}", h.gtpr_status);
            println!("auto_start_supported: {}", probe.auto_start_supported);
            println!("launch_mode: {:?}", probe.mode);
            if std::env::var("DETECTIC_HEALTH_JSON").is_ok() {
                println!("{}", serde_json::to_string_pretty(&h).unwrap_or_default());
            }
        }
        Command::Config => {
            let cfg = detectic::config::SensorConfig::from_env();
            println!("router_url: {}", cfg.router_url);
            println!("router_user: {}", cfg.router_user);
            println!("router_password: ***");
            println!("router_timeout: {:?}", cfg.router_timeout);
            println!("sensor_id: {}", cfg.sensor_id);
            println!("secret: ***");
            println!("interval: {:?}", cfg.interval);
            println!("backend_url: {:?}", cfg.backend_url);
            println!(
                "backend_token: {:?}",
                cfg.backend_token.as_ref().map(|_| "***")
            );
            println!("backend_timeout: {:?}", cfg.backend_timeout);
            println!("spool_path: {:?}", cfg.spool_path);
            println!("spool_max_bytes: {}", cfg.spool_max_bytes);
            println!("enable_site_survey: {}", cfg.enable_site_survey);
            println!("enable_radio_stats: {}", cfg.enable_radio_stats);
            println!("log_level: {:?}", cfg.log_level);
            println!("log_macs: {}", cfg.log_macs);
            println!("max_stations: {}", cfg.max_stations);
            println!("max_nearby_aps: {}", cfg.max_nearby_aps);
            println!("max_poll_retries: {}", cfg.max_poll_retries);
            println!("max_upload_retries: {}", cfg.max_upload_retries);
            println!(
                "missing_polls_before_leave: {}",
                cfg.presence.missing_polls_before_leave
            );
            println!(
                "rssi_smoothing_alpha: {}",
                cfg.presence.rssi_smoothing_alpha
            );
        }
        Command::Spool => {
            let cfg = detectic::config::SensorConfig::from_env();
            if cfg.spool_path.exists() {
                let size = std::fs::metadata(&cfg.spool_path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                println!("spool_path: {:?}", cfg.spool_path);
                println!("spool_size: {} bytes", size);
                if size > 0 {
                    if let Ok(content) = std::fs::read_to_string(&cfg.spool_path) {
                        let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
                        let tail = lines.iter().rev().take(3).rev().collect::<Vec<_>>();
                        println!("last_entries:");
                        for l in tail {
                            println!("  {}", l.chars().take(160).collect::<String>());
                        }
                    }
                }
            } else {
                println!("spool_path: {:?}", cfg.spool_path);
                println!("spool_size: 0 bytes (no spool file)");
            }
        }
        Command::Update { check } => {
            if check {
                println!("[detectic] update check");
                println!("current_version: {}", env!("CARGO_PKG_VERSION"));
                println!(
                    "update_manifest_url: {:?}",
                    std::env::var("DETECTIC_UPDATE_MANIFEST_URL").ok()
                );
                println!("update_channel: stable");
                println!("status: manual update only; use deploy/detectic-update.sh for a safe, verified update");
            } else {
                println!("[detectic] update");
                println!("Run the deployment update script with a verified release:");
                println!("  /var/run/misc/misc_rw/detectic/current/detectic-update.sh");
                println!("This will download, verify SHA256, stage, and atomically activate the new binary.");
            }
        }
        Command::Rollback => {
            println!("[detectic] rollback");
            println!("Run the deployment rollback script:");
            println!("  /var/run/misc/misc_rw/detectic/current/detectic-rollback.sh");
            println!("This will revert to the previous verified release.");
        }
        Command::Uninstall => {
            println!("[detectic] uninstall");
            println!("Run the deployment removal script:");
            println!("  /var/run/misc/misc_rw/detectic/current/detectic-remove.sh");
            println!("This will stop the sensor, remove the installation directory,");
            println!("and restore the router to its pre-Detectic state.");
            println!("Then disable Telnet/Lifemote if they were enabled for deployment:");
            println!("  detectic set DEV2_TELNET_CFG '{{\"telnetLocalEnabled\":\"0\",\"stack\":\"0,0,0,0,0,0\",\"pstack\":\"0,0,0,0,0,0\"}}'");
            println!("  detectic set DEV2_LIFEMOTE_AGENT '{{\"enable\":\"0\",\"URL\":\"\",\"stack\":\"0,0,0,0,0,0\",\"pstack\":\"0,0,0,0,0,0\"}}'");
        }
        Command::Query { oid } => {
            if password.is_empty() {
                return Err("router password is required (set DETECTIC_PASSWORD)".into());
            }
            let mut transport =
                GtprClient::with_dialect(&cli.url, &cli.user, &password, cli.dialect.into());
            transport.connect().map_err(|e| e.to_string())?;
            let result = transport.go(&oid).map_err(|e| e.to_string())?;
            println!("{}", result);
        }
        Command::Set { oid, data } => {
            if password.is_empty() {
                return Err("router password is required (set DETECTIC_PASSWORD)".into());
            }
            let mut transport =
                GtprClient::with_dialect(&cli.url, &cli.user, &password, cli.dialect.into());
            transport.connect().map_err(|e| e.to_string())?;
            let result = transport.so(&oid, &data).map_err(|e| e.to_string())?;
            println!("{}", result);
        }
        Command::Cgi { oid, data } => {
            if password.is_empty() {
                return Err("router password is required (set DETECTIC_PASSWORD)".into());
            }
            let mut transport =
                GtprClient::with_dialect(&cli.url, &cli.user, &password, cli.dialect.into());
            transport.connect().map_err(|e| e.to_string())?;
            let result = transport.cgi(&oid, &data).map_err(|e| e.to_string())?;
            println!("{}", result);
        }
        Command::Op { oid } => {
            if password.is_empty() {
                return Err("router password is required (set DETECTIC_PASSWORD)".into());
            }
            let mut transport =
                GtprClient::with_dialect(&cli.url, &cli.user, &password, cli.dialect.into());
            transport.connect().map_err(|e| e.to_string())?;
            let result = transport.op(&oid).map_err(|e| e.to_string())?;
            println!("{}", result);
        }
    Command::Driver => {
        let provider = detectic::driver::select_best();
        let matrix = detectic::driver::capability_matrix(provider.as_ref());
        println!("{}", matrix);
    }
        Command::Realtime { once: _once } => {
            use detectic::realtime::{ObservationBatch, RealtimePipeline};
            // Build a one-shot batch from the GTPR source if reachable, else
            // an empty batch (dev/client mode never fabricates observations).
            let mut batch = ObservationBatch {
                map: Default::default(),
                nearby: vec![],
                probes: vec![],
            };
            if !password.is_empty() {
                let mut transport =
                    GtprClient::with_dialect(&cli.url, &cli.user, &password, cli.dialect.into());
                if transport.connect().is_ok() {
                    if let Ok(map) = detectic::collector::collect(&transport) {
                        batch.map = map;
                    }
                }
            }
            let mut pipeline = RealtimePipeline::new();
            let events = pipeline.ingest(&batch, |id| detectic::pseudonymize(secret, id));
            for e in &events {
                println!(
                    "seq={} kind={} identity={} rssi={:?} source={}",
                    e.seq, e.kind.as_str(), e.identity, e.rssi, e.source
                );
            }
            if events.is_empty() {
                println!("no events (no associated stations observed)");
            }
        }
        Command::Launcher { action } => {
            use detectic::launcher::DetecticLauncher;
            let launcher = DetecticLauncher::default();
            let result = match action {
                LauncherAction::Install => launcher.install(),
                LauncherAction::Remove => launcher.remove(),
                LauncherAction::Status => launcher.status(),
            };
            println!("{}", result);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    use detectic::model::{Device, NetworkMap};
    use std::time::Duration;

    // Re-use publisher helpers — thin wrappers so existing tests keep passing.
    fn backoff_delay(attempt: usize) -> Duration {
        detectic::publisher::backoff_delay(attempt)
    }
    fn parse_buffer_line(line: &str) -> &str {
        detectic::publisher::parse_buffer_line(line)
    }
    fn append_bounded(path: &str, line: &str, max: u64) {
        detectic::publisher::append_bounded(path, line, max)
    }
    fn drain_buffer<F: Fn(&[u8]) -> bool>(path: &str, uploader: F) {
        detectic::publisher::drain_buffer(path, uploader)
    }
    type UploadPayload = detectic::publisher::UploadPayload;

    #[test]
    fn backoff_grows_then_caps() {
        assert_eq!(backoff_delay(0), Duration::ZERO);
        assert_eq!(backoff_delay(1), Duration::from_secs(1));
        assert_eq!(backoff_delay(2), Duration::from_secs(2));
        assert_eq!(backoff_delay(3), Duration::from_secs(4));
        assert_eq!(backoff_delay(4), Duration::from_secs(8));
        assert_eq!(backoff_delay(7), Duration::from_secs(8));
    }

    #[test]
    fn parse_buffer_line_strips_timestamp() {
        assert_eq!(parse_buffer_line("1700000000 {\"a\":1}"), "{\"a\":1}");
        assert_eq!(parse_buffer_line("nojson"), "");
    }

    fn sample_device(mac: &str, rssi: Option<i64>) -> Device {
        Device {
            hostname: Some("h".into()),
            ip: Some("10.0.0.1".into()),
            mac: Some(mac.into()),
            rssi,
            standard: Some("ax".into()),
            onemesh_stack: None,
            assoc_time: None,
            radio_mac: None,
            source: Some("wifi".into()),
            tx_rate: None,
            rx_rate: None,
            noise: None,
            signal_level: None,
            max_link_rate: None,
            interface: None,
            ipv6: None,
            client_type: None,
            active: None,
        }
    }

    #[test]
    fn upload_payload_is_stable_and_identifiably_pseudonymized() {
        let m = NetworkMap {
            captured_at: 100,
            devices: vec![sample_device("AA:BB:CC:11:22:33", Some(50))],
            raw: Default::default(),
        };
        let p1 = UploadPayload::from_map(&m, "home-001", b"secret");
        let p2 = UploadPayload::from_map(&m, "home-001", b"secret");
        assert_eq!(p1.id, p2.id);
        assert_eq!(p1.devices.len(), 1);
        let json = serde_json::to_string(&p1).unwrap();
        assert!(!json.contains("AA:BB:CC"));
        assert!(!json.contains("10.0.0.1"));
        assert!(!json.contains("\"raw\""));
        let p3 = UploadPayload::from_map(&m, "home-001", b"other-secret");
        assert_ne!(p1.id, p3.id);
        assert_ne!(p1.devices[0].pseudonym, p3.devices[0].pseudonym);
        let m2 = NetworkMap {
            captured_at: 200,
            devices: vec![sample_device("AA:BB:CC:11:22:33", Some(-60))],
            raw: Default::default(),
        };
        let p4 = UploadPayload::from_map(&m2, "home-001", b"secret");
        assert_eq!(p1.devices[0].pseudonym, p4.devices[0].pseudonym);
        assert_ne!(p1.id, p4.id);
    }

    #[test]
    fn append_bounded_never_splits_a_line() {
        let dir = std::env::temp_dir();
        let path = dir.join("detectic_append_test.jsonl");
        let _ = std::fs::remove_file(&path);
        for _ in 0..2 {
            append_bounded(
                path.to_str().unwrap(),
                "{\"device\":\"aa:bb:cc:dd:ee:ff\",\"rssi\":-50}",
                10,
            );
        }
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(!content.contains("aa:bb:cc:dd:ee:ff"));
        let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
        for l in &lines {
            assert!(l.contains('{') && l.contains('}'));
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn drain_buffer_drops_sent_keeps_failed() {
        let dir = std::env::temp_dir();
        let path = dir.join("detectic_drain_test.jsonl");
        std::fs::write(
            &path,
            "1700000000 {\"id\":\"a\"}\n1700000001 {\"id\":\"b\"}\n",
        )
        .unwrap();
        drain_buffer(path.to_str().unwrap(), |_b| true);
        assert!(!path.exists());
        std::fs::write(
            &path,
            "1700000000 {\"id\":\"a\"}\n1700000001 {\"id\":\"b\"}\n",
        )
        .unwrap();
        drain_buffer(path.to_str().unwrap(), |b| !b.ends_with(b"\"b\"}"));
        let remaining = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(remaining.contains("\"id\":\"b\""));
        assert!(!remaining.contains("\"id\":\"a\""));
        assert_eq!(remaining.lines().filter(|l| !l.is_empty()).count(), 1);
        let _ = std::fs::remove_file(&path);
    }
}
