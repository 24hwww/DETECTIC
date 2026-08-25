# Detectic — Distance Estimation Architecture

**Status:** OFF-ROUTER DESIGN DOCUMENT
**Date:** 2026-08-23
**Objective:** Define a realistic, privacy-preserving, router-constrained architecture for estimating approximate device distance from the EX520V using RSSI/RCPI, without claiming false precision.

---

## 1. Key Principle

> **RSSI is not distance.** RSSI is a signal-quality feature. Distance is an inference that depends on environment, hardware, antenna orientation, and transmit power.

The sensor must produce **confidence-bounded proximity buckets**, not a precise meter value.

---

## 2. Signal Metric on the EX520V

The EX520V (MediaTek MT7981B) reports **RCPI** (Received Channel Power Indicator), not absolute dBm. Confirmed via `investigations/rssi_semantics.md`.

| Value | Meaning |
|---|---|
| Range | 0–127 (theoretical), 100–110 (observed on EX520V) |
| Direction | Higher = stronger |
| Conversion | `dBm ≈ -110 + (RCPI / 127) × 30` (approximate, chip-dependent) |

For distance estimation, we first convert RCPI → estimated dBm:

```text
P_rx_dbm = -110 + 0.236 × RCPI
```

Where:
- `P_rx_dbm` = received signal strength in dBm
- `-110` = approximate sensitivity floor at RCPI=0
- `0.236` = 30 dB span / 127 RCPI units

At RCPI=104 → P_rx ≈ -85.7 dBm
At RCPI=110 → P_rx ≈ -84.1 dBm

These correspond to strong in-room signals, consistent with the observed associated devices being in the same room as the router.

---

## 3. Path-Loss Model

### 3.1 Log-Distance Path Loss (primary model)

```text
PL(d) = PL_0 + 10·n·log₁₀(d / d_ref)

Solving for distance:
d = d_ref × 10^((P_rx_dbm - P_tx_dbm + PL_0) / (10·n))
```

Where:
| Parameter | Default (2.4 GHz) | Default (5 GHz) | Notes |
|---|---|---|---|
| `P_tx_dbm` | +20 dBm | +20 dBm | Router TX power (configurable, assumed) |
| `PL_0` | 40 dB | 47 dB | Free-space path loss at 1 m |
| `n` | 2.0 (free space) → 3.0 (indoor) | 2.0 → 3.5 (indoor) | Path-loss exponent |
| `d_ref` | 1 m | 1 m | Reference distance |

Free-space path loss at 1 m:
- 2.4 GHz: `PL_0 = 20·log₁₀(2.4e9 × 4π/c) ≈ 40 dB`
- 5 GHz: `PL_0 = 20·log₁₀(5.18e9 × 4π/c) ≈ 47 dB`

### 3.2 ITU Indoor Attenuation (secondary model)

For indoor environments, the ITU-R P.1238 model adds per-wall/ per-floor attenuation:

```text
P_rx = P_tx - PL_free_space(d) - Σ(L_material)
```

Where typical material losses:
- Wood wall: 3–7 dB
- Brick wall: 10–25 dB
- Concrete wall: 10–30 dB
- Human body: 3–10 dB (frequency-dependent)
- Floor: 12–25 dB

This model is more complex and requires material information (unknown). The log-distance model with a higher `n` (2.5–3.0) effectively absorbs typical indoor attenuation.

---

## 4. Band and Frequency Considerations

| Band | Frequency | Free-space `PL_0` (1 m) | Typical `n` (indoor) | Notes |
|---|---|---|---|---|
| 2.4 GHz | 2.4–2.485 GHz | ~40 dB | 2.0–3.0 | Better wall penetration, longer range |
| 5 GHz | 5.15–5.85 GHz | ~47 dB | 2.0–3.5 | Higher path loss, less penetration |

The EX520V has separate interfaces for each band:
- `rai0` = 2.4 GHz (channel 3)
- `rax0` = 5 GHz (channel 40)

The RSSI reported for each associated device includes `X_TP_RadioMac`, which identifies the radio (band). The distance estimator must use the correct band model.

---

## 5. Temporal Smoothing

RSSI fluctuates rapidly due to:
- Multipath fading (constructive/destructive interference)
- Body movement
- Antenna rotation
- Interference
- Device transmit power changes

### 5.1 Moving Average (EMA)

```text
RSSI_ema[t] = α · RSSI_current + (1 - α) · RSSI_ema[t-1]
```

Where `α = 0.2` (responds to changes over ~5 samples) is a reasonable default.

### 5.2 Median Filter

