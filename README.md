# Detectic

A lightweight Wi-Fi presence/activity sensor for consumer routers (current
target: TP-Link EX520V). Detectic reads the router's legitimate management API
(GTPR/GDPR) and produces a **pseudonymized, ordered event stream** of associated
devices — without modifying firmware, opening router listeners, or flashing.

See [`AGENTS.md`](AGENTS.md) for the full project philosophy, safety rules, and
milestone definitions.

## Principles
- **External / off-router observer.** Detectic runs as a client that *reads* the
  router API. It does not install itself on the router, modify partitions, or
  change network configuration.
- **Privacy by design.** Raw MAC addresses are pseudonymized locally
  (HMAC-SHA256 with a per-sensor secret) before any event is emitted.
- **No fabrication.** Observations only come from real API responses. On stock
  firmware, unassociated-station (probe) detection is **not supported** and the
  pipeline explicitly ignores empty probe batches.

## Build
```bash
cargo build --release                 # host binary
cargo test --lib                      # 123 lib tests
# Cross-compile for the router (no C deps):
cargo build --release --no-default-features --target aarch64-unknown-linux-musl
```

## CLI
```bash
# Credentials (never hardcoded)
export DETECTIC_URL=http://192.168.0.1
export DETECTIC_USER=user
export DETECTIC_PASSWORD=...
export DETECTIC_SECRET=...            # per-sensor HMAC secret

detectic map            # current network map (associated stations)
detectic sensor         # continuous polling loop
detectic realtime       # unified event pipeline (one cycle, dev/client)
detectic driver         # driver capability matrix (M11-A)
detectic launcher status      # launch probe (M11-E)
detectic launcher install     # safe no-op / refusal on stock firmware
detectic health         # config + binary + reachability check
detectic status         # runtime/resource status
```

## Architecture (modules)
- `driver.rs` — capability-aware `DriverProvider` selection (HAL/iwpriv/GTPR/Null).
- `realtime.rs` — unified, deduplicated, monotonic-`seq` event pipeline (M11-C).
- `launcher.rs` — `DetecticLauncher` (M11-E); refuses firmware-modifying installs.
- `persistence.rs` — `LaunchMode` design space; `StockManual` is the only safe mode.
- `collector.rs` / `transport.rs` — GTPR/GDPR client → `NetworkMap`.
- `presence.rs`, `publisher.rs`, `backend.rs` — presence engine, pseudonymized
  upload, authenticated transport.

## Documentation
- [`investigations/`](investigations) — milestone reports (M4–M11), including the
  M11 recovery incident, boot-mechanism analysis, accuracy, and security posture.
- [`deploy/`](deploy) — install/start/stop/update/rollback/remove scripts
  (manual invocation; no auto-start on stock firmware).

## Milestone status
M11 (permanent sensor) is **implemented off-router** and fully tested. Boot
persistence (M11-D) and reboot validation (M11-F) are documented as procedures
only — they require an authorized reboot and are intentionally blocked under the
no-reboot safety rule.
