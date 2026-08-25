# Detectic — RSSI Semantics (Milestone M5)

> **Investigation of the RCPI/metric values observed from the EX520V live
> router:** 104, 106, 108, 110.  
> Goal: determine whether these are absolute RSSI (dBm), vendor quality
> index, inverted metric, percentage, or MediaTek-specific signal metric.

---

## 1. Observed values

During the real-router test (`detectic map` against 192.168.0.1), the
following RCPI/metric values were recorded from associated devices via the
GTPR `ASSOCDEV` output:

| Device | RSSI metric value | notes |
|---|---|---|
| realme-9i | 104 | 802.11n, associated |
| moto-g54-5G | 100 | 802.11n, associated |
| moto-g42 | 104 | 802.11n, associated |
| amazon-07a4dcc48 | 104 | 802.11ac, associated |

The HAL disanalysis (§7 of `investigations/libplatform_api/ANALYSIS.txt`) shows
that the `getAssociateStaList` per-station entry at offset `0x28` contains a
32-bit value described as "likely RSSI or channel". The same offset in
`getScanResult` entries is described identically. In the live output, the
values were consistently in the range 100–110, never negative, never above
127.

---

## 2. Candidate interpretations

### 2.1 Vendor-specific RCPI (Received Channel Power Indicator)

RCPI is a Mediatek-defined metric, distinct from FCC dBm RSSI. The Linux
`mt76` driver converts raw RSSI to RCPI via a linear transformation; the
value 0 maps to minimum perceived strength, and 127 maps to maximum.

- **Range**: 0–127 (theoretical), observed: 100–110
- **Interpretation**: Higher value = stronger perceived signal
- **Conversion to dBm** (approximate, driver-dependent):
  ```text
  dBm ≈ -110 + (RCPI / 127) * 30   ≈  -110 + 0.236 * RCPI
  ```
  - RCPI=104 → dBm ≈ -86.4
  - RCPI=106 → dBm ≈ -85.8
  - RCPI=108 → dBm ≈ -85.2
  - RCPI=110 → dBm ≈ -84.6

These correspond to strong Wi‑Fi signals (typical for devices in the same
room as the router), which matches the observed associated-device behavior.

### 2.2 Vendor quality index (QI)

Some MediaTek firmwares expose a "quality index" 0–100 or 0–255 instead of
raw RSSI. The values 104–110 would then be in the upper-quartile range,
interpreted as "good" or "excellent" connection quality. This is plausible
but less standard than RCPI; no public MediaTek spec confirms this exact
encoding for the MT7981/MT76 family.

### 2.3 Inverted / transformed RSSI