For removing spikes, a sliding median over N=5 samples is recommended. Median is robust to single-sample outliers (e.g., a momentary spike from a microwave oven).

### 5.3 Recommendation

Use a **combined EMA + median** approach:
1. Apply EMA first to smooth the raw RSSI stream
2. Maintain a window of the 5 most recent EMA values
3. Report the **median** of the window as the representative RSSI

This provides robustness against both single-sample spikes and short-term fluctuations.

---

## 6. Confidence Intervals and Buckets

### 6.1 Distance Buckets (Recommended Output)

Instead of a single distance estimate, produce:

```text
VERY_NEAR   → 0–2 m
NEAR        → 2–7 m
MEDIUM      → 7–20 m
FAR         → 20–50 m
VERY_FAR    → 50+ m
UNKNOWN     → insufficient data
```

These buckets are environment-dependent but provide a usable coarse-grained proximity signal without false precision.

### 6.2 Confidence Heuristic

```text
confidence = clamp(1.0 - (|P_rx_dbm - noise_floor_dbm| / 60), 0.0, 1.0)
```

Where:
- `noise_floor_dbm` = -92 dBm (typical for 2.4 GHz, observed -51 dBm noise in `/proc/net/wireless`)
- Signal near noise floor → low confidence
- Strong signal → high confidence

**Additional confidence penalties:**
- If `P_rx_dbm > -60` → cap confidence at 0.9 (possibly saturated/very close)
- If calibration profile is uncalibrated → cap at 0.5
- If fewer than 3 samples → reduce confidence proportional to sample count

---

## 7. Per-Device Calibration

Each sensor should build a calibration profile per band:

```text
CalibrationProfile {
    band: Band,
    P_tx_dbm: f32,       // measured router TX power
    PL_0: f32,            // measured path loss at 1 m
    n: f32,               // measured path-loss exponent
    noise_floor_dbm: f32,
    calibrated_at: Timestamp,
    valid: bool,
}
```

Calibration method (documented in `rssi_semantics.md` §7.5):
1. Place a reference device at known distances (1m, 3m, 5m, 10m)
2. Record RSSI at each distance
3. Fit `PL_0` and `n` via least-squares
4. Store per-band calibration profile

Without calibration, default coefficients (Table in §3.1) are used with capped confidence.

---

## 8. Antenna Orientation and Multipath

### 8.1 Antenna Orientation

The EX520V is a consumer router with internal antennas. Antenna orientation effects:
- Devices aligned with the antenna pattern can show 5–10 dB stronger RSSI
- Devices perpendicular to the antenna pattern show weaker RSSI
- Unknown antenna pattern on consumer hardware

**Mitigation:** Multiple RSSI samples over time help average out orientation effects.

### 8.2 Multipath

Indoor multipath causes RSSI fluctuations of ±5–15 dB:
- Constructive interference → RSSI spikes up
- Destructive interference → RSSI dips down

**Mitigation:** Median filtering (§5.2) is the primary defense. Kalman filtering is not justified for this application (see §9).

### 8.3 Human/Environmental Attenuation

| Material / Factor | Attenuation at 2.4 GHz | Attenuation at 5 GHz |
|---|---|---|
| Human body (1 person) | 3–10 dB | 5–15 dB |
| Wooden door | 1–3 dB | 2–5 dB |
| Drywall wall | 3–5 dB | 5–10 dB |
| Concrete wall | 10–30 dB | 15–40 dB |
| Metal cabinet/reflection | 20+ dB | 20+ dB |

These effects mean the same device at the same distance can show wildly different RSSI values depending on the environment. This is why **buckets with confidence** are preferred over point estimates.

---

## 9. Filtering Approach: EMA + Median (Not Kalman)

### Why not Kalman filter?

| Criterion | Kalman | EMA + Median |
|---|---|---|
| Memory requirement | High (matrix operations, state vector) | Minimal (scalar EMA + 5-element window) |
| CPU requirement | Moderate (matrix math per sample) | Minimal (add + shift) |
| Flash storage | Needs persistent matrix state | Needs 5 scalars |
| Robustness to spikes | Moderate | High (median rejects outliers) |
| Tuning complexity | High (Q, R, P matrices) | Low (α parameter only) |
| Fit for resource-constrained router | ⚠️ Marginal | ✅ Ideal |

**Conclusion:** EMA + median filtering is the right choice for the EX520V. Kalman filtering would be over-engineering for this use case and is not justified by the accuracy gains on raw RCPI values.

---

## 10. Recommended Distance Estimator Output

### 10.1 Rust data structure

