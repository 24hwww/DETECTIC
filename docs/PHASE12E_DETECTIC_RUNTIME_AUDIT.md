# PHASE12E_DETECTIC_RUNTIME_AUDIT

Audit of actual Detectic source/binary.

## Executable arguments
PROVEN_FROM_SOURCE: Cli supports subcommands:
- Map, Sensor {once}, Status, Version, Health, Config, Spool, Update, Rollback, Uninstall
- Query/Set/Cgi/Op for router access
- Launcher {Install/Remove/Status}
- Sensor runs loop with interval, --once supported
Classification: PROVEN_FROM_SOURCE

## Environment variables
PROVEN_FROM_SOURCE: DETECTIC_URL, DETECTIC_USER, DETECTIC_PASSWORD, DETECTIC_SECRET, DETECTIC_INTERVAL, DETECTIC_BACKEND_URL, DETECTIC_BUFFER, DETECTIC_BUFFER_MAX, DETECTIC_SENSOR_ID, DETECTIC_DIALECT, DETECTIC_LOG_LEVEL, DETECTIC_LOG_MACS, etc.
Classification: PROVEN_FROM_SOURCE

## Config files
PROVEN_FROM_SOURCE: key=value file supported via from_file. Path via DETECTIC config.
Classification: PROVEN_FROM_SOURCE

## Working directory
UNKNOWN: Binary may run from any cwd. Spool path defaults to /tmp/detectic_buffer.jsonl
Classification: PROVEN_FROM_SOURCE

## State directory
PROVEN_FROM_SOURCE: sensor_id_path default /var/run/misc/misc_rw/detectic/state/sensor_id
Classification: PROVEN_FROM_SOURCE

## Log behavior
PROVEN_FROM_SOURCE: LogLevel Error/Warn/Info/Debug, log_macs default false. Output to stdout/stderr.
Classification: PROVEN_FROM_SOURCE

## Stdout/stderr behavior
PROVEN_FROM_SOURCE: Sensor prints [detectic] sensor started, snapshot events. No structured heartbeat.
Classification: PROVEN_FROM_SOURCE

## Signal handling
UNKNOWN: No explicit signal handling found in main.rs. Likely default.
Classification: UNKNOWN

## Exit codes
PROVEN_FROM_SOURCE: Result<(), Box<dyn Error>>; non-zero on failure.
Classification: PROVEN_FROM_SOURCE

## Startup time
UNKNOWN: Depends on router latency. Not measured.
Classification: LIVE_REQUIRED

## Heartbeat capability
UNKNOWN: No built-in heartbeat endpoint. Health derived from process existence and logs.
Classification: UNKNOWN

## Backend retry behavior
PROVEN_FROM_SOURCE: upload_with_retry, backoff_delay, max_upload_retries config.
Classification: PROVEN_FROM_SOURCE

## Local health mechanism
SIMULATED: Health command probes config, router reachability, auto_start_supported. No persistent health file.
Classification: SIMULATED

## Graceful shutdown behavior
UNKNOWN: No explicit shutdown hook. Loop sleep based.
Classification: UNKNOWN

## Actual memory requirements
UNKNOWN: Static binary musl. Estimated RSS < 32 MB. Not measured live.
Classification: LIVE_REQUIRED

## Actual network destinations
PROVEN_FROM_SOURCE: router_url HTTP for GTPR, backend_url optional for upload.
Classification: PROVEN_FROM_SOURCE

## Actual writable paths
PROVEN_FROM_SOURCE: spool_path default /tmp/detectic_buffer.jsonl, sensor_id_path /var/run/misc/misc_rw/detectic/state/sensor_id
Classification: PROVEN_FROM_SOURCE

## Features
- persist feature gated for rusqlite/rustls. On-router build --no-default-features => tiny static binary.
- No --daemon flag, no --log flag. Logging to stdout.
- No health endpoint.
- No stdout heartbeat.

Classification summary:
PROVEN_FROM_SOURCE: CLI args, env vars, config file, state path, spool path, backend retry
PROVEN_FROM_BINARY: static musl aarch64 1.3 MB
SIMULATED: local health
UNKNOWN: signal handling, graceful shutdown, startup time, heartbeat
LIVE_REQUIRED: memory RSS, actual runtime latency

## Corrections to spec
- Telnet upload via scp-like not possible. File transfer must be abstract.
- pidof/pgrep not guaranteed. Use ps parsing.
- /proc/pid/exe not proven.
- No symlink requirement; current design ok.
- Detectic does NOT support --daemon/--log/health endpoint/stdout heartbeat.
- CPU/RAM limits are targets, not proven kills.
- misc_rw 12 MB not proven.
- Telnet persistence unknown until Phase12F.

Conclusion: Controller design must adapt to real Detectic behavior: env-var config, spool file, no daemon flags, no health endpoint.
