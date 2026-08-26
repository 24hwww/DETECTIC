# DETECTIC — EX520 RF Distance Estimation Investigation

> **Status:** Evidence-based investigation and implementation  
> **Date:** 2026-08-26  
> **Target:** TP-Link EX520V (MediaTek MT7981B) stock firmware  
> **Objective:** Determine what RF metrics the stock EX520 firmware/API really exposes, why timing-based distance is not possible, and how Detectic can safely estimate proximity from RSSI/RCPI with explicit uncertainty.

---

## 1. Executive Summary

The TP-Link EX520V running stock firmware `EX520V124101568249n_agc3000_0945460481` does **not** expose:

- FTM / 802.11mc / 802.11az Fine Timing Measurement
- ToF / round-trip RF propagation time
- PHY-level timestamps
- Per-packet RSSI
- True SNR
- Unassociated client discovery

The only realistic distance signal is the **RCPI** value reported for associated clients in the GTPR `DEV2_WIFI_APDEV_ASSOCDEV` table and via the MediaTek HAL `getAssociateStaList` / `getStaLinkMetrics` private ioctls.

The implementation added to the repo treats RCPI as the primary observation, converts it to an **uncalibrated, vendor-approximate dBm** only when needed, applies **EMA + median filtering**, estimates distance with a **log-distance path-loss model**, and always reports **confidence and calibration status**. No FTM is faked, no PHY timestamps are emulated, and no universal "accurate distance" is claimed.

---

## 2. Hardware/API Findings

### 2.1 GTPR web API

Live captures of `http://[fe80::3e6a:d2ff:fe5f:abc1%enp2s0]/` show the EX520 exposes associated station metrics through the `DEV2_WIFI_APDEV_ASSOCDEV` OID (`getList`):

| Field | Meaning | Relevance |
|-------|---------|-----------|
| `MACAddress` | Station MAC | Primary identity (pseudonymized in Detectic) |
| `X_TP_HostName` | Hostname | Enrichment |
| `X_TP_IPAddress` | IPv4 | Enrichment |
| `operatingStandard` | `n`, `ac`, `ax` | Band/standard inference |
| `signalStrength` | 0–127, MediaTek RCPI | **Primary distance signal** |
| `noise` | Vendor-scale noise floor | Not a validated dBm |
| `lastDataDownlinkRate` / `lastDataUplinkRate` | Link rates (kbps) | Indirect quality |
| `X_TP_RadioMac` / `X_TP_BssMac` | AP radio MAC | Band selection (`:C1` = 2.4 GHz, `:C3` = 5 GHz) |
| `associationTime` | ISO 8601 | Wall-clock association timestamp |

Source: `investigations/ex520v_api_findings.md`, section 4.1.

### 2.2 MediaTek HAL (`libplatform_api.so`)

Static analysis of `_rootfs/lib/libplatform_api.so` confirms the vendor uses private ioctls (`ioctl 0x8be1`) and exposes these read-only helpers:

| Function | OID / Command | Output |
|----------|---------------|--------|
| `hal_multiap_mtk_getAssociateStaList` | `0x0a01` | ≤128 stations × 640 B: MAC, traffic, link metrics, RSSI/RCPI, assoc frame |
| `hal_multiap_mtk_getUnassocStaLinkMetrics` | `0x0a03` | 24 B for a **known** MAC: channel, RSSI, timestamp |
| `hal_multiap_mtk_getScanResult` | `0x0b04` | Neighbour APs (SSID, BSSID, channel, RSSI) |
| `hal_multiap_mtk_getRssi` | `0x0b05` | Interface-level int32 RSSI |
| `hal_multiap_mtk_getRadioMetrics` | `0x0a05` | 10 B: BSSID, noise, counters |
| `hal_multiap_mtk_getStaLinkMetrics` | (internal) | Per-station link metrics including RSSI |

Notably, the library exports `rssi_to_rcpi` and `radio_noise_check`, confirming that the native metric is **RCPI**, not dBm.

Source: `investigations/libplatform_api/ANALYSIS.txt`, sections 2, 7 and 8.

### 2.3 Full Wi-Fi capability inventory

`investigations/m4_4_wifi_capability.md` documents what is available and explicitly **not** available on the stock EX520:

