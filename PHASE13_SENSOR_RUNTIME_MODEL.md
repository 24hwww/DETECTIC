# PHASE13_SENSOR_RUNTIME_MODEL.md

## Detectic runtime behavior

RF acquisition → signal processing → feature extraction → presence estimation → compact event → remote server

## Dependencies
- Network access to backend
- WiFi interface access
- Minimal config

## Failure behavior
Backend loss → sensor can continue processing, buffering optional
No persistent local DB required

Classification:
SIMULATED based on Detectic design
UNKNOWN live behavior without hardware test

Local buffering optional, not required for core function.
