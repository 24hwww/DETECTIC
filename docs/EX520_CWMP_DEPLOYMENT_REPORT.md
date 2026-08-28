# EX520 CWMP-Based Detectic Deployment — Final Report

## 1. Executive Summary

A minimal, idempotent CWMP ACS (`ex520_detectic_acs.py`) was built and validated on a live TP-Link EX520V. It autonomously re-deploys the `detectic` sensor after cold boots and after a sensor crash, using the router's stock `Device.X_TP_LIFEMOTE_EXT.LifemoteAgent` (Phoenix) path. The sensor reliably produces Wi-Fi association events after recovery.

The deployment does **not** require any new device configuration beyond the existing CWMP ACS URL. It also does **not** depend on the IPv6 GTPR management path after initial CWMP registration, reducing the operational surface area.

## 2. Architecture

```text
            TP-Link EX520V                  Host / package server
            --------------                  ---------------------
            |                                 |
            |  cwmp Inform  ----------------> |  ex520_detectic_acs.py
            |      (HTTP POST /acs)           |  (CWMP + package server)
            |                                 |
            |  GetParameterValues /           |
            |  SetParameterValues             |
            |  Reboot  <----------------------|
            |                                 |
            |  HTTP GET bootstart.sh ------>  |
            |  HTTP GET detectic.aa/ab ---->  |
            |  HTTP POST /done               |
            |  HTTP GET /heartbeat           |
            |  HTTP GET /env_line            |
            v                                 v
      /var/tmp/detectic/detectic       192.168.0.27:8080
            |
            v
      192.168.0.1:80  (router web API)
            |
            v
      DEV2_WIFI_APDEV_ASSOCDEV
            |
            v
      local event processing
            |
            v
      host callbacks (/env_line)
```

Key components:

* `deploy/ex520_package/ex520_detectic_acs.py` — combined CWMP ACS and package/heartbeat server.
* `deploy/ex520_package/bootstart.sh` — router-side bootstrap script, downloaded by `phoenix.sh`.
* `deploy/ex520_package/launcher.sh` — watchdog that starts `detectic` and reports heartbeats.
* `deploy/ex520_package/none.sh` — no-op script used to keep `LifemoteAgent` enabled while idle.
* `deploy/ex520_package/detectic.env` — runtime configuration for the sensor.

## 3. State Machine

The ACS tracks:

* `last_heartbeat` — timestamp of last `/heartbeat` from `launcher.sh`.
* `last_heartbeat_pid` — PID of the running `detectic`/`launcher` pair.
* `last_done_status` / `last_done_pid` — result of `bootstart.sh`.
* `pending_command` — `execute` or `reboot`.
* `target_url` — `bootstart.sh` when a re-deploy is needed, `none.sh` when healthy.

Decision on every `Inform`:

* **Cold boot** (`0 BOOTSTRAP`, `1 BOOT`, vendor reboot events) → trigger `bootstart.sh`.
* **Heartbeat stale** (no heartbeat for `HEARTBEAT_TIMEOUT`, currently 75 s) → trigger `bootstart.sh`.
* **Heartbeat recent** → set `LifemoteAgent.URL` to `none.sh` so `phoenix` stays enabled but does not re-run `bootstart`.
* **Queued `/reboot_now`** → issue CWMP `Reboot` on the next `Inform` that continues the session, then treat the subsequent cold boot normally.

## 4. Evidence

### 4.1 Cold boot autostart

A CWMP `Reboot` was queued and issued; the router cold-booted, sent `1 BOOT`, and the ACS re-deployed `detectic`.

