# EX520V Access Architecture — Historically Validated

## Purpose
Document the historically validated Detectic access mechanism for TP-Link EX520V. SSH is NOT a validated access method for this router. Normal interaction is via GTPR/GDPR HTTP API. Temporary shell access, when explicitly authorized, is via GTPR-controlled Telnet/Lifemote, not SSH.

## Normal access — GTPR/GDPR HTTP API

Primary and historically validated Detectic communication mechanism.

Base URL:
```
http://192.168.0.1
```

Endpoints used by Detectic:
* `/cgi/getGDPRParm`
* `/cgi/login`
* `/cgi/getTokenc`
* `/cgi_gdpr?N`

Authentication / session concepts:
* Router web-user authentication via `DETECTIC_USER` and `DETECTIC_PASSWORD`
* Session established with TokenID / JSESSIONID returned by login flow
* Requests signed/encrypted with AES-128-CBC + RSA signing as implemented in `src/gtpr.rs`
* `DETECTIC_URL`, `DETECTIC_USER`, `DETECTIC_PASSWORD`, `DETECTIC_DIALECT`, `DETECTIC_SECRET` are configuration variables. Values are never logged or exposed.

This is the default diagnostic and sensor-data mechanism. All read-only operations should use GTPR/GDPR.

## Temporary shell access — historically validated

Shell access was historically obtained only via manufacturer data-model objects, not SSH:

```
GTPR/GDPR
   ↓
DEV2_TELNET_CFG
   ↓
temporary telnet enable
   ↓
DEV2_LIFEMOTE_AGENT
   ↓
Lifemote script
   ↓
telnetd -p 8888 -l /bin/sh
   ↓
temporary shell
```

Characteristics:
* NOT SSH
* Temporary, manual, and must be explicitly authorized
* Requires GTPR `so` writes to enable Telnet and Lifemote agent
* Lifemote agent downloads and executes a script via `/usr/bin/phoenix.sh` and can start `telnetd`
* Must be reverted after testing: stop `telnetd`, disable Lifemote, disable Telnet via GTPR
* Must never be enabled automatically by an agent
* Must never be treated as a persistent deployment mechanism

## Detectic deployment documentation — historical test flow

Historically validated manual test flow:

```
GTPR authorization
→ temporary shell
→ temporary Detectic binary
→ manual execution
→ observation
→ stop Detectic
→ stop temporary telnetd
→ disable temporary Lifemote/Telnet configuration
```

Historical test location:
```
/var/tmp/detectic
```

Planned persistent location, not historically used:
```
/var/run/misc/misc_rw/detectic/
```

Distinguish clearly between manual historical tests and future persistent deployment proposals.

## Security rules

* Never assume SSH access exists.
* Never request or invent SSH credentials.
* Never expose `DETECTIC_PASSWORD`.
* Never expose `DETECTIC_SECRET`.
* Never enable Telnet/Lifemote without explicit authorization.
* Never use Lifemote as an automatic deployment mechanism.
* Never modify persistent router configuration without explicit authorization.
* Never reboot/power-cycle without explicit authorization.
* Never modify firmware/rootfs/bootloader.
* GTPR/GDPR read-only operations are the default diagnostic mechanism.

## Agent requirement

Future agents MUST use GTPR/GDPR HTTP API as the standard EX520 access method. Do not assume SSH exists, do not create SSH credentials, do not enable Telnet/Lifemote without explicit authorization.

## Evidence

Historically validated artefacts:
* `ex520-network-map-gdpr.md`
* `investigations/m5_smoke_test_report.md`
* `investigations/m4_3_execution_paths.md`
* `investigations/admin_shell_access.md`
* `investigations/m4_4_detectic_gdpr_runtime.md`
* `investigations/m5_persistence_strategy.md`
* `docs/EX520_ON_ROUTER_DEPLOYMENT_DISCOVERY.md`
* `docs/EX520_ON_ROUTER_RUNTIME.md`

Last updated: 2026-08-23
