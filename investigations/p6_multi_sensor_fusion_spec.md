# Spec: P6 — Multi-Sensor Fusion (first slice)

> Status: DRAFT — awaiting human review before implementation.

## Objective

Extend the existing Cloudflare Worker + D1 backend so it can receive and
correlate observations from **more than one Detectic sensor**. The first slice
delivers the data model and API surface for cross-sensor AP and device
visibility without claiming spatial positioning.

User stories:
- As an operator I can see which sensors observe a given AP and their relative
  signal strength.
- As an operator I can list all APs currently seen by any sensor, grouped by
  `sensor_id`.
- As an operator I can see when a device pseudonym was last observed by any
  sensor (first cross-sensor recurrence view).

This is the foundation for later zone inference and movement estimation
(Milestone 8).

## Tech Stack

- Rust sensor (existing) — no changes required for this slice.
- Cloudflare Worker (TypeScript, Wrangler, D1) — new endpoints + schema.
- Existing `EventEnvelope`/`ap_state`/`device_state`/`rf_environment_snapshots`
  tables as the source of truth.

## Commands

```bash
# Backend lint + typecheck
npx tsc -p backend/cf-worker/tsconfig.json --noEmit

# Unit / integration tests (Worker local)
cd backend/cf-worker && npm test

# Deploy Worker + schema
npx wrangler deploy
npx wrangler d1 migrations apply detectic-db --remote

# Local sensor integration (when a second sensor is available)
cargo build --release --target aarch64-unknown-linux-musl
```

## Project Structure

```
backend/cf-worker/src/index.ts            # Worker entry, add /fusion endpoints
backend/cf-worker/schema.sql              # add multi-sensor views/tables
investigations/p6_multi_sensor_fusion_spec.md  # this spec
```

## Code Style

- Use the existing HMAC auth + `verifyAuth` pattern for every new endpoint.
- Reuse the D1 repository pattern (`getDb`) and prepared statements.
- Return JSON with `sensor_id` explicitly included in every multi-sensor
  payload.
- Keep raw MACs out of responses (pseudonyms only).

## Testing Strategy

- **Unit:** TypeScript tests for SQL aggregation helpers.
- **Integration:** local Miniflare + D1 with two synthetic sensors
  (`ex520-001`, `ex520-002`) sending `network.detected` and
  `rf.environment_snapshot` events.
- **E2E:** query `/api/v1/fusion?ap_id=...` and assert two sensor rows
  returned with `rssi` and `timestamp`.

## Boundaries

- **Always:** validate HMAC on new endpoints; use pseudonyms; keep SQL
  parameterized.
- **Ask first:** add new npm dependencies; change D1 migrations strategy;
  introduce Workers AI / ML.
- **Never:** store raw MACs; claim meter-level positioning; modify router
  firmware.

## Success Criteria (first slice)

1. `POST /api/v1/events` remains idempotent for any number of sensors.
2. `GET /api/v1/networks?sensor_id=all` returns APs from all sensors with
   per-sensor `rssi`, `status`, `first_seen`, `last_seen`.
3. `GET /api/v1/fusion?ap_id=<ap_id>` returns a list of sensors that observe
   that AP, sorted by strongest recent signal.
4. `GET /api/v1/devices?pseudonym=<p>` returns the last observation across
   all sensors.
5. All existing P5 tests continue to pass.

## Open Questions

1. Should `ap_id` be stable across sensors (same BSSID → same
   `HMAC(master_secret, bssid)`) or per-sensor pseudonym?
2. Do we need a new D1 table for cross-sensor materialized state, or can the
   first slice live on top of `ap_state` + `device_state` queries?
3. Is the second physical sensor available now, or should P6 use synthetic
   `ex520-002` events for the first implementation?
4. Is there a desired max sensor count / SLA for the `/fusion` endpoint?

---

**Approval needed:** confirm or edit the scope above before code starts.


---

## 12. P6 First Slice — IMPLEMENTED & DEPLOYED

Implemented on `backend/cf-worker/src/index.ts` and deployed to
`https://detectic.24hwww.workers.dev`:

1. `GET /api/v1/sensors` now uses canonical `ap_state` and `device_state` tables.
2. `GET /api/v1/networks?sensor_id=all` returns AP state from all sensors with
   `sensor_id` in each row.
3. `GET /api/v1/fusion?ap_id=...` or `?ssid=...` returns the same AP as observed
   by every sensor, sorted by strongest signal.

### Live evidence

```bash
curl -s 'https://detectic.24hwww.workers.dev/api/v1/sensors'
curl -s 'https://detectic.24hwww.workers.dev/api/v1/networks?sensor_id=all&hours=1'
curl -s 'https://detectic.24hwww.workers.dev/api/v1/fusion?ap_id=ap-real-001&hours=1'
```

All three return multi-sensor-aware JSON. The existing `ex520-001` sensor is
visible; a second sensor will naturally appear once its HMAC-signed events are
ingested.

### Remaining for full P6

- Cross-sensor `ap_id` / `pseudonym` stability: currently these are
  `HMAC(sensor_secret, bssid|mac)`, so the same real AP or device has a
  different id on each sensor. True cross-sensor correlation requires either a
  shared pseudonymization key or a backend mapping table.
- Add a second physical sensor (`ex520-002`) or synthetic test for
  end-to-end multi-sensor fusion.
- `GET /api/v1/devices` cross-sensor view once the pseudonym model is aligned.
