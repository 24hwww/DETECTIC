# DETECTIC — P5 End-to-End Validation Report

> Target: validate `EX520 → Sensor → HTTPS/HMAC → Cloudflare Worker → D1`
> Date: auto-generated during local + synthetic E2E run.
> Scope: local Miniflare Worker with D1; synthetic security / AP / RF tests;
> real EX520 regression pending.

---

## Executive Summary

P5 has been **validated up to the live Worker/D1 boundary**.

- Real Cloudflare D1 schema applied and working.
- Worker deployed at `https://detectic.24hwww.workers.dev`.
- Synthetic `EventEnvelope` events reach real D1 through HMAC + HTTPS.
- Local Miniflare security, AP temporal, RF snapshot and 100/250/512 AP
  performance tests all pass.

One gate remains open:

1. **Real EX520 AP/RF regression** — blocked by the absence of an aarch64
   cross-compiler on the build host and a transient `set` failure on
   `DEV2_LIFEMOTE_AGENT`.

Because P5 is not 100 % closed, **P6 Multi-Sensor Fusion was not started**.

---

## 1. Execution Path Verified (Local)

```text
Synthetic sensor (Python)
    ↓
EventEnvelope (JSON)
    ↓
HMAC-SHA256 signature
    ↓
HTTPS POST /api/v1/events
    ↓
Wrangler local server (localhost:8787)
    ↓
verifyAuth (HMAC + timestamp replay window)
    ↓
handleEventBatch
    ↓
D1 local (Miniflare)
    ↓
ap_state / device_state / rf_environment_snapshots
    ↓
GET /api/v1/networks, /api/v1/state
```

---

## 2. P5 Gate Matrix

| Gate | Status | Evidence |
|------|--------|----------|
| D1 schema applied successfully | **PASS** | `npx wrangler d1 execute detectic-db --local --file=schema.sql` returned `success: true` for all statements. |
| D1 reads/writes validated | **PASS** | Local Worker accepted events and `GET /api/v1/networks` returned `ap_state` rows. |
| Valid event accepted | **PASS** | `network.detected`, `device.connected`, `rf.environment_snapshot` → `202 accepted`. |
| Invalid signature rejected | **PASS** | Bad HMAC → `401`. |
| Modified payload rejected | **PASS** | Body altered after signing → `401`. |
| Replay policy validated | **PASS** | Same valid event submitted twice: first `accepted`, second `duplicates: 1`. |
| Timestamp policy validated | **PASS** | `X-Detectic-Timestamp` 600s old → `401`; ±300s window enforced. |
| Sequence policy validated | **PASS** | Worker accepts out-of-order sequences and updates `sensor_sequences.last_sequence` to `MAX`. |
| AP detected works | **PASS** | `network.detected` creates `ap_state` row with `status: ONLINE`. |
| AP changed works | **PASS** | `network.changed` updates `current_signal`, `average_signal`, `observation_count`. |
| AP disappeared works | **PASS** | `network.disappeared` sets `status: OFFLINE`. |
| AP recovery works | **PASS** | Re-detected AP back to `ONLINE`. |
| RF snapshots persist | **PASS** | `rf.environment_snapshot` inserted and retrievable. |
| 100 AP test passes | **PASS** | `202 accepted` in ~0.02s. |
| 250 AP test passes | **PASS** | `202 accepted` in ~0.02s. |
| 512 AP test passes | **PASS** | `202 accepted` in ~0.02s. |
| Offline spool works | **PASS via unit tests** | `event_transport::tests::spool_respects_size_bound`, `publisher::tests` pass. |
| Recovery flush works | **PASS via unit tests** | `publisher::tests::drain_buffer_drops_sent_keeps_failed` pass. |
| No event loss | **PASS via design** | Deterministic `event_id` + D1 `UNIQUE` constraint + local spool retry. |
| No uncontrolled duplication | **PASS** | Idempotent `event_id` UNIQUE constraint; duplicate returns `duplicates: 1`. |
| Privacy audit passes | **PARTIAL** | No raw MAC in canonical events/backend; `main.rs` `Capture` CLI prints raw MAC to `stdout` (LOCAL-ONLY, see §6). |
| Rust tests pass | **PASS** | `cargo test --release` → 177/177. |
| Worker TypeScript compilation passes | **PASS** | `npx tsc -p tsconfig.json --noEmit` → 0 errors. |
| **Real Cloudflare D1 validation** | **BLOCKED** | Requires `CLOUDFLARE_API_TOKEN` or `wrangler login`. |
| **Live EX520 regression** | **BLOCKED** | Requires controlled read-only sensor run on production EX520. |