| Capability | Status |
|------------|--------|
| Associated station metrics (MAC, RSSI, rates, standard) | ✅ GTPR / HAL |
| Per-antenna RSSI | ✅ `iwpriv stat` |
| Noise floor (vendor scale) | ✅ GTPR / HAL |
| Site survey (neighbour APs) | ✅ `iwpriv` |
| Unassociated station metrics | ❌ Not accessible |
| Channel utilization | ❌ Not exposed |
| Connection/disconnection events | ❌ Not exposed via API |
| Probe request data | ❌ Not exposed |
| Per-packet RSSI | ❌ Not exposed |

### 2.4 Search for FTM/802.11mc

A repository-wide grep for `\bFTM\b`, `802\.11mc`, `fine timing`, `ToF`, `ranging`, and `PHY timestamp` returned **zero matches** outside of this document and the existing distance-estimation design notes. No HAL symbol, no `iwpriv` command, no GTPR OID, and no firmware string references timing-based ranging.

---

## 3. RCPI Semantics

The EX520 `signalStrength` field is MediaTek **RCPI** (Received Channel Power Indicator), not dBm.

- **Range:** 0–127 (theoretical). Live observations cluster around 100–110.
- **Direction:** Higher RCPI = stronger received signal.
- **Units:** Vendor-specific, chip-dependent.
- **Source confidence:** Confirmed by:
  - `rssi_to_rcpi` symbol in `libplatform_api.so`
  - `radio_noise_check` in `libplatform_api.so`
  - The cross-reference table in `investigations/rssi_semantics.md`, section 6.

This means `signalStrength` cannot be directly subtracted from a noise value to obtain SNR, and it cannot be compared to a known dBm value without calibration.

---

## 4. RSSI Conversion

The conversion from RCPI to estimated dBm is **inferred**, not documented by MediaTek. The repo has adopted the linear mapping proposed in `investigations/rssi_semantics.md`:

```text
RSSI(dBm) ≈ -110 + (RCPI / 127) × 30
```

Examples:

| RCPI | Estimated dBm |
|------|---------------|
| 0 | -110.0 |
| 50 | -98.2 |
| 80 | -91.1 |
| 100 | -86.4 |
| 104 | -85.5 |
| 110 | -84.0 |
| 127 | -80.0 |

The conversion is implemented in `src/calibrate.rs` as `rcpi_to_dbm(rcpi: i64) -> Option<f64>`. It rejects values outside 0–127 and the sentinel `-100`, returning `None` instead of a meaningless dBm value.

A unit test confirms the boundary points and the conversion of a typical associated value:

```rust
assert!((rcpi_to_dbm(0).unwrap() + 110.0).abs() < 0.01);
assert!((rcpi_to_dbm(127).unwrap() + 80.0).abs() < 0.01);
```

---

## 5. Noise Semantics

The GTPR `noise` field is also a vendor scale. The HAL `radio_noise_check` operates on it, but the repo has **not validated** a conversion to dBm.

Consequently:

- `noise_to_dbm(_)` in `src/calibrate.rs` deliberately returns `None`.
- Detectic does **not** report a true SNR.
- Any future SNR estimate must be explicitly labelled as approximate and only produced after the noise scale is calibrated.

---

## 6. SNR Limitations

True SNR = signal power / noise power, both in the same dBm-compatible units. Because:

1. The signal is RCPI, not dBm.
2. The noise scale has not been dBm-validated.
3. The two values may not share the same scale or offset.

Subtracting `noise` from `signalStrength` to get SNR is **not defensible** without per-sensor calibration. Therefore the current data model sets `estimated_snr_db` to `null`.

---

## 7. Timestamp Analysis

The EX520 exposes several timestamps; none of them is a PHY or RF propagation timestamp.

| Timestamp | Source | Semantics |
|-----------|--------|-----------|
| `associationTime` | GTPR `DEV2_WIFI_APDEV_ASSOCDEV` | ISO 8601 wall-clock time of association |
| HAL `getUnassocStaLinkMetrics` timestamp | `clock_gettime(CLOCK_REALTIME)` + `sysinfo()` | Wall-clock + uptime |
| Netlink `RTM_NEWLINK` events | Kernel link state changes | Event-driven, millisecond-resolution |

All of these include HTTP request latency, kernel scheduling, driver queueing, and firmware processing. None can resolve nanosecond-level RF propagation time.

---

## 8. FTM / 802.11mc Investigation

A fine-timing system requires at least one of:

- FTM requester/responder 802.11mc capability exposed by the driver
- ToF counters from the PHY
- Ranging OIDs / netlink attributes
- `NL80211_CMD_GET_MEASUREMENT` or similar

None of these are present in the stock EX520 firmware, HAL symbols, `iwpriv` strings, GTPR OIDs, or repository findings. A grep for the exact strings `FTM`, `802.11mc`, and `fine timing` returned no matches in the codebase.

**Conclusion:** FTM / 802.11mc ranging is **not usable** on this stock firmware without a different firmware image and likely different driver support.

---

## 9. Why ToF Is Unavailable

RF time-of-flight is impractical for several independent reasons:

1. **Physical scale:**  
   `1 m ≈ 3.34 ns`, `10 m ≈ 33.4 ns`, `100 m ≈ 334 ns`.

2. **Latency sources dominate:**
   - HTTP request/response over IPv6 link-local: milliseconds
   - GTPR JavaScript/AES/RSA processing: milliseconds
   - Linux network and driver queues: microseconds to milliseconds
   - Host Python/Rust client scheduling: milliseconds
   - 802.11 retransmissions and DCF backoff: unpredictable

3. **No PHY timestamping:** The MediaTek HAL does not expose `tx_time`/`rx_time` per-frame timestamps in the observed symbols.

4. **No way to correlate local and remote clocks at nanosecond precision.**

Therefore `associationTime`, wall-clock timestamps, and netlink events are **not interchangeable** with RF propagation time. Detectic cannot derive ToF from the available API.

---

## 10. RSSI Distance Model

The only defensible distance estimator is the log-distance path-loss model:

```text
PL(d) = PL_0 + 10·n·log10(d / d_0)
```

Solving for distance:

```text
d = d_0 × 10^((RSSI_0 - RSSI) / (10·n))
```

Where:

- `RSSI` is the received signal strength in dBm.
- `RSSI_0` is the calibrated signal at the reference distance `d_0`.
- `n` is the path-loss exponent (environment-dependent).

The implementation in `src/calibrate.rs` provides:

```rust
pub fn log_distance_m(rssi_dbm: f64, rssi0_dbm: f64, n: f64, d0_m: f64) -> f64
```

Example tests:

```rust
let d = log_distance_m(-80.0, -60.0, 2.0, 1.0); // 10 m
let d = log_distance_m(-100.0, -60.0, 4.0, 1.0); // 10 m
```

These pass and confirm the mathematical correctness of the model.

---

## 11. Calibration Methodology

A `Calibrator` collects samples at known distances and fits `RSSI_0` and `n` per band:

1. Place a reference device at known distances (1 m, 2 m, 3 m, 5 m, 10 m, 15 m, 20 m).
2. Record at least 20 samples per distance.
3. Store `timestamp`, `sensor_id`, `client_id` (pseudonym), `band`, `channel`, `RCPI`, `rssi_dbm`, `noise`, `operating_standard`, `uplink_rate`, `downlink_rate`, `association_state`.
4. Convert mean RCPI per distance to dBm.
5. Fit `n` from pairs of (distance, dBm) using least-squares over the log-distance equation.
6. Produce a `DistanceProfile` with `d0_m`, `rssi0_dbm`, `n`, and `calibrated: true`.

Without calibration data, a default uncalibrated profile is used with `calibrated: false` and a confidence cap of 0.5.

---

## 12. Filtering Methodology

RSSI is noisy due to multipath, body movement, antenna orientation, and interference. The chosen filter is a **two-stage EMA + median**:

1. **EMA:** `rssi_ema[t] = α · rssi_current + (1 - α) · rssi_ema[t-1]`  
   Default `α = 0.2`.
2. **Median window:** Maintain the last 5 EMA values and report the median.  
   This rejects single-sample spikes (e.g., microwave oven interference).

Kalman filtering was rejected because it adds matrix state, tuning complexity, and CPU/memory overhead for marginal accuracy gains on uncalibrated RCPI values.

Source: `investigations/distance_estimation.md`, section 9.

---

