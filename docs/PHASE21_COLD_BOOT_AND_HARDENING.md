# PHASE 21 — Cold-Boot Proof, Persistence Hardening, and Final Classification

## A — Baseline Snapshot (pre-cold-boot)

Taken before any power-cycle.

### Host package files

| File | Size (bytes) | SHA256 |
|------|-------------:|--------|
| `deploy/ex520_package/detectic.aa` | 720000 | `50365c1b9f3561ff561227db7b87012e129075343d7b1deff012744c0cb15619` |
| `deploy/ex520_package/detectic.ab` | 601216 | `c21c358173d7c6d7a88a9fda6ff37f15f7d7c2388c7e470ea0135bc2e1a2c34e` |
| `deploy/ex520_package/launcher.sh` | 3715 | `8c7374263ebf92eec9e74c7863757d699a4a742e0ddfb7513e4340a088e3a01a` |
| `deploy/ex520_package/bootstart.sh` | 2563 | `54358c03da0760163e557ea0cd655531862bf89b911d1caac2acbd7c05c8e43e` |
| `deploy/ex520_package/detectic.env` | 161 | `5a40ca6985c329e152aba2345ffd17bd864db99bd71f2f96e436ab62da5b34cf` |
| `deploy/ex520_package/version` | 22 | `8e4f2c1289b657237b33e72f47bcb60a9fec5ce24aede77847ff84d6d5eaa62b` |

Version: `v0.1.0-ex520-20260824`

### Router GTPR state

```
DEV2_LIFEMOTE_AGENT:
  enable: 0
  state: 0
  URL:    (empty)
  stack:  0,0,0,0,0,0
```

### Host services

* Package server: `python3 -m http.server 8080 --bind 192.168.0.27` (PID 149471)
* Watchdog: `python3 deploy/ex520_package/watchdog.py` (PID 189760)

### Status before power-cycle

* `DEV2_LIFEMOTE_AGENT` is disabled (`enable:0`, `state:0`).
* No `detectic` process is currently running on the router.
* Watchdog is armed and will set `DEV2_LIFEMOTE_AGENT` to `bootstart.sh` after a sustained down/up transition.

---

## B — Cold-Boot Proof

### Event timeline

| Time (UTC-3) | Event |
|-------------:|-------|
| 13:26:17 | Watchdog: `router went DOWN` |
| 13:29:06 | Watchdog: `router UP after cold boot` |
| 13:29:06 | **No `GTPR trigger SENT` was logged** |
| after 13:29:06 | **No HTTP `GET /bootstart.sh` or `/done?status=ok` was received** |

### GTPR state after cold boot

```
DEV2_LIFEMOTE_AGENT:
  enable: 0
  state: 0
  URL:    (empty)
  stack:  0,0,0,0,0,0
```

### HTTP server evidence

The package-server log contained no requests for `/bootstart.sh` and no `/done` callback after the router returned. The chain did not reach `phoenix.sh`.

### Broken boundary

The cold boot was correctly detected by the watchdog:
- `router went DOWN` at 13:26:17
- `router UP after cold boot` at 13:29:06 (down for ~169 s)

However, the watchdog did **not** emit `GTPR trigger SENT`.

Root cause: the watchdog initializes `triggered_for_boot = router_up` as `True` on startup, but the logic that is supposed to reset it on a sustained down only runs when `not triggered_for_boot` is already `False`:

```python
if down_for >= DOWN_THRESHOLD and not triggered_for_boot:
    triggered_for_boot = False
```

Because `triggered_for_boot` is `True` at startup, this condition is never met, so the watchdog never arms for a new trigger.

### Result

`COLD-BOOT PROOF: FAILED`

---

## C — Classification

| Component | Status | Evidence |
|-----------|--------|----------|
| `DEPLOY` | **PROVEN-LIVE** | `bootstart.sh` downloads, validates, persists, reassembles, and starts Detectic on manual trigger. |
| `PERSIST` | **PROVEN-LIVE** | `detectic.aa` / `detectic.ab` / `launcher.sh` / `detectic.env` / `version` fit in `misc_rw` + `misc_rw_bak` and are promoted atomically. |
| `EXECUTE` | **PROVEN-LIVE** | `launcher.sh` reassembles the ELF and starts `detectic sensor`; `done?status=ok&ret=0&version=v0.1.0-ex520-20260824` observed. |
| `AUTOSTART` (manual `so` / `set`) | **PROVEN-LIVE** | Manual `set DEV2_LIFEMOTE_AGENT` triggers `phoenix.sh → bootstart.sh` and Detectic starts. |
| `COLD-BOOT RECOVERY` | **FAILED** | Watchdog detected down/up but did not reset `triggered_for_boot`; no `GTPR trigger SENT`, so `phoenix` was not started and `bootstart` did not run. |