```text
2026-08-28T08:04:16-0300 /reboot_now requested: will issue cwmp:Reboot on next Inform
2026-08-28T08:05:04-0300 sending Reboot
2026-08-28T08:05:04-0300 Reboot accepted by CPE, expecting disconnect
2026-08-28T08:06:28-0300 cold-boot Inform received
2026-08-28T08:06:28-0300 trigger_decision need=True reason=cold_boot target=.../bootstart.sh
2026-08-28T08:06:28-0300 sending InformResponse
2026-08-28T08:06:28-0300 GetParameterValues Enable=1 URL=.../bootstart.sh
2026-08-28T08:06:29-0300 sending SetParameterValues phase=0 params=[('...Enable', '0', ...), ('...URL', '.../bootstart.sh', ...)]
2026-08-28T08:06:29-0300 sending SetParameterValues phase=1 params=[('...Enable', '1', ...), ('...URL', '.../bootstart.sh', ...)]
2026-08-28T08:06:29-0300 SetParameterValues final accepted, closing session
2026-08-28T08:06:30-0300 "GET /bootstart.sh HTTP/1.1" 200 -
...
2026-08-28T08:06:41-0300 EVENT done {'status': 'ok', 'pid': '3659', 'up': '73.98', ... 'ret': '0'}
```

The sensor started and produced its first successful poll on the new boot:

```text
2026-08-28T11:06:31.498001385Z_INFO_poll_success_stations=3_events=3
2026-08-28T08:06:40-0300 GET /env_line?n=93&d=...INFO_poll_success_stations=3_events=3  HTTP/1.1" 200 -
```

### 4.2 Crash recovery

`kill_detectic.sh` was used to terminate the sensor. Heartbeats stopped, the next `Inform` was flagged `heartbeat_stale`, and the ACS re-triggered `bootstart.sh`.

```text
... trigger_decision need=True reason=heartbeat_stale target=.../bootstart.sh
... sending SetParameterValues ... URL=.../bootstart.sh
... GET /bootstart.sh
```

After the restart the sensor resumed heartbeats and `poll_success` logs.

### 4.3 Stable idle state

Once healthy, the ACS sets `LifemoteAgent.URL` to `none.sh`. `phoenix` downloads and executes `none.sh` (exit 0) periodically, but no longer re-runs `bootstart`:

```text
2026-08-28T08:08:28-0300 trigger_decision need=False reason=heartbeat_recent age=11s target=.../none.sh
2026-08-28T08:08:28-0300 sensor appears healthy; setting LifemoteAgent to none if session continues
...
2026-08-28T08:08:29-0300 "GET /none.sh HTTP/1.1" 200 -
```

### 4.4 ACS counters (from `acs_state.json`)

```json
{
  "last_heartbeat": 1787915367.922215,
  "last_heartbeat_pid": "4564",
  "last_done_status": "ok",
  "last_done_pid": "3659",
  "boot_count": 7,
  "trigger_count": 25,
  "skip_count": 58,
  "reboot_count": 2
}
```

### 4.5 Health endpoint confirmation

From the host, the router's `detectic` HTTP health endpoint is reachable and reports `healthy`/`ready`:

```bash
$ curl -m 5 -s http://192.168.0.1:8787/health
{ "ready":"true", "port":"8787", "version":"0.1.0", "uptime":"380",
  "sensor_id":"ex520-001", "gtpr":"ok", "devices":"4", "backend":"",
  "mdns":"", "status":"healthy" }

$ curl -m 5 -s http://192.168.0.1:8787/metrics
{ "uptime_seconds":"383", "device_count":"4", "last_poll_ago":"0s",
  "last_upload_ago":"5s", "gtpr_status":"ok", "mdns_status":"",
  "backend_status":"" }
```

`gtpr=ok` and `last_poll_ago=0s` confirm the sensor is actively polling the router GTPR API.

## 5. Guarantee Matrix