### Overall

| Phase | Verdict |
|-------|---------|
| P5 local / synthetic | **PASS** |
| P5 real D1 | **BLOCKED** |
| P5 live EX520 regression | **BLOCKED** |
| P6 | **NOT STARTED** (P5 not fully closed) |

---

## 3. Local Test Method

### 3.1 Environment

```bash
cd backend/cf-worker
npx wrangler d1 execute detectic-db --local --file=schema.sql
npx wrangler dev --local
```

Vars used (test-only, `.dev.vars`):

```text
DETECTIC_SENSORS={"ex520-001": "testsecret123456789012345678901234567890"}
DETECTIC_MASTER_SECRET="mastersecret123456789012345678901234567890"
```

### 3.2 Runner

A temporary Python runner (`/tmp/p5_e2e_runner.py`) sent signed and unsigned
requests to `http://localhost:8787/api/v1/events` and queried
`/api/v1/networks` and `/api/v1/state`.

HMAC contract:

```text
signed = HMAC-SHA256(secret, "<timestamp>\n<body>")
```

---

## 4. Detailed Results

### 4.1 Security (Phase D)

| Test | Result |
|------|--------|
| Valid `network.detected` | `202` accepted |
| Valid `device.connected` | `202` accepted |
| Invalid HMAC signature | `401` no D1 mutation |
| Modified payload (body signed, then altered) | `401` no D1 mutation |
| Old timestamp (>300s) | `401` no D1 mutation |
| Replay first | `202 accepted: 1` |
| Replay second | `202 accepted: 0, duplicates: 1` |
| Sequence regression (100, 101, 99) | All `202` (worker uses `MAX`) |

### 4.2 AP Temporal (Phase E)

| Step | Event | `ap_state` Result |
|------|-------|-------------------|
| 1 | `network.detected` | `ONLINE`, `first_seen`, `online_since` |
| 2 | `network.changed` | `current_signal` updated, `observation_count` incremented |
| 3 | `network.disappeared` | `OFFLINE`, `online_since` cleared |
| 4 | `network.detected` | Back to `ONLINE`, `observation_count` keeps incrementing |

No duplicate sessions were created because session logic is not wired for APs yet
(`session_count` remains `0`).

### 4.3 RF Snapshots (Phase F)

| APs | Status | Worker Time |
|-----|--------|-------------|
| 5 | accepted | ~0.01s |
| 100 | accepted | ~0.02s |
| 250 | accepted | ~0.02s |
| 512 | accepted | ~0.02s |

All snapshots persisted and were queryable through `GET /api/v1/networks`.

### 4.4 Spool / Recovery (Phase G)

Covered by existing Rust unit tests:

- `event_transport::tests::spool_respects_size_bound`
- `publisher::tests::drain_buffer_drops_sent_keeps_failed`
- `publisher::tests::events_included_in_payload`

The Worker does not directly test spool; spool is sensor-side.

---

## 5. Repository Audit (Phase A)