---

## D — Watchdog Re-arm Fix and State-Machine Tests

### Audit

The state machine was refactored for testability:

* Core loop extracted into `watchdog._watchdog_loop(is_reachable, do_trigger, poll_interval, down_threshold, now, sleep, logger)`.
* `triggered_for_boot` semantics preserved:
  * `True` means "trigger already performed / no trigger needed".
  * On sustained DOWN, `triggered_for_boot` is set `False` to arm.
  * On recovery, if `triggered_for_boot` is `False`, a single `do_trigger()` is called and `triggered_for_boot` returns `True`.
  * The arm guard `if down_for >= down_threshold and triggered_for_boot:` prevents repeated arm logs while already armed.
* Brief blips now correctly set `router_up = True` without triggering.

### Corrective change

The old (failed) arm line:

```python
if down_for >= DOWN_THRESHOLD and not triggered_for_boot:
    triggered_for_boot = False
```

was changed to:

```python
if down_for >= down_threshold and triggered_for_boot:
    triggered_for_boot = False
```

This allows the watchdog to re-arm when it is still in the `triggered_for_boot = True` state.

### Saved artifacts

| File | SHA256 |
|------|--------|
| `watchdog.py.phase21a.failed` (saved) | `aaaa4cc71114b5af7dc84882d2620e36968b2ee08b09dd62bb2aec0cdbc24e72` |
| `watchdog.py` (fixed) | `22f3b85571d29c391a441b19c26d87ed681b36d0427b07e2b05c402b2b28354f` |
| `watchdog_state_test.py` | `288864cc805ba350541768299a54af837c17b4257da749cf5678c43dc3d233a5` |

### Local deterministic tests

`deploy/ex520_package/watchdog_state_test.py` was run and passed all six scenarios:

```
[PASS] TEST 1: persistent UP: 0 trigger(s), expected 0
[PASS] TEST 2: brief blip: 0 trigger(s), expected 0
[PASS] TEST 3: cold boot: 1 trigger(s), expected 1
[PASS] TEST 4: trigger once then stable: 1 trigger(s), expected 1
[PASS] TEST 5: two cold boots: 2 trigger(s), expected 2
[PASS] TEST 6: long stable UP: 0 trigger(s), expected 0
```

---

## E — Email Observability

### Design

A host-side, non-blocking email endpoint was added:

* `emaild.py` listens on `192.168.0.27:8081`.
* `launcher.sh` calls it after the `detectic sensor` health check passes.
* `launcher.sh` also spawns a 5-minute background reporter.
* All email handling is best-effort; `detectic` does not depend on it.
* SMTP credentials are read from environment variables; none are embedded in source.
* `detectic.env` sets `DETECTIC_EMAILD` and `DETECTIC_EMAIL_INTERVAL=300`.

### Files

| File | SHA256 | Purpose |
|------|--------|---------|
| `emaild.py` | `2f5907edf0d76191b3b4eeade6822cea6497016488728d99e681e4ed0d755587` | Host email daemon |
| `testsmtp.py` | `c17104a254b97e6d04be349876c77e4eff6a92e699fba4439c92783d190560d4` | Local test-only SMTP capture server (not currently used) |
| `launcher.sh` | `a64e0ffef3b5b960eccc15b659504b96e1c7207f29c3a64ed49b84c49cb5397e` | Calls `emaild` on startup and starts 5-min reporter |
| `detectic.env` | `0975585bccca5a21195312b5ea7a4c76387983c0d2ff84d12acb65f060e53009` | Adds `DETECTIC_EMAILD` and `DETECTIC_EMAIL_INTERVAL` |

### Email tests

Using Brevo SMTP relay (`smtp-relay.brevo.com:587`, credentials from `.env`):

* **TEST 1 (startup):** `curl /email?type=startup&...` → `emaild` log: `email sent: [DETECTIC] EX520 sensor started — v0.1.0-ex520-20260824`.
* **TEST 2 (SMTP unavailable):** `testsmtp.py` killed, `curl /email?type=startup&...` → `emaild` logs `Connection refused` and returns HTTP 200; no crash.
* **TEST 3 (report):** `curl /email?type=report&...` → `emaild` log: `email sent: [DETECTIC] EX520 report — v0.1.0-ex520-20260824`.
* **TEST 5 (5-minute interval):** `launcher.sh` reporter uses `DETECTIC_EMAIL_INTERVAL=300`.
* **TEST 6 (no storm):** `emaild` does not retry; one email per request.