| Capability | Status | Evidence |
|------------|--------|----------|
| Cold boot auto-detect | **PROVEN-LIVE** | `1 BOOT` handled; `trigger_decision reason=cold_boot` |
| Sensor re-install after cold boot | **PROVEN-LIVE** | `/bootstart.sh` downloaded after `1 BOOT`; `done status=ok` |
| Functional sensor output after cold boot | **PROVEN-LIVE** | `poll_success stations=3 events=3` on 08:06:31 |
| Crash detection via heartbeat timeout | **PROVEN-LIVE** | `heartbeat_stale` triggered `bootstart` after `kill_detectic` |
| Sensor re-install after crash | **PROVEN-LIVE** | Same `bootstart` flow after `kill_detectic` |
| CWMP `Reboot` command delivery | **PROVEN-LIVE** | `/reboot_now` → `Reboot` sent → `RebootResponse` → cold boot |
| Idempotent ACS (no duplicate triggers when healthy) | **PROVEN-LIVE** | `none.sh` set when heartbeat recent; `phoenix` runs no-op |
| Package files served with `Connection: close` | **PROVEN-FROM-SOURCE** | `_write_request(... close=True)` for CWMP/file paths |
| Heartbeat/callback endpoints | **PROVEN-LIVE** | `/heartbeat`, `/env_line`, `/done` observed |
| IPv4 `DETECTIC_URL=http://192.168.0.1` works from router | **PROVEN-LIVE** | `poll_success` observed with IPv4 URL |
| Health endpoint reachable and `healthy` | **PROVEN-LIVE** | `curl 192.168.0.1:8787/health` returns `status=healthy, ready=true` |
| Active GTPR polling in `detectic` | **PROVEN-LIVE** | `gtpr=ok`, `last_poll_ago=0s`, `device_count=4` |
| IPv6 link-local scope handling in `detectic` binary | **INFERRED** | `http.rs` parses `%25` scope; not validated because IPv4 path is sufficient |

## 6. Known Limitations and Notes

1. **GTPR web server lockout / 406.** Repeated `detectic` GTPR login attempts from the router occasionally cause the management web server to return `406 Not Acceptable` to the sensor's `/cgi/getGDPRParm` request. A clean router power cycle or CWMP `Reboot` clears the condition. This is consistent with the previous `investigations/ex520_rust_login_diagnostic.md` finding that the router's `httpd` can enter a lockout state.
2. **Heartbeat timeout.** `HEARTBEAT_TIMEOUT=75` seconds is chosen to be longer than the `launcher.sh` heartbeat interval (30 s) and the router CWMP periodic interval.
3. **Phoenix polling.** The ACS now sets `LifemoteAgent.URL` to `none.sh` when healthy. This keeps the agent enabled for future control but stops it from repeatedly downloading `bootstart.sh`.
4. **CWMP Reboot and double-reboot prevention.** The ACS consumes a queued `Reboot` when a cold-boot `Inform` arrives, preventing a second unnecessary `Reboot`.
5. **IPv4 vs IPv6 management.** The `detectic` sensor on the router successfully uses `http://192.168.0.1` for the internal GTPR API. Host management still requires the IPv6 link-local path documented in `AGENTS.md`.

## 7. How to Reproduce

1. Build and stage the package:

   ```bash
   ./deploy/ex520_package/build_package.sh
   cp _fw_build/package/* deploy/ex520_package/
   ```

2. Start the ACS + package server on a host reachable by the router:

   ```bash
   cd deploy/ex520_package
   python3 ex520_detectic_acs.py
   ```

3. Point the EX520 CWMP ACS URL at the host (or use an existing ACS/DNS redirect so `http://192.168.0.27:8080/acs` is reached).

4. Reboot the router. The `1 BOOT` `Inform` triggers `bootstart.sh` and the sensor starts.

5. To test crash recovery from the host:

   ```bash
   # Queue a CWMP Reboot
   curl http://192.168.0.27:8080/reboot_now

   # Or trigger a fresh bootstart on the next Inform (sets pending=execute)
   curl http://192.168.0.27:8080/trigger_now
   ```

   Note: A direct `kill -9` of the `detectic` process on the router causes `launcher.sh` to stop heartbeats, and the ACS re-triggers within the next `HEARTBEAT_TIMEOUT` window.

## 8. Files Changed

* `deploy/ex520_package/ex520_detectic_acs.py` — idempotent CWMP ACS with `none.sh` idle state and double-reboot prevention.
* `deploy/ex520_package/none.sh` — new no-op `LifemoteAgent` payload.
* `deploy/ex520_package/detectic.env` — reverted to stable IPv4 `DETECTIC_URL=http://192.168.0.1`.
* `deploy/ex520_package/kill_detectic.sh` — narrowed to only `detectic` / `launcher` PIDs.

## 9. Conclusion

The CWMP ACS path is **PROVEN-LIVE** for cold-boot and crash-recovery autostart of the Detectic sensor on the EX520V. The system is now idempotent, self-correcting, and stable: it deploys on cold boot, recovers from sensor death, and quiets down to a no-op `phoenix` payload when healthy.
