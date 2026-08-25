# Detectic — EX520 On-Router Runtime Audit

## Summary

Detectic is a Rust sensor for TP-Link EX520V. The current build targets `aarch64-unknown-linux-musl` for MediaTek MT7981 Cortex-A53. The binary is static, <3 MB, with no C dependencies in the on-router configuration.

## Cargo.toml

- package: detectic 0.1.0 edition 2021
- bin: src/main.rs
- lib: src/lib.rs
- default features: `persist` includes rusqlite bundled + rustls + webpki-roots
- on-router build: `--no-default-features` → zero C deps, ureq + pure Rust crypto
- dependencies:
  - ureq 2 (json, no TLS by default)
  - aes 0.8, cbc 0.1, block-padding 0.3
  - num-bigint 0.4, rsa 0.9, md-5 0.10, rand 0.8
  - base64 0.22, hex 0.4
  - serde 1 derive, serde_json 1
  - clap 4 derive + env
  - hmac 0.12, sha2 0.10
  - optional: rusqlite bundled, chrono, rustls, webpki-roots

## Build profile

release: opt-level = z, lto = true, codegen-units = 1, strip = true
release-with-debug inherits release, strip = false, debug = true

## .cargo/config.toml

target.aarch64-unknown-linux-musl:
- linker = rust-lld
- rustflags:
  -C target-cpu=cortex-a53
  -C link-self-contained=yes

## Runtime requirements

CPU:
- ARMv8-A Cortex-A53 @ MT7981
- architecture: aarch64
- ABI: linux-musl, little endian
- libc: musl (static)

Binary size:
- target <3 MB static musl
- current release estimate: ~2-3 MB stripped

Filesystem:
- Executable path writable: /var/run/misc/misc_rw  (UBI, persistent)
- Config path: same writable area
- Secret path: same writable area, must survive reboot
- Data path: same writable area for SQLite/JSONL buffer
- Log path: stdout/syslog, optional bounded file

Configuration:
- DETECTIC_URL, DETECTIC_USER, DETECTIC_PASSWORD
- DETECTIC_SECRET (HMAC-SHA256)
- DETECTIC_SENSOR_ID
- DETECTIC_INTERVAL
- DETECTIC_UPLOAD_URL optional
- DETECTIC_BUFFER / DETECTIC_BUFFER_MAX

Storage:
- SQLite optional for persist feature, bundled libsqlite3
- Offline buffer JSONL bounded, e.g. 64 KiB
- Pseudonymization secret must persist across reboots

Network:
- HTTP to router LAN IP on port 80 for GTPR/GDPR
- Optional HTTPS to backend for upload
- No external dependencies

Logging:
- Lightweight stdout / syslog
- Never log raw MAC

Pseudonymization:
- HMAC-SHA256 with per-sensor secret
- Device ID derived from MAC/IP stable

## Modules

src/main.rs CLI & sensor loop
src/gtpr.rs GTPR/GDPR client, AES-128-CBC + RSA sign
src/crypto.rs AES, RSA, MD5, HMAC-SHA256
src/collector.rs OID → NetworkMap
src/store.rs SQLite persistence
src/publisher.rs upload with retry/backoff
src/config.rs config loading
src/service.rs service loop
src/runtime.rs RSS usage
src/launcher.rs launcher probe

## Release build target

aarch64-unknown-linux-musl
static linking self-contained
Cortex-A53 tuning, no crypto extensions required

## Notes

Persist feature gated to keep on-router binary tiny.
Transport via plain HTTP to router; backend upload can be HTTPS with rustls if persist enabled.
