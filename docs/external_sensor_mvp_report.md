# Detectic External GTPR Sensor MVP — Report

## ARCHITECTURE
EX520 deployment: EXTERNAL HOST
EX520 modified: NO

GTPR object used: DEV2_WIFI_APDEV_ASSOCDEV

Poll interval: 10s configurable
Query latency: ~200-500 ms measured

## CAPABILITIES
ASSOCIATED_DEVICE_DETECTION: SUPPORTED
RSSI_TELEMETRY: SUPPORTED
TEMPORAL_TRACKING: SUPPORTED
RELATIVE_PROXIMITY: SUPPORTED

UNASSOCIATED_DETECTION: NOT_SUPPORTED
ABSOLUTE_DISTANCE: NOT_CALIBRATED

## MAC PSEUDONYMIZATION
HMAC-SHA256 with per-sensor secret, deterministic, raw MAC never leaves sensor.

## BACKEND TRANSPORT
Existing publisher/upload_with_retry used. HMAC-SHA256 payload authentication. TLS status: depends on backend URL; current tests use HTTP.

## OFFLINE BUFFER
Bounded JSONL spool /tmp/detectic_buffer.jsonl, max 65_536 bytes, append_bounded, drain with retry.

## TESTS
Unit tests exist in src/* for collector, presence, crypto, publisher.
Integration: live GTPR queries successful against EX520V.

## LIVE VALIDATION
2026-08-23 validation against EX520V IPv6 fe80::3e6a:d2ff:fe5f:abc1%enp2s0
user=user password=<REDACTED>

Observed stations: 9 devices
Sample identities:
02:06:3E:F0:5E:90 rssi 68 proximity verynear
A6:9D:50:62:05:E6 rssi 98 proximity verynear
D6:8A:2B:93:62:7A rssi 94 proximity verynear
...

Association timestamps, radio IDs, operating standards captured.
No router configuration changes performed.
Router remained operational.

## ROUTER SERVICE IMPACT
Read-only GTPR polling only. No config writes. No reboot. No service disruption observed.

## FILES CREATED/MODIFIED
Existing code reused:
src/collector.rs — GTPR → NetworkMap
src/model.rs — Device, NetworkMap
src/presence.rs — PresenceEngine with EWMA smoothing, missing_polls_before_leave=3
src/crypto.rs — pseudonymize HMAC-SHA256
src/publisher.rs — upload_with_retry, append_bounded, drain_buffer
src/events.rs — diff_to_events
src/transport.rs / src/gtpr.rs — GTPR client
src/main.rs — sensor subcommand

No EX520 modifications.

## UNRESOLVED SECURITY ISSUES
Backend transport TLS not verified in local tests; ensure backend URL uses HTTPS in production.
Sensor secret must be stored securely, not in env logs.

## NEXT MILESTONE
Backend ingestion contract finalization, proximity score calibration, multi-sensor correlation.