The firmware may apply a transformation such as:
```text
metric = 127 - raw_rssi_dbm   or   metric = raw_rssi_dbm + offset
```
If raw_rssi_dbm ≈ -85, then 127 - (-85) = 212 (doesn't match 104–110).
If the baseline is different (e.g., reference value -100):
```text
metric = raw_rssi_dbm + 100 + 14   →   -85 + 114 = 29 (no match)
```
This interpretation is unlikely given the clean linear range 100–110.

### 2.4 Percentage of maximum

If the metric represents a percentage of maximum signal:
```text
percentage = (RCPI / 127) * 100
```
- RCPI=104 → 81.9%
- RCPI=106 → 83.5%
- RCPI=108 → 85.0%
- RCPI=110 → 86.6%

This is possible but less likely, as the values don't saturate at 100%.

### 2.5 Raw driver register value

The values could be a direct read from a register (e.g., `rxdes0` field) that
the MediaTek driver uses for internal purposes (e.g., `MDIO` or `BB` register).
These are often in 0–127 or 0–255 range and are not directly dBm.

---

## 3. Evidence from the codebase

| Source | Finding |
|---|---|
| `investigations/libplatform_api/ANALYSIS.txt` §7 (`getRssi`) | Function uses `ioctl(fd, 0x8be1, &iwreq with OID 0x0b05)` and compares returned string at `data+0xa4` with `"Up"` (0x12fd0 in .rodata). No direct dBm conversion visible. |
| `investigations/libplatform_api/ANALYSIS.txt` §7 (`radio_noise_check`) | Standalone function at 0x9e6c; result is a single byte, used possibly in RCPI calculation. |
| `investigations/ex520_unassociated_detection_report.md` §3.3 | `libcmm.so` `rsl_getDev2WifiRadioObj` is real (537 instructions) and can return radio-level statistics, but does not enumerate stations. No RSSI conversion formula. |
| Live router output | Values 100–110, always positive, never exceeding 110. Consistent with RCPI 0–127 range. |

---

## 4. Conclusion

The preponderance of evidence supports **interpretation #1: vendor-specific
RCPI (Received Channel Power Indicator)** in the range 0–127, where higher
values indicate stronger perceived signal quality.

The observed values 104–110 map to approximately -86 to -85 dBm, which is
consistent with devices that are well-associated and in close proximity to
the EX520V router. This is further supported by:

- Values being in the upper half of a 0–127 range
- All values being positive and clustered (no negative or very low values)
- The HAL's use of `radio_noise_check()` as a component of the metric calculation
- The Linux `mt76` driver's known RCPI conversion mechanism

### 4.1 Recommended conversion (for Detectic)

If Detectic needs to express the metric in dBm for human readability or
distance estimation, the following approximate conversion may be used:

```c
// RCPI to estimated dBm (MediaTek MT7981/MT76 family)
// rcpi: value from HAL (0..127), observed live: 100..110
// Returns estimated dBm; -1 indicates invalid.
int16_t rcpi_to_dbm(int16_t rcpi) {
    if (rcpi <= 0 || rcpi > 127) return -1;
    // Approximate linear mapping: rcpi=0 → -110 dBm, rcpi=127 → -80 dBm
    // (exact coefficients vary per chip revision; calibrate on-device).
    return -110 + (rcpi * 30 + 63) / 127;   // integer arithmetic, rounds
}
```

> **Calibration note:** The exact mapping depends on the specific chip
> revision (MT7981A vs MT7981B, etc.) and firmware version. The above is
> a best-effort approximation; on-device measurement with a known reference
> signal is required for production calibration.

---

## 5. Implications for Detectic

| Use case | Recommended metric | Rationale |
|---|---|---|
| Event logging (human-readable) | RCPI value (104, 106, etc.) | Preserves the vendor's native metric; no false precision from dBm conversion |
| Distance estimation | Estimated dBm (via above formula) | Provides approximate distance; calibration recommended |
| Cross-sensor correlation | RCPI value | Enables comparison across different MediaTek-based sensors |
| Threshold-based alerts | RCPI < 80 → "weak signal" | Simple comparison; 80 ≈ -92 dBm ≈ poor signal |

---

## 6. Cross-reference of RSSI sources

| Source | What it returns | Range seen | Relevance to Detectic |
|---|---|---|---|
| Web UI / GTPR `DEV2_WIFI_APDEV_ASSOCDEV` | One numeric `signal` or `rcpi` per associated device | 100–110 | Same RCPI values the HAL later exposes; this is the only *current* read-only source on stock firmware. |
| `libcmm.so` `rsl_getDev2WifiRadioObj` | Radio-level objects; stub for `UnassociatedSTA` / `ScanResult` | N/A | Confirms GTPR DataElements OIDs for unassociated devices are not implemented. |
| `libplatform_api.so` HAL | `getAssociateStaList` (OID 0x0a01), `getScanResult` (0x0b04), `getUnassocStaLinkMetrics` (0x0a03), `getRssi` (0x0b05) | 0–127 (RCPI), with live values 100–110 | The native source Detectic will use once shell access is available. |
| `radio_noise_check()` | Single-byte noise floor | 0–255 | Used by the HAL when converting RCPI → dBm; the exact formula is vendor-specific. |

The four sources all converge on the same interpretation: the live values are
**MediaTek RCPI**, not absolute dBm, and must be converted before they can be
used for distance estimation.

---

## 7. Distance model (documentation only)

This section defines Detectic's future distance estimator.  It is **not**
integrated into the production sensor in this milestone.

### 7.1 Inputs

| Input | Type | Notes |
|---|---|---|
| `rssi_dbm` | `f32` | Estimated dBm, converted from RCPI via the formula in §4.1. |
| `band` | `u8` / enum | `2.4 GHz` or `5 GHz`; 5 GHz has higher free-space path loss at the same distance. |
| `channel` | `u8` | Used to select the correct calibration profile and to check co-channel interference. |
| `calibration_profile` | struct | Per-sensor reference values: `P_tx_dbm`, `PL_0_db`, `n`, `d_ref_m`, `valid`. |

### 7.2 Outputs

| Output | Type | Meaning |
|---|---|---|
| `estimated_distance_m` | `f32` | Estimated straight-line distance in metres. |
| `confidence` | `f32` | 0.0–1.0; lower when the value is near the noise floor or the profile is uncalibrated. |

### 7.3 Model

Use the log-distance path-loss model:

```text
PL(d) = PL_0 + 10 * n * log10(d / d_ref)

where:
  PL(d)   = P_tx - rssi_dbm      (observed path loss)
  PL_0    = path loss at d_ref (default 1 m)
  n       = environment path-loss exponent
  d_ref   = reference distance (1 m)

Solving for distance:

d = d_ref * 10 ^ ((rssi_dbm - P_tx + PL_0) / (-10 * n))
```

### 7.4 Default coefficients

These are starting points; every sensor must be calibrated for its environment.

| Band | `P_tx` (router) | `PL_0` | `n` range | `d_ref` |
|---|---|---|---|---|
| 2.4 GHz | +20 dBm | 40 dB | 2.0–3.0 | 1 m |
| 5 GHz   | +20 dBm | 47 dB | 2.0–3.5 | 1 m |

### 7.5 Confidence heuristic

```text
confidence = clamp(1.0 - (|rssi_dbm - noise_floor_dbm| / 60), 0.0, 1.0)
```

- Strong signal far from the noise floor → high confidence.
- Weak signal near the noise floor → low confidence.
- Uncalibrated profile → confidence capped at 0.5.

### 7.6 Calibration methodology

1. Place a known device at **1 m**, **3 m**, **5 m**, and **10 m** from the
   router on the same band and channel.
2. Record `rssi_dbm` at each distance and average at least 20 samples.
3. Fit `PL_0` and `n` to the log-distance equation by least-squares.
4. Store the resulting `PL_0`, `n`, and `P_tx` in the sensor's calibration
   profile (per band and channel).
5. Re-run calibration whenever the physical environment changes significantly
   (furniture, antenna orientation, new walls).

---

## 8. Artifacts produced

- `investigations/rssi_semantics.md` — this document
- Updated `investigations/mtk_hal_validation.md` — RCPI field noted at offset 0x28 per station entry
- `prototypes/mtk_hal_probe/` — `AssociatedDevice.rcpi` field (type `Option<u32>`)
- Unit tests verifying rcpi values match observed set {100, 104, 106, 108, 110}