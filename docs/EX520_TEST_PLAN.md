# EX520 Test Plan

## Unit / integration tests (host)

```bash
cargo test
python3 -m unittest tests/test_supervisor.py -v
```

These cover:

* Supervisor state machine and backoff
* SHA-256 verification paths
* mDNS question parsing
* ARP table parsing
* HTTP server response formatting
* Sensor configuration parsing
* GTPR collector parsing
* Event transport and spooling

## Local functional tests

### 1. HTTP server (no router required)

```bash
make build
DETECTIC_PASSWORD=test DETECTIC_SECRET=sk DETECTIC_SENSOR_ID=ex520-test \
  DETECTIC_MDNS=0 ./target/debug/detectic sensor &
sleep 2
curl -s http://127.0.0.1:8787/health | python3 -m json.tool
curl -s http://127.0.0.1:8787/ready
curl -s http://127.0.0.1:8787/metrics
curl -s http://127.0.0.1:8787/version
kill %1
```

Expected:

* `status` is `healthy` or `unhealthy`.
* `ready` is `false` until a successful GTPR poll.
* `port` is `8787`.
* `/version` returns `detectic <version>`.

### 2. Package server

```bash
./deploy/ex520_package/build_package.sh
cp _fw_build/package/* deploy/ex520_package/
python3 deploy/ex520_package/package_server.py &
sleep 1
curl -s http://127.0.0.1:8080/manifest.json | python3 -m json.tool
curl -s http://127.0.0.1:8080/detectic.sha256
kill %1
```

Expected:

* `manifest.json` contains `version`, `files.detectic.aa`, `files.detectic.ab`,
  `files.detectic`.
* `.sha256` files contain 64 hex chars.

### 3. Sensor `sensor` with live EX520

```bash
DETECTIC_PASSWORD=<real> DETECTIC_SECRET=<real> DETECTIC_SENSOR_ID=ex520-001 \
  ./target/aarch64-unknown-linux-musl/release/detectic sensor
```

Expected:

* `INFO service_started ...`
* `INFO poll_success stations=N events=M` every `DETECTIC_INTERVAL` seconds.
* `curl http://[ex520-ll%25enp2s0]:8787/health` returns healthy.

## Live EX520 acceptance tests

### Cold boot

1. Ensure the supervisor has the correct URL and password.
2. Power-cycle or `echo b > /proc/sysrq-trigger` (use with caution).
3. Observe the host supervisor:
   * `ROUTER_DOWN` state
   * `ROUTER_UP` after ~30 s
   * `GTPR_READY`
   * `SENSOR_STARTING` after exactly one `so DEV2_LIFEMOTE_AGENT`
   * `SENSOR_HEALTHY` after `/health` returns `healthy`
4. Verify one Phoenix process on the router:
   `ps | grep phoenix` should show exactly one.
5. Verify one `detectic` process.
6. Verify `curl http://detectic.local:8787/health` or
   `curl http://192.168.0.1:8787/health`.

Repeat 3 times.

### Power cycle

1. Unplug the router power.
2. Wait 10 seconds.
3. Reconnect power.
4. Wait for supervisor to trigger and sensor to become healthy.
5. Record result as `POWER_CYCLE_AUTOSTART = PROVEN-LIVE` when passing 3/3.

### Duplicate-Phoenix prevention

1. While sensor is healthy, confirm no additional `so DEV2_LIFEMOTE_AGENT` is
   sent for 5 minutes.
2. Use `done_log.txt` and supervisor logs to prove exactly one trigger.

### TCP 8787 validation

1. From the host: `curl http://detectic.local:8787/health`.
2. Verify JSON contains `port: 8787` and `ready: true`.
3. Record as `TCP_8787 = PROVEN-LIVE`.

### mDNS validation

1. From the host: `avahi-resolve -n detectic.local` or
   `dns-sd -L detectic _http._tcp`.
2. Verify `detectic.local` resolves to the EX520 LAN IP.
3. Record as `MDNS = PROVEN-LIVE` if `detectic.local:8787/health` works.

### Failure tests

| Test | Procedure | Expected |
|------|-----------|----------|
| Package server down | Stop package server; trigger Phoenix | `bootstart.sh` logs `download_*` error; router unaffected |
| Corrupted part | Edit `detectic.aa.sha256` to wrong value | `bootstart.sh` rejects; no binary execution |
| Missing part | Remove `detectic.ab` from package server | `download_ab` error; no execution |
| GTPR unavailable | Firewall port 80 briefly | Supervisor waits, retries, does not reboot router |
| Sensor crash | `kill -9 <detectic pid>` | `launcher.sh` restarts up to 5 times, then stops |
| Backend unavailable | Drop backend route | Sensor keeps polling; events spool locally |
| mDNS unavailable | Block UDP 5353 | Sensor continues; mDNS warning in logs |
| Watchdog restart | Restart host supervisor | Supervisor recovers state, no duplicate Phoenix |

### Evidence classification

Record each item as:

* **PROVEN-LIVE** — observed on a real EX520.
* **PROVEN-FROM-SOURCE** — verified by source code / unit tests.
* **INFERRED** — logically follows from proven facts.
* **NOT TESTED** — not yet verified on live hardware.

Do not upgrade `NOT TESTED` to `PROVEN` without live evidence.
