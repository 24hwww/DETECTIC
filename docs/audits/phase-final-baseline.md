# Detectic — Phase Final Baseline Audit

**Date:** 2026-08-25
**Commit:** cd36945db26afc1bcbc3f83dfdabe7f8462b0784
**Binary SHA256:** 616c3200578dd89ef62e857630006de57c0dca5d8c07b2a6cd8b9de097f95c08

## Capability Classification

| Capability | Status | Evidence |
|---|---|---|
| EX520 IPv6 connectivity | PROVEN-LIVE | ping6 0.6ms, neighbor entry matches MAC |
| EX520 IPv4 connectivity | PROVEN-LIVE | 192.168.0.1:80 HTTP 200 |
| GTPR/GDPR login | PROVEN-LIVE | JSESSIONID obtained, TokenID fetched |
| DEV2_WIFI_APDEV_ASSOCDEV | PROVEN-LIVE | 3-4 real devices observed with RSSI/noise/band |
| Rust ARM64 sensor execution | PROVEN-LIVE | sensor_log.txt: poll_success stations=4 |
| Sensor as root | PROVEN-LIVE | phoenix.sh runs as root |
| RSSI/RCPI capture | PROVEN-LIVE | signalStrength 102/128/106 captured |
| Noise capture | PROVEN-LIVE | noise=50 in all observations |
| Band info | PROVEN-LIVE | 2.4GHz/5GHz derived from RadioMac |
| Local SQLite persistence | PROVEN-LIVE | 7 captures, 21 observations in collector.db |
| Router JSONL spool | PROVEN-FROM-SOURCE | append_bounded implemented, path configured |
| Email reports | PROVEN-LIVE | DELIVERED in every run |
| Watchdog cold-boot recovery | PROVEN-LIVE | cold boots detected+re-triggered today |
| Site survey (iwpriv) | PROVEN-FROM-SOURCE | code in monitor.rs, DISABLED in config |
| Radio stats (iwpriv stat) | BLOCKED | struct exists, no implementation |
| Calibration | PROVEN-OFFLINE | calibrate.rs exists, not integrated |
| LED control | UNKNOWN | OIDs found in firmware, not tested |
| Backend upload (Rust→Worker) | BLOCKED | HTTP 401 — HMAC key mismatch |
| Backend upload (Python→Worker) | BLOCKED | HTTP 401 — HMAC key mismatch |
| D1 data | PROVEN-OFFLINE | 6 captures with rssi=null (mapping bug) |
| D1 API /devices | BLOCKED | returns [] despite data |
| D1 API /presence | BLOCKED | returns [] despite data |
| D1 API /stats | PARTIAL | distinct_devices=0 but sensors show 4 |

## Configuration Identifiers (no secrets)

| Location | sensor_id | URL | Secret format |
|---|---|---|---|
| .env (host) | home-001 | http://192.168.0.1 | 64-char hex |
| detectic.env (router) | ex520-001 | http://192.168.0.1 | 32-char string |
| sensors.json (local backend) | ex520-001 | n/a | 17-char string |
| Worker DETECTIC_SENSORS | unknown | n/a | unknown |
| collector.py default | detectic-ex520-live | env | bytes.fromhex() |
| Rust sensor default | home-001 | env | UTF-8 string |

## Test State

- cargo test: 139 passed, **1 failed** (runtime::tests::build_backend_returns_local_spool_when_url_set)
- Python tests: not run in this audit
- Worker tests: not run in this audit

## Firmware LED Evidence (read-only)

- `/proc/tp_led` exists in rootfs; format: `echo <NAME> <mode> <state> > /proc/tp_led`
- Known LEDs: POWR, WL2G, WL5G, WPS, INET, LAN
- Modes: 1=OFF, 2=ON, 3=BLINK (per AGENTS.md)
- OIDs: `DEV2_LED_LOCATE`, `DEV2_LED_LOCATE_APINFO`, `DEV2_LED_SCHEDULE_CFG`, `DEV2_XTP_GPIO`, `DEV2_XTP_GPIO_BTN`
- `INCLUDE_EASYMESH_LED_LOCATE=1` in firmware config
- `tp_gpio.ko` kernel module present (DO NOT install/modify)
- `leds-gpio.ko` standard kernel module present

## Critical Blockers (priority order)

1. HMAC key derivation mismatch (collector hex-decode vs Worker UTF-8)
2. Multiple incompatible secrets across components
3. Multiple sensor_ids across components
4. Python GTPR parser doesn't handle `{"data":[...]}` format
5. D1/API queries return empty despite data
6. Signal/band mapping lost in Worker→D1
7. Rust test failure (SpoolBackend::name)
8. RadioStats not implemented
9. Calibration not integrated
10. Site survey disabled
11. Watchdog duplicate triggers
