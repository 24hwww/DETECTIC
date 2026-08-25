# Changelog — Phase 2: EX520 without firmware modification

> Firmware target: `EX520V124101568249n_agc3000_0945460481` (MT7981 / Cortex-A53)
> Constraints respected: no firmware rebuild, no binary patch, no RO writes,
> no `backupcfg.bin` exploit, communication only via GDPR/GTPR.

---

## Phase A — Production build (MT7981 / ARM64)

**Deliverables (all present):**

| Artifact | Size | SHA256 |
|----------|------|--------|
| `target/aarch64/release/detectic` | 1.1 MB | `f89ff35f6529f9a26de4795ab3773ac5ad9c00b1cda5f91763b06a9dd6e4a3d9` |
| `target/aarch64-unknown-linux-musl/release/detectic` | 1.1 MB | `f89ff35f…` (same file) |
| `target/release/detectic` (host x86_64) | 2.2 MB | `6667b77bd16e563347aa5d76503516549aca3666ee1e8b638072d8bf7a1e0219` |

- `detectic.sha256` at the repo root lists all three.
- Binary properties (`file` output): *ELF 64-bit LSB executable, ARM aarch64,
  statically linked, stripped*. **Well under the 3 MB target.**
- `.cargo/config.toml`: `target-cpu=cortex-a53`, and — key finding —
  **`linker = "rust-lld"`**: the host GNU `ld` cannot link the AArch64
  self-contained CRT objects (`Relocations in generic ELF`), so an external
  musl cross-toolchain is *not* required; rust-lld (bundled with every Rust
  toolchain) performs the cross link reproducibly.
- `Cargo.toml [profile.release]`: `opt-level="z"`, `lto=true`,
  `codegen-units=1`, `strip=true` (+ `release-with-debug` profile for
  symbolized builds).
- Reproduce with:
  ```bash
  cargo build --target aarch64-unknown-linux-musl --release --no-default-features
  cp target/aarch64-unknown-linux-musl/release/detectic target/aarch64/release/detectic
  sha256sum target/aarch64-*/release/detectic target/release/detectic > detectic.sha256
  ```

## Phase B — Runtime abstraction

| Layer | Module | Responsibility |
|-------|--------|----------------|
| Transport | `src/transport.rs` | GDPR session (RSA/AES, `getGDPRParm`, login, `TokenID`, encrypted `gl`). Trait `Transport { fn gl(&self, oid) -> Result<String, GtprError> }`. |
| Collector | `src/collector.rs` | Pure OID→`NetworkMap` merge (Wi-Fi + DHCP + host). Depends only on `&dyn Transport` — verified by a fake-transport unit test. No HTTP/crypto. |
| Publisher | `src/publisher.rs` | Pseudonymized `UploadPayload`, HMAC signing, retry/backoff, bounded JSONL buffer. |

- `src/gtpr.rs` kept as a compatibility shim re-exporting transport+collector;
  all pre-existing tests still pass unchanged.
- `src/main.rs` wires `Transport → Collector → Publisher`; new `analytics`
  subcommand added.

## Phase C — Deployment strategy

`DEPLOYMENT_PATHS.md`: full survey of `_rootfs` (rcS, rcS.model, inittab,
fstab, rcS_hook, crond, service manager, partitions). Conclusion:
`/var/run/misc/misc_rw` (UBI) is the only persistent+writable+executable
location; no writable boot hook exists → manual relaunch after reboot is the
remaining gap.

## Phase D — Shell enablement research

`REMOTE_ACCESS_OBJECTS.md`: mapped `DEV2_SSH_CFG`
(`Device.X_TP_AppCfg.SSHCfg.`, handlers `oal_dropbearRestart`) and
`DEV2_TELNET_CFG` (`Device.X_TP_AppCfg.TelnetCfg.`, chain
`rsl_initTelnetCfgObj → rsl_setDev2TelnetCfgObj → oal_setTelnetd → telnetd -p %d &`),
plus all related build flags (`INCLUDE_SSH_ACCESS=0`, `INCLUDE_WEB_TELNET=1`,
`INCLUDE_REMOTE_TELNET=1`, `CONFIG_PACKAGE_dropbear=y`). Live `gl` inspection
commands included; no success claimed without hardware evidence.

