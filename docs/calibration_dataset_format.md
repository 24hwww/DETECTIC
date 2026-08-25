# Calibration Dataset Format

## Signal Scale

EX520V reports MediaTek RCPI via `signalStrength` 0..127. Native scale is primary.

No documented dBm mapping confirmed. Empirical conversion approx dBm ≈ -110 + 0.236*RCPI, but keep native scale for calibration.

## Band Detection

Radio identification via `X_TP_RadioMac`. Band mapping derived from radio config; placeholder `Band::Unknown` in MVP.

## Data Model

CalibrationSample:
- sample_id
- collected_at
- device_id: pseudonymized HMAC-SHA256
- band: 2.4GHz /5GHz /unknown
- radio_id
- known_distance_m
- raw_signal_strength
- smoothed_signal_strength
- signal_level
- noise
- signal_delta
- orientation
- environment
- tx_rate / rx_rate
- session_id

CalibrationSession:
- session_id
- started_at
- environment
- device_id
- band
- radio_id
- distance_positions[]

## Collection Procedure

1. Associated test device at known distance.
2. Run `detectic calibrate --distance X --duration 60`
3. Collect ~6 samples at 10s interval.
4. Compute mean/median/stddev per distance.
5. Record orientation and environment.

No router modification. Read-only GTPR.

## Model

Initial model: monotonic mapping native_signal_strength → proximity_score per band.

Proximity_score ∈ [0,1] relative, not meters.

Confidence based on sample count, variance, calibration availability.

## Validation

Hold-out samples, monotonicity check, classification accuracy.

## Security

No raw MAC/IP in dataset. Device_id pseudonymized. Calibration data stays local unless explicitly exported.