## 13. Uncertainty / Confidence Model

Distance is always reported with confidence and a bucket, never as a single point value.

The confidence heuristic in `src/calibrate.rs` (`confidence_score`) is:

- Hard-capped at **0.5** if the profile is uncalibrated.
- Reduced for very weak signals (near the noise floor).
- Reduced for very strong / saturated signals where the log-distance model becomes insensitive.
- Increased with more samples, up to a calibrated maximum of 0.95.

Output structure (`model::DistanceEstimate`):

```json
{
  "bucket": "NEAR",
  "estimated_distance_m": 5.2,
  "rssi_dbm": -70.0,
  "confidence": 0.43,
  "calibrated": false,
  "band_mhz": 2400
}
```

If the RCPI is invalid, the output is:

```json
{
  "bucket": "UNKNOWN",
  "estimated_distance_m": null,
  "rssi_dbm": null,
  "confidence": 0.0,
  "calibrated": false
}
```

---

## 14. `-100` Handling

The value `-100` is a common sentinel for missing or invalid RSSI in some Wi-Fi toolchains. It is **outside** the valid RCPI range (0–127) and therefore cannot represent a real RCPI sample.

`classify_rcpi(-100)` returns `RssiQuality::Sentinel` and `rcpi_to_dbm(-100)` returns `None`. The estimator produces `bucket: UNKNOWN`, `estimated_distance_m: null`, `rssi_dbm: null`, and `confidence: 0.0`.

Unit test:

```rust
assert_eq!(rcpi_to_dbm(-100), None);
assert_eq!(rcpi_to_dbm(-1), None);
assert_eq!(rcpi_to_dbm(128), None);
```

---

## 15. Multi-Sensor Positioning Design

Future Detectic deployments may use multiple EX520 sensors. The proposed architecture is:

```text
RSSI/RCPI per sensor
        ↓
Distance estimate + uncertainty per sensor
        ↓
Trilateration / least-squares
        ↓
Probable position with uncertainty ellipse
```

Because each distance estimate is already uncertain, trilateration must treat each distance as a probability distribution rather than a circle. The position output should be a **region of probability**, not a single point.

Important: inaccurate RSSI distances produce large, overlapping uncertainty regions. Three sensors do **not** automatically provide GPS-like accuracy. The data model is designed so multi-sensor positioning can be added later without changing the per-sensor RCPI-first design.

---

## 16. Data Model

Observations should be divided into **observed** and **derived** fields:

```json
{
  "sensor_id": "ex520-001",
  "client_id": "privacy-preserving-id",
  "timestamp": "2026-08-26T00:00:00Z",

  "radio": {
    "band": "2.4GHz",
    "channel": null,
    "operating_standard": "ax"
  },

  "signal": {
    "rcpi": 104,
    "rssi_dbm": -85.5,
    "filtered_rssi_dbm": -85.3,
    "noise": null,
    "estimated_snr_db": null
  },

  "distance": {
    "meters": 2.1,
    "uncertainty_meters": 3.0,
    "confidence": 0.42,
    "method": "rssi_path_loss",
    "calibrated": false
  }
}
```

- `noise` and `estimated_snr_db` are `null` because the EX520 noise scale has not been validated.
- `rssi_dbm` is always a derived value, never stored as truth.
- `method` is always recorded so consumers can distinguish data sources.

---

## 17. Implementation Status

The implementation is in `src/calibrate.rs`:

| Component | Status | Notes |
|-----------|--------|-------|
| RCPI validation and classification | ✅ | `RssiQuality`, `classify_rcpi` |
| RCPI → dBm conversion | ✅ | `rcpi_to_dbm`, approximate, rejects sentinels |
| Noise → dBm conversion | ✅ | `noise_to_dbm` returns `None` intentionally |
| Log-distance model | ✅ | `log_distance_m` with unit tests |
| Calibration profile | ✅ | `DistanceProfile`, `Calibrator::fit` |
| EMA filter | ✅ | `ema` function |
| Moving median | ✅ | `moving_median` function |
| Per-device estimator | ✅ | `LogDistanceEstimator` |
| Distance → bucket mapping | ✅ | `proximity_bucket_from_meters` |
| Confidence heuristic | ✅ | `confidence_score` with calibration cap |

---

## 18. Test Results