```rust
/// Proximity bucket — coarse-grained, no false precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProximityBucket {
    VeryNear,  // 0–2 m
    Near,      // 2–7 m
    Medium,    // 7–20 m
    Far,       // 20–50 m
    VeryFar,   // 50+ m
    Unknown,   // insufficient data
}

/// Distance estimate with confidence.
#[derive(Debug, Clone)]
pub struct DistanceEstimate {
    /// Coarse proximity bucket.
    pub bucket: ProximityBucket,
    /// Raw estimated distance in meters (for logging only).
    pub estimated_distance_m: Option<f32>,
    /// RSSI in dBm used for this estimate (after smoothing).
    pub rssi_dbm: Option<f32>,
    /// 0.0–1.0 confidence in the estimate.
    pub confidence: f32,
    /// Band this estimate was computed for.
    pub band: Band,
    /// Whether the sensor has been calibrated for this band.
    pub calibrated: bool,
}
```

### 10.2 Estimator trait (off-router, in Detectic core)

```rust
pub trait DistanceEstimator {
    /// Feed an RSSI sample. Returns updated estimate or None if insufficient data.
    fn feed_rssi(&mut self, rssi_dbm: f32, ts: i64) -> Option<&DistanceEstimate>;
    
    /// Current best estimate.
    fn estimate(&self) -> &DistanceEstimate;
    
    /// Reset state (e.g., device moved out of range).
    fn reset(&mut self);
}
```

### 10.3 Per-device state

The estimator must maintain per-device state (smoothed RSSI, sample count). This is lightweight:

```text
Per-device state (per band):
  - EMA value: f32 (4 bytes)
  - Sample count: u32 (4 bytes)
  - RSSI median window: [f32; 5] (20 bytes)
  - Last estimate: DistanceEstimate (~24 bytes)
  - Last sample timestamp: i64 (8 bytes)

Total per device: ~60 bytes
```

For 100 devices × 2 bands: ~12 KB. Well within router memory constraints.

---

## 11. Router-Side Feasibility

### 11.1 Memory

| Component | Memory |
|---|---|
| Per-device state (100 devices × 2 bands) | ~12 KB |
| Median window (5 samples) | ~20 bytes per device-band |
| Calibration profiles (2 bands) | ~24 bytes |
| **Total** | **~13 KB** |

This is negligible compared to the router's RAM (256 MB on MT7981B).

### 11.2 CPU

EMA: 1 multiply, 1 add per sample. Median of 5: sort 5 elements (insertion sort, ~10 comparisons). Both are trivial for the MT7981's dual-core ARM A53.

### 11.3 Flash

Only calibration profiles need persistent storage: 2 × ~24 bytes = 48 bytes.

---

## 12. Privacy Considerations

- **Raw MAC addresses** are pseudonymized before distance estimation (§M11 privacy). The distance estimator works on pseudonyms + RSSI, never seeing raw MACs.
- **Per-device RSSI history** is ephemeral (in-memory only, cleared when the device leaves range). It is not persisted to flash or sent to the backend.
- **Distance estimates** can be safely transmitted to the backend as buckets + confidence, without revealing the underlying RSSI precision or raw identifiers.

---

## 13. Recommendations

1. **Implement distance estimation as a separate module** (`src/distance.rs`) with the `DistanceEstimator` trait and a `LogDistanceEstimator` implementation.
2. **Start with default coefficients** (§3.1) and cap confidence at 0.5 when uncalibrated.
3. **Use EMA + median filtering** (not Kalman) for RSSI smoothing.
4. **Output proximity buckets** (§10.1), not precise distances.
5. **Maintain per-device state** keyed by pseudonym, with automatic cleanup when devices leave range.
6. **Document calibration procedure** clearly for future deployment.
7. **Never claim precise distance** — always report confidence alongside the estimate.

---

## 14. Equations Summary

```text
RCPI → dBm:
  P_rx_dbm = -110 + 0.236 × RCPI

Log-distance path loss:
  PL(d) = PL_0 + 10·n·log₁₀(d / d_ref)

Distance from RSSI:
  d = d_ref × 10^((P_tx_dbm - P_rx_dbm - PL_0) / (10·n))

  = 1 × 10^((20 - P_rx_dbm - PL_0) / (10·n))

EMA smoothing:
  rssi_ema[t] = α · rssi_current + (1 - α) · rssi_ema[t-1]
  (α = 0.2)

Confidence:
  confidence = clamp(1.0 - |P_rx_dbm - noise_floor| / 60, 0.0, 1.0)
  noise_floor = -92 dBm (2.4 GHz), -92 dBm (5 GHz)
  Additional: cap at 0.5 if uncalibrated
```