## Phase E — Privacy hardening (verified end-to-end)

- Sensor upload contains only: `pseudonym`, `rssi`, `standard`, `source`,
  `radio_mac`.
- Backend schema now `detections(id, snapshot_id, sensor_id, pseudonym, rssi,
  source, standard, radio_mac)` — legacy `hostname`/`ip` columns are migrated
  away automatically on startup.
- Backend sanitizes the snapshot JSON before persisting it.
- **Verified against the running stack:** DB introspection shows no MAC/IP/
  hostname anywhere in `detections` or stored `raw_json`.

## Phase F — Presence analytics

New `src/analytics.rs` (pure, reusable):

- `PresenceStats`: `first_seen`, `last_seen`, `visit_duration_secs`,
  `distinct_days`, `recurrence_score ∈ [0,1]`, `hour_histogram[24]`,
  `weekday_histogram[7]`, RSSI aggregates.
- Entry points: `PresenceStats::from_observations()`, `aggregate_presence()`,
  `presence_from_store_rows()` (persist-gated), histogram formatters.
- CLI: `detectic analytics` renders duration/recurrence/histograms.
- 5 unit tests including a known-calendar anchor (1970-01-01 = Thursday).

---

## Validation matrix (all green)

| Check | Result |
|-------|--------|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets` | 0 warnings (after fixes) |
| `cargo test` | 26 passed (21 lib + 5 bin), 0 failed |
| `cargo test --no-default-features` | 14 passed |
| Release build host | OK (2.2 MB) |
| Cross release build | OK — static AArch64 musl, 1.1 MB, stripped |
| Mock-router integration: `map` | OK — 2 devices merged from ASSOCDEV/DHCP |
| Mock-router integration: `sensor` + backend | OK — HMAC-authenticated upload accepted (HTTP 200), devices listed via `/api/v1/devices` |
| Privacy verification | OK — no raw identifiers in backend DB or snapshot JSON |
| Offline buffering | OK — backend down → snapshot buffered to JSONL; backend restored → buffer drained automatically, snapshots ingested |

No existing tests were removed.

---

## Remaining blockers (with evidence)

| Blocker | Evidence |
|---------|----------|
| `backupcfg.bin` sample is password-protected | `investigations/backupcfg/REPORT.md` §4: full 2³² brute-force with no password and empty password → 0 hits |
| `DeviceInfo[0x51c]` runtime value unknown | Offset proven by disassembly (`sp+0x554 − sp+0x38 = 0x51c`); value requires live read |
| Autostart across reboots | `DEPLOYMENT_PATHS.md` §4: `rcS` is read-only and sources nothing from `misc_rw`; no procd/systemd/overlay present |
| Whether writing `DEV2_TELNET_CFG`/`DEV2_SSH_CFG` actually starts `telnetd`/`dropbear` | Handlers exist (`oal_setTelnetd`, `telnetd -p %d &` strings) but `INCLUDE_SSH_ACCESS=0` may gate apply; needs one live `gl`/`go` round-trip on hardware (`REMOTE_ACCESS_OBJECTS.md` §4) |
| AArch64 binary execution not smoke-tested off-device | `qemu-user-static` unavailable on this host (no sudo); binary verified as static AArch64 ELF via `file`/`readelf` |

## Files changed / added

- Modified: `Cargo.toml`, `src/lib.rs`, `src/main.rs`, `backend/server.py`
- Added: `.cargo/config.toml`, `src/transport.rs`, `src/collector.rs`,
  `src/publisher.rs`, `src/analytics.rs`, `src/gtpr.rs` (shim),
  `DEPLOYMENT_PATHS.md`, `REMOTE_ACCESS_OBJECTS.md`, `CHANGELOG_PHASE2.md`,
  `detectic.sha256`, artifacts under `target/aarch64{,-unknown-linux-musl}/release/`
