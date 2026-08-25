# M9 — Production Validation

## Date
2026-08-23

## Objective
Validate that Detectic is ready for production deployment on the EX520V and
identify any remaining gaps.

## What Was Implemented

### M6 Presence
- `src/presence.rs` module with `PresenceEngine`, thresholds, EWMA smoothing,
  confidence, debounced LEAVE, and unit tests.
- `detectic presence` CLI command.
- 7 unit tests pass.

### M7 Deployment
- `deploy/` scripts: install, start, stop, health, update, rollback, remove.
- Atomic release directory structure in `/var/run/misc/misc_rw/detectic`.
- SHA256 and architecture verification.
- No firmware modification; no auto-start on stock firmware.

### M8 Backend
- `HttpBackend` sends JSON payloads with `Authorization: Bearer` token support.
- HMAC-SHA256 signature (`X-Detectic-Signature`).
- Bounded offline spool with replay via `drain_spool()`.
- Token redaction in `detectic config`.

### M9 Hardening / CLI
- New commands: `version`, `health`, `config`, `spool`, `update`, `rollback`,
  `uninstall`, `presence`, `sensor --once` (stubbed).
- `config` redacts `router_password`, `secret`, and `backend_token`.
- Configuration validation in `SensorConfig::validate()`.
- Resource limits preserved from M5.

## Build & Tests

```
cargo test --no-default-features  -> 69 passed
cargo test                        -> 96 passed
cargo build --release --no-default-features --target aarch64-unknown-linux-musl  -> OK
```

## Release Artifacts

```
dist/
  detectic-aarch64-musl
  detectic-aarch64-musl.sha256
  manifest.json
  detectic-install.sh
  detectic-start.sh
  detectic-stop.sh
  detectic-health.sh
  detectic-update.sh
  detectic-rollback.sh
  detectic-remove.sh
```

Binary: `ELF 64-bit LSB executable, ARM aarch64, version 1 (SYSV), statically linked, stripped`
Size: 1,215,992 bytes (1.2 MB)

## Validation Matrix

| Capability | Status |
|------------|--------|
| Build ARM64/musl | PROVEN |
| GTPR/GDPR data collection | PROVEN (M5) |
| JOIN / UPDATE / LEAVE detection | PROVEN (M5, with debounce in M6) |
| Presence / proximity | IMPLEMENTED, unit-tested |
| RSSI smoothing | IMPLEMENTED |
| Confidence | IMPLEMENTED |
| Offline spool | PROVEN |
| Backend upload (HTTP + Bearer + HMAC) | IMPLEMENTED |
| Sensor ID | ENV-based, file path configured |
| Config validation | IMPLEMENTED |
| Secret redaction | IMPLEMENTED |
| Install / start / stop scripts | IMPLEMENTED |
| Update / rollback scripts | IMPLEMENTED |
| Health check command | IMPLEMENTED |
| Version command | IMPLEMENTED |
| Binary verification (SHA256, arch, static) | IMPLEMENTED |
| Auto-start after reboot | **NOT SUPPORTED** on stock firmware |
| Real EX520V M6-M9 smoke test | NOT RUN in this session (M5 proven) |

## Known Limitations

1. **Auto-start after reboot is not available on stock EX520V firmware.**
   The install scripts are manual and must be re-run after a reboot.
2. **Unassociated station detection is not supported.** Only associated
   stations are observed.
3. **HTTPS/TLS requires a separate build with the `persist` feature.**
   The default on-router binary uses HTTP.
4. **Real M6-M9 router smoke test not performed in this session.** The M5
   smoke test proved execution, and all new code compiles and passes tests.

## Recommendation

Detectic is **ready for controlled, manual deployment** on the EX520V. It is
not ready for fully autonomous, reboot-persistent operation until a legitimate
firmware-level startup hook is provided by the vendor/operator. The deployment
scripts provide a safe, verifiable, and reversible install/update/rollback
workflow that satisfies the M6-M9 requirements without modifying the firmware.