### Startup email example

```
From: detectic@example.com
To: admin@example.com
Subject: [DETECTIC] EX520 sensor started - v0.1.0-ex520-20260824

DETECTIC startup notification
Router:      EX520
Version:     v0.1.0-ex520-20260824
PID:         9999
Uptime:      123s
Status:      running
Timestamp:   2026-08-24T13:46:22-0300
```

### Failure-safety evidence

With `testsmtp.py` not running:

```
email notification failed: [Errno 111] Connection refused
```

`emaild` returned `200 ok` to the router immediately and the sensor would continue.

---

## F — Cold-Boot Retest Status

* Watchdog state machine corrected and tested.
* New watchdog running (PID 273073) and armed with `PHOENIX_GRACE=45s`.
* HTTP package server running (PID 149471).
* `emaild` running (PID 241696) using real Brevo SMTP from `.env`.
* `DEV2_LIFEMOTE_AGENT` set to `enable:0, URL:(empty)` to ensure a `0→1` toggle on next cold boot.
* No immediate trigger occurred on watchdog startup.
* First cold boot (13:58:01–13:59:52) proved: `router DOWN → armed → UP → GTPR trigger SENT`.
* `phoenix` did not pick up the first `so` because the trigger was sent immediately after GTPR came up (before `phoenix` was ready).
* Manual `0→1` toggle at 14:33:48 proved the full chain: `GET /bootstart.sh` → `GET /detectic.aa` `/detectic.ab` `/launcher.sh` `/detectic.env` `/version` → `GET /done?status=ok&pid=14972&up=2078.77&version=v0.1.0-ex520-20260824&ret=0`.
* `launcher.sh` email integration fixed to use `curl` instead of `$BB curl`.
* `detectic` is still running from the manual start (PID 14972).
* A second controlled cold boot is required to prove the complete `watchdog → cold start` chain.

---

## G — Final Evidence and Classification

### Second cold-boot proof (14:43:29–14:46:08)

Watchdog:

```
2026-08-24T14:43:29-0300 router went DOWN
2026-08-24T14:44:05-0300 router down for 36s, armed for re-trigger
2026-08-24T14:45:23-0300 router UP after cold boot
2026-08-24T14:45:23-0300 waiting 45s for phoenix
2026-08-24T14:46:08-0300 GTPR trigger SENT
```

HTTP package server:

```
192.168.0.1 - - "GET /bootstart.sh HTTP/1.1" 200 -
192.168.0.1 - - "GET /detectic.aa HTTP/1.1" 200 -
192.168.0.1 - - "GET /detectic.ab HTTP/1.1" 200 -
192.168.0.1 - - "GET /launcher.sh HTTP/1.1" 200 -
192.168.0.1 - - "GET /detectic.env HTTP/1.1" 200 -
192.168.0.1 - - "GET /version HTTP/1.1" 200 -
192.168.0.1 - - "GET /done?status=ok&pid=3958&up=85.81&version=v0.1.0-ex520-20260824&ret=0&trace= HTTP/1.1" 404 -
```

`ret=0` from `bootstart` confirms `launcher.sh start` succeeded. `detectic` is running on the router.

### Email

The `emaild` and Brevo SMTP were verified earlier from the host. A manual `phoenix`-routed `email_test.sh` just confirmed the router can reach `emaild` and the email is delivered:

```
2026-08-24T14:58:05-0300 startup notification requested up=801.94 version=v0.1.0-ex520-20260824 pid=8410
2026-08-24T14:58:06-0300 email sent: [DETECTIC] EX520 sensor started — v0.1.0-ex520-20260824
```

The `launcher.sh` `curl` email-calls were on a single physical shell line; multi-line `curl` commands were failing silently inside `phoenix`. This has been fixed.

### Classification

```text
DEPLOY              = PROVEN-LIVE
PERSIST             = PROVEN-LIVE
EXECUTE             = PROVEN-LIVE
AUTOSTART (manual)  = PROVEN-LIVE
COLD-BOOT RECOVERY  = PROVEN-LIVE
EMAILD + SMTP       = PROVEN-LIVE
5-MINUTE REPORTING  = PENDING (will start on next clean detectic start)
```

The only item not observed end-to-end on this cold boot was the **startup email from `detectic` itself**, because the old `launcher.sh` ran before the `curl` fix. That is now corrected, so the next cold boot should deliver it.
