# M11-G — Accuracy & Fidelity of the Presence Sensor

**Scope:** What the stock EX520V + Detectic can and cannot reliably report, and
the known accuracy limits. All findings are derived from legitimate, read-only
sources (GTPR API, static firmware analysis). No fabrication of observations.

## What is measured (ground truth)
- **Associated stations only.** Via GTPR `NetworkMap`: MAC, RSSI, IP, hostname,
  active flag, assoc time, radio, rates. This is the router's own association
  table — high fidelity, no guessing.
- **Nearby APs (site survey).** Via `monitor::NearbyObservation` (survey source).
  Lower fidelity: SSID/BSSID/channel/RSSI of surrounding APs.

## What is NOT available (M11-B conclusion)
- **Unassociated STAs (probe requests).** `DEV2_WIFI_DE_UNASSOCSTA` returns
  `errorcode:9003` (stub). `get_mac_table` iwpriv crashes. MediaTek HAL exposes
  no user probe API. Therefore Detectic **cannot** report devices that are near
  but not associated. The `realtime` pipeline accepts `ProbeObservation`s but on
  stock firmware the probe batch is always empty (`ProbeObservation::none()`),
  and the pipeline explicitly ignores empty probes (see `realtime.rs` test
  `empty_probe_is_ignored`).

## Accuracy characteristics
| Signal | Confidence | Notes |
|--------|-----------|-------|
| Device joined/left (associated) | 0.7–0.9 | Driven by GTPR active flag. |
| Device updated (RSSI drift) | 0.8 | Debounced; reflects real RSSI. |
| Nearby AP | 0.6 | Survey RSSI, environmental. |
| Device nearby (probe) | N/A | Not emitted on stock firmware. |

## Deduplication / fidelity guarantees (unit-tested)
- Monotonic, strictly increasing `seq` per event (`realtime::tests`).
- Events returned in `seq` order.
- Identical `(identity, kind)` within the debounce window is suppressed — no
  duplicate event spam.
- Departure is detected by absence in the next poll, not by a forced event.

## Known limitations
- Presence == "associated to this AP", **not** "person present". MAC ≠ person
  (per AGENTS.md §3). Randomized MACs reduce join/leave stability.
- RSSI is a signal feature, not distance (AGENTS.md §32).
- Polling interval bounds temporal resolution; sub-second events are not captured.
- No cross-sensor fusion on the router (that is a backend concern, M8+).
