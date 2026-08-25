# M11-H — Security & Privacy Posture (stock TP-Link EX520V)

**Status:** Off-router design + read-only/remote inference. No router runtime
install was performed (per no-reboot / no-persistence rules). Live resource
validation is deferred to read-only GTPR queries and static analysis.

## Attack surface added by Detectic (external, not on router)
Detectic runs as an **external client** that only *reads* the GTPR/GDPR API. It
does not open listeners on the router, does not modify router config, and does
not install binaries on the router. Surface added:
- One outbound HTTPS (or HTTP-in-LAN) session from the operator machine to the
  router's GTPR endpoint, authenticated with the router admin/user credentials.
- Local pseudonymized event log on the operator machine only.

## Credential handling
- Router credentials are supplied via env (`DETECTIC_PASSWORD`, `DETECTIC_USER`)
  or CLI flags; never hardcoded (AGENTS.md §21, §40).
- The per-sensor HMAC secret (`DETECTIC_SECRET`) is used only for local
  pseudonymization (`crypto::pseudonymize`). It is **never** transmitted to the
  router and must never be committed.
- GTPR shared secret is a router-side config value; the Rust binary does not need
  it for read-only polling.

## Privacy guarantees
- Raw MAC addresses are **never** emitted by the pipeline. `realtime` pseudonymizes
  each identity before emitting an event (`crate::pseudonymize`).
- The `publisher` layer further ensures upload payloads contain only pseudonyms
  (verified in `publisher::tests::upload_payload_is_stable_and_identifiably_pseudonymized`).
- No raw traffic capture, no packet storage (AGENTS.md §39).

## Router-side safety (authorization enforced this session)
- No service installed, no port opened, no init modified.
- Telnet (`DEV2_TELNET_CFG.telnetLocalEnabled=1`) was kept **LAN-only** as a local
  diagnostic path during recovery; it is a pre-existing debug feature, not
  something Detectic added, and should be disabled when no longer needed.
- The IPv4 management outage (`m11_recovery_incident.md`) is pre-existing and was
  **not** caused by Detectic; recovery actions only *reverted* a prior agent
  enable flag and preserved evidence.

## Hardening checklist (for production later)
- [ ] TLS to the GTPR endpoint (avoid plaintext HTTP in untrusted L2).
- [ ] Secret sourced from a vault, not env in shared shells.
- [ ] Rotate router admin credentials after any diagnostic use.
- [ ] Disable Telnet on the router once diagnostics are complete.
- [ ] Bound local event log size; never let telemetry fill disk.
- [ ] Sign/verify any future router-side binary before install (AGENTS.md §41).
