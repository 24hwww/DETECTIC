# Detectic — Data-Flow Invariants

## Roles

| Concept | Definition | Stored in | Owned by |
|---|---|---|---|
| **Observation / Snapshot** | What the sensor observed at a particular `captured_at`. | `snapshots`, `detections` | HTTP `/api/v1/events` |
| **Event** | A semantic state transition derived from observations. | `events` | WSS `RealtimeHub` or HTTP `handleEventBatch` |
| **State** | The latest known derived state of a device. | `device_state` | Shared side-effect path (`applyCanonicalEventToD1`) |

## 1. Snapshot invariant

Every accepted HTTP snapshot represents the sensor's observed state at `captured_at`.

- The snapshot is persisted in `snapshots`.
- Each device is persisted in `detections`.
- A successful snapshot ingestion remains queryable.

## 2. State invariant

`device_state` is the authoritative current state of a device for a sensor.

- A device observed in the latest valid snapshot must be `PRESENT`.
- A device reported absent by WSS `device.disconnected` must be `DISCONNECTED`.
- A device not observed for more than the absence threshold must become `ABSENT`.
- A device only seen by an RF probe must be `RF_PRESENT`.
- The `state` column never becomes an empty string due to a partial update.

## 3. Event invariant

Events represent transitions or meaningful changes.

- A snapshot containing the same device state as the previous snapshot does NOT create a new event.
- `device.appeared` / `device.connected` is emitted when a device transitions from not-present to present.
- `device.departed` / `device.disconnected` is emitted when a device transitions from present to not-present.
- `device.signal_changed` is emitted when RSSI changes by at least the configured threshold.
- Canonical event IDs must be deterministic and unique within a sensor.

## 4. HTTP / WSS relationship

- **HTTP snapshot** is the authoritative periodic observation.
- **WSS events** are the low-latency semantic transitions.
- Both feeds derive from the same sensor state but use different transports.
- Duplicate events are prevented by unique `event_id` (`INSERT OR IGNORE` / `INSERT` with `UNIQUE` error handling).
- `device_state` is updated by both transports through the same side-effect path.
- HTTP snapshots always update `device_state` from the `devices` array to guarantee state correctness even if WSS is delayed or drops.

## 5. Latency measurement semantics

The metric `received_at - captured_at` is NOT pure transport latency. It includes:

1. Sensor clock offset.
2. Local buffering / spooling.
3. Network round-trip.
4. Worker processing time.

True transport latency must be measured at the HTTP client with `curl`/`wrk` or by comparing an injected client timestamp with `received_at`.