```text
$ cargo test --lib

running 190 tests
test result: ok. 189 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
```

Relevant new tests in `calibrate.rs`:

| Test | Purpose |
|------|---------|
| `rcpi_to_dbm_boundaries` | RCPI 0 → -110 dBm, RCPI 127 → -80 dBm |
| `rcpi_invalid_values_rejected` | `-100`, `-1`, `128`, `i64::MAX` all return `None` |
| `noise_to_dbm_returns_none` | Noise is not converted to dBm |
| `log_distance_math` | Path-loss model correctness |
| `ema_smoothing` | EMA correctness |
| `moving_median_rejects_spikes` | Median removes a single outlier once the window is full |
| `confidence_uncalibrated_capped` | Uncalibrated confidence ≤ 0.5 |
| `confidence_calibrated_higher_than_uncalibrated` | Calibration increases confidence |
| `estimate_distance_with_and_without_calibration` | `calibrated: false`, invalid RCPI returns `Unknown` |
| `log_distance_estimator_updates` | `LogDistanceEstimator` feeds and maintains state |
| `calibrator_fit_requires_two_distances` | `Calibrator` only fits when enough data exists |

---

## 19. Known Limitations

| Limitation | Impact |
|------------|--------|
| RCPI → dBm conversion is inferred, not validated | Distance estimates may be off by meters to tens of meters |
| No calibrated noise floor | No SNR, no true signal-to-noise confidence |
| FTM/ToF unavailable | No timing-based ranging possible |
| Unassociated clients not discoverable | Distance only for associated or pre-known MACs |
| Indoor multipath ±5–15 dB | Same device at same distance can show very different RCPI |
| 2.4 GHz vs 5 GHz propagation | Requires per-band calibration profiles |
| Antenna pattern unknown | Orientation changes affect apparent signal strength |

---

## 20. Recommended Next Steps

1. **Field calibration:** Place a reference phone at 1 m, 2 m, 3 m, 5 m, 10 m, 15 m, 20 m from the EX520 on each band and record at least 20 RCPI samples per distance.
2. **Validate noise scale:** Compare GTPR `noise` values against `iwpriv stat` and `/proc/net/wireless` to determine if a reliable dBm conversion exists.
3. **Replace default `rssi0_dbm` / `n` with measured values** from the calibration above.
4. **Add multi-sensor `DistanceEstimate` fusion** in a new `src/fusion.rs` (or extend `src/fusion.rs`) after at least two calibrated sensors are available.
5. **Dashboard surface uncertainty:** Display `bucket`, `estimated_distance_m`, and `confidence` together; never display `estimated_distance_m` alone.
6. **Do not attempt firmware modification or FTM enablement** unless a separate, explicit investigation proves the stock firmware can support it.

---

## Final Output Summary

```text
AUDIT:
  EX520 exposes associated-station RCPI, noise (vendor scale), rates, and
  wall-clock association timestamps. No FTM, SNR, or PHY timestamps exist.

REPRODUCE:
  Grep for FTM/802.11mc returned zero matches. `rssi_to_rcpi` in the HAL
  confirms the native metric is RCPI. `rcpi_to_dbm` unit tests pass.

ROOT CAUSE:
  ToF requires nanosecond timing; GTPR/HTTP latency, kernel scheduling, driver
  queues, and the absence of PHY timestamps make RF ToF impossible on this
  stock firmware.

DESIGN:
  RCPI → approximate dBm → log-distance path-loss with per-band calibration
  profiles, EMA + median filtering, and explicit confidence.

IMPLEMENT:
  Added RCPI validation, dBm conversion, `DistanceProfile`, `Calibrator::fit`,
  `LogDistanceEstimator`, `confidence_score`, and bucket mapping in
  `src/calibrate.rs`.

TEST:
  `cargo test --lib` — 189 passed, 0 failed.

VERIFY:
  All new functions have unit tests; the log-distance math and median filtering
  produce expected values; invalid RCPI values are rejected.

LIMITATIONS:
  RCPI is uncalibrated, noise scale unknown, FTM unavailable, and indoor
  multipath limits absolute accuracy.

NEXT STEP:
  Perform live field calibration for 2.4 GHz and 5 GHz to replace the
  uncalibrated defaults.
```
