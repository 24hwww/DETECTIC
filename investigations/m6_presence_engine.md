# M6 — Presence Engine

## Date
2026-08-23

## Objective
Implement a lightweight, deterministic presence and proximity engine for the
Detectic sensor. It receives successive Wi-Fi station snapshots and produces
debounced JOIN / UPDATE / LEAVE events with smoothed RSSI, proximity
classification, and confidence scores.

## Implementation

### `src/presence.rs`

New module added to the library. It is pure Rust, no I/O, fully unit-tested,
and designed for the EX520V's limited resources.

### Core Types

| Type | Purpose |
|------|---------|
| `PresenceState` | `Present`, `Away`, `Unknown` |
| `Proximity` | `VeryNear`, `Near`, `Medium`, `Far`, `Unknown` |
| `PresenceObservation` | One observation per device with rssi, presence, proximity, confidence, first/last seen, consecutive counters |
| `ProximityThresholds` | Calibratable RSSI thresholds (dBm) |
| `PresenceConfig` | `missing_polls_before_leave`, `rssi_smoothing_alpha`, thresholds |
| `PresenceEngine` | Maintains state, computes observations |

### Thresholds

Default (configurable):

| dBm | Classification |
|-----|----------------|
| ≥ -45 | very_near |
| ≥ -60 | near |
| ≥ -70 | medium |
| ≥ -80 | far |
| < -80 | unknown |

### Hysteresis / Debounce

`missing_polls_before_leave` defaults to `3`. With a 30s poll interval this
means a device is only declared `Away` after ~90 seconds of consecutive misses.

### RSSI Smoothing

EWMA with `rssi_smoothing_alpha` default `0.3`.

```
smoothed[n] = alpha * raw[n] + (1 - alpha) * smoothed[n-1]
```

This reduces rapid proximity class flapping.

### Confidence

A score [0.0, 1.0] computed from:
- sample count (saturates at ~5 observations)
- RSSI stability relative to the smoothed value
- recency

### CLI

`detectic presence` polls the router once and prints per-device presence
observations.

### Tests

Unit tests cover:
- device joins and remains `Present`
- debounced `Away` after 3 missing polls
- proximity classification by smoothed RSSI
- confidence growth with samples
- away devices have zero confidence
- prune stale away devices
- `joined_now` / `left_now` helpers

## Unassociated Stations

The stock EX520V does **not** expose a reliable, legitimate source of
unassociated (probe) stations through the GTPR API, `iwpriv`, `iw`, or
`/proc`/`/sys` in a safe, documented form. The MediaTek driver and TP-Link
firmware do not provide a passive probe request capture interface to user-space
without potentially dangerous kernel/driver modifications.

Therefore **unassociated station detection is not supported** on the current
stock firmware. Detectic's presence engine is based on associated stations and
their temporal history. This limitation is documented and not invented.