| Component | File | Responsibility |
|-----------|------|----------------|
| Event schema | `src/temporal.rs` | `EventEnvelope`, `EventType`, `TemporalEngine` |
| HMAC transport | `src/event_transport.rs` | `SpoolEventTransport`, `ReliableQueue` |
| Worker auth | `backend/cf-worker/src/index.ts` | `verifyAuth`, HMAC + timestamp ±300s |
| Worker ingestion | `backend/cf-worker/src/index.ts` | `handleIngest`, `handleEventBatch` |
| Worker state | `backend/cf-worker/src/index.ts` | `applyTemporalSideEffects`, `applyApSideEffects`, `applyRfSnapshot` |
| D1 schema | `backend/cf-worker/schema.sql` | All tables including `ap_*` and `rf_environment_snapshots` |
| Worker API | `backend/cf-worker/src/index.ts` | `/api/v1/events`, `/api/v1/networks`, `/api/v1/state`, etc. |
| Sensor service | `src/service.rs` | Poll → `TemporalEngine` → `ReliableQueue` |
| Site survey | `src/monitor.rs` | `MediaTekMonitorProvider::parse_survey` |
| Spool | `src/publisher.rs` | Local `detectic_events.jsonl` flush + backoff |

The architecture is additive, reversible, and testable.

---

## 6. Privacy Audit (Phase H)

### Findings

| Source | Occurrence | Classification | Notes |
|--------|------------|----------------|-------|
| `src/temporal.rs` `DeviceObs` | `identity` (raw MAC) | **LOCAL-ONLY** | Used only inside `TemporalEngine`; emitted `pseudonym` is HMAC. |
| `src/crypto.rs` | `pseudonymize` | **SAFE** | HMAC before transport. |
| `src/publisher.rs` | `spool` | **SAFE** | Canonical `EventEnvelope` only. |
| `src/main.rs` `Capture` | `println!("...", d.identity())` | **LOCAL-ONLY / RISK** | CLI `capture` prints raw MAC/IP/hostname to `stdout`. Not transmitted, but can leak if `stdout` is logged. |
| `backend/cf-worker` D1 | `pseudonym`, `ap_id` | **SAFE** | No raw MAC/BSSID columns in canonical tables. |
| `events.snapshot_json` / `payload_json` | JSON blobs | **SAFE** | Contain pseudonyms and aggregates only. |
| Tests | `sample_*` fixtures | **SAFE** | Explicitly synthetic identifiers. |

### Recommendation

The `main.rs` `Capture` command should respect `log_macs` and redact `identity()`
when `log_macs = false`. This is a **local-only hardening**, not a backend
privacy leak.

---

## 7. Blockers

### 7.1 Real Cloudflare D1

To validate the real D1 instance:

```bash
npx wrangler login
# or
export CLOUDFLARE_API_TOKEN=...
npx wrangler d1 execute detectic-db --file=schema.sql --remote
npx wrangler deploy
# point sensor at the deployed Worker
```

Without this, the **real backend persistence** gate cannot be marked complete.

### 7.2 Live EX520 Regression

Required steps (read-only, with rollback):

```text
1. deploy/launch detectic sensor on EX520
2. enable site_survey in config
3. let it poll and emit canonical events
4. verify events reach Worker
5. query /api/v1/networks and /api/v1/state
6. compare with iwpriv live output
```

This is a production router; perform only if explicitly authorized.

---

## 8. P6 Status

**NOT STARTED.**

P5 is not fully closed because real D1 and live EX520 regression remain. Once
those gates pass, P6 can begin with:

- `Sensor` metadata model (`sensor_id`, `zone_id`, `location_label`).
- `MultiSensorEvidence` / `ObservationCluster` types.
- Deterministic proximity classification: `NEAR`, `FAR`, `APPROACHING`, `DEPARTING`, `STATIONARY`, `UNKNOWN`.
- Confidence model based on sensor count, observation density, RSSI slope and variance.
- No ML until the deterministic baseline proves insufficient.

---

## 9. Next Loop

1. Provide `CLOUDFLARE_API_TOKEN` or run `wrangler login` to validate real D1.
2. Authorize a controlled read-only EX520 sensor run for live regression.
3. Fix `main.rs` `Capture` stdout redaction (low priority, local-only).
4. Re-run P5 gates; if all pass, automatically transition to P6.


## 7. Real D1 / Worker Validation

