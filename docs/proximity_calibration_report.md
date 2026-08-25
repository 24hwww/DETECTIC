# Proximity Calibration Report

## Radio → Band Mapping

Live EX520V data:
- Radio MAC 3C:6A:D2:5F:AB:C1 → associated stations operatingStandard = n
- Radio MAC 3C:6A:D2:5F:AB:C3 → associated stations operatingStandard = ac / n

Heuristic used for MVP:
- operatingStandard ∈ {ac, ax} → Band::Ghz5
- operatingStandard ∈ {n, g, b} → Band::Ghz2_4
- Radio MAC alone insufficient without config; band derived per-station from operatingStandard.

No hardcoded MAC addresses.

Signal scale: native 0-127 RCPI-like signalStrength. No proven dBm mapping used.

## Calibration Tooling

src/calibrate.rs implements:
- CalibrationSample
- CalibrationSession
- Calibrator with record/summary

CLI scaffold prepared. Full CLI subcommand requires additional wiring to existing Clap tree; current scaffold supports programmatic use.

## Live Validation

EX520 remains read-only. No physical distance measurements performed in this session. Tooling is ready for controlled known-distance experiment.

Current observed stations: 9 associated devices, signalStrength 68-114, radio MACs C1/C3.

## Model

Initial model: monotonic native signalStrength → proximity_score per band. Thresholds to be derived empirically from calibration samples.

## Security

No raw MAC in calibration output. Pseudonymization via HMAC-SHA256. Credentials redacted.

## Limitations

No known-distance samples collected yet. Band mapping via operatingStandard is heuristic. Absolute distance not claimed.

Next: execute controlled known-distance experiment with associated test device, collect samples per distance, derive band-specific thresholds, validate monotonicity and movement.