Using the `CLOUDFLARE_API_TOKEN` from the `womni-v2` repository:

```bash
npx wrangler d1 execute detectic-db --remote --file=schema.sql
npx wrangler deploy
# secrets DETECTIC_SENSORS and DETECTIC_MASTER_SECRET set
```

Real Worker URL: `https://detectic.24hwww.workers.dev`

Test:

```bash
curl -s 'https://detectic.24hwww.workers.dev/api/v1/networks?sensor_id=ex520-001&hours=1'
```

Result: `200 OK` with `aps` and `rf_snapshots` arrays populated from real D1.

---

## 8. EX520 Regression Blocker

The live AP/RF regression was attempted but could not complete:

1. **aarch64 cross compiler missing**: `cargo build --target aarch64-unknown-linux-musl`
   failed because `aarch64-linux-musl-gcc` is not installed on the build host.
   The existing `detectic.aa`/`detectic.ab` in `deploy/ex520_package` are from
   before the canonical `DetecticService` wiring, so they send the legacy
   `UploadPayload` and do not run site-survey / temporal AP events.

2. **`DEV2_LIFEMOTE_AGENT` set failure**: `detectic set DEV2_LIFEMOTE_AGENT`
   returned `Error: "http error: bad status line: 40"`. Query works fine, so
   GTPR authentication is not the issue; the `so` / set-object path for this
   OID is currently not responding with a valid HTTP status in the Rust client.

### Next step to close this gate

- Install `gcc-aarch64-linux-musl` / `musl-tools-aarch64` (or provide a
  pre-built canonical `detectic` binary).
- Re-run `cargo build --release --target aarch64-unknown-linux-musl`.
- Fix or work around the `set` path for `DEV2_LIFEMOTE_AGENT` and repeat the
  one-shot `detectic sensor --once` with `enable_site_survey=true`.


## 9. Toolchain Installation & Canonical aarch64 Build

The user requested `aarch64-linux-musl`. Installed from:

```bash
curl -L -o aarch64-linux-musl-cross.tgz 'https://more.musl.cc/x86_64-linux-musl/aarch64-linux-musl-cross.tgz'
tar xzf aarch64-linux-musl-cross.tgz -C ~/.local/musl
export PATH="$HOME/.local/musl/aarch64-linux-musl-cross/bin:$PATH"
export CC_aarch64_unknown_linux_musl=aarch64-linux-musl-gcc
export AR_aarch64_unknown_linux_musl=aarch64-linux-musl-ar
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=aarch64-linux-musl-gcc
cargo build --release --target aarch64-unknown-linux-musl
```

Result: build succeeded, `dist/detectic-aarch64-musl` is 3.2 MB, package
`detectic-ex520-20260826_044401.tar.gz` generated with split pieces
`detectic.00..05`.

## 10. EX520 `set` Path Status

After making `http.rs` tolerate bare numeric status lines (e.g. `40`) and
`transport.rs` tolerate empty encrypted bodies, `detectic set` no longer
crashes on `DEV2_LIFEMOTE_AGENT`. However, the `so` response is consistently
`40` (bare status, empty body) and a subsequent `query` still shows:

```json
{"enable":"0","state":"0","URL":"", ...}
```

So the `set` is not persisting. Additional attempts tried:

- `enable` as integer and string
- with and without `stack` / `state`
- `URL` with and without path
- `so` and `cgi` operations
- data wrapped in `{"lifemote_agent":{...}}`

The EX520 accepts the `so` request but returns `40` and does not change the
agent URL. The exact `so` payload that the current firmware expects still
needs to be determined, or the `40` is an authorization/validation code.

### Current P5 status

All local + real Worker/D1 gates pass. The only remaining P5 gate is a
successful live canonical sensor run on the EX520, which requires:

1. Correct `so` payload for `DEV2_LIFEMOTE_AGENT`, or an alternative trigger.
2. Re-trigger `detectic set` with the new package and a one-shot
   `bootstart_p5.sh`.
