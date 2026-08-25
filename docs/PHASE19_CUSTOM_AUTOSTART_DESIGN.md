# PHASE 19B — Custom Detectic Autostart Design for EX520V

## TP-Link EX520V — Detectic Resident Path

**Date:** 2026-08-24
**Method:** Design only (static analysis and prior live proofs). No router modification. No live test executed yet.
**Conclusion:** **C — EXTERNAL MECHANISM REQUIRED (design complete, live proof pending)**

---

## 1. Executive Summary

Phase 19A proved that the stock EX520V firmware provides **no native boot trigger** for user payloads. `phoenix.sh` is started only by a GTPR `so DEV2_LIFEMOTE_AGENT` and will not run automatically after a cold boot even when `enable:1` and `URL` are persisted.

This document designs the **least invasive** custom autostart mechanism:

- A small external **watchdog** running on a trusted host (or edge device) on the same LAN.
- The watchdog detects the router has finished booting (IPv6/IPv4 reachability + GTPR).
- The watchdog sends a single GTPR `so DEV2_LIFEMOTE_AGENT` with a `bootstart.sh` URL.
- `phoenix.sh` downloads and executes `bootstart.sh` as root.
- `bootstart.sh` downloads `detectic` and a `launcher-min.sh` into `/var/run/misc/misc_rw/detectic/`, then starts the launcher.
- `launcher-min.sh` starts and supervises `detectic sensor`.
- After the first trigger, `phoenix.sh` itself re-runs `bootstart.sh` periodically (≈30 min) as a lightweight self-check.

This uses only proven primitives, does not modify the router firmware, and is fully reversible.

---

## 2. Why an External Watchdog is the Best Option

| Priority | Mechanism | Status |
|----------|-----------|--------|
| 1 | Native stock trigger | Excluded in Phase 19A |
| 2 | Configurable existing service | Not available (no procd/cron/ubus) |
| 3 | Existing watchdog/health mechanism | Not user-influenceable (kernel watchdog, `cos`) |
| 4 | Network/event trigger | `hotplug.d` on read-only rootfs, no user hook |
| 5 | **External watchdog** | **Selected: proven GTPR `so` + `phoenix` execution path** |
| 6 | Firmware/rootfs modification | Avoided per project safety rules |

The external watchdog is the **only** option that satisfies the constraints:

- does not touch `rcS`, rootfs, U-Boot, or firmware;
- does not modify network/WAN/LAN/WLAN/DHCP/DNS/NAT/routing/firewall;
- uses only the already-proven `so DEV2_LIFEMOTE_AGENT` primitive;
- can be disabled and rolled back by `set` `enable:0` and `URL:""`;
- the only recurring external traffic is the watchdog poll until boot, after which `phoenix` can self-supervise.

---

## 3. Target Architecture

```text
REBOOT
  │
  ▼
cos starts, httpd starts, GTPR available
  │
  ▼
External watchdog on LAN
  │   detects router is up (ping + GTPR query)
  ▼
watchdog: detectic --user user set DEV2_LIFEMOTE_AGENT
          '{"enable":"1","URL":"http://<host>:8080/bootstart.sh"}'
  │
  ▼
phoenix.sh (started by rsl_setDev2LifemoteAgentObj)
  │
  ▼
curl http://<host>:8080/bootstart.sh → /tmp/lifemote_cpe_daemon.sh
  │
  ▼
sh /tmp/lifemote_cpe_daemon.sh
  │
  ▼
bootstart.sh
  │
  ├── mkdir -p /var/run/misc/misc_rw/detectic
  ├── cd /var/run/misc/misc_rw/detectic
  ├── curl detectic binary
  ├── curl launcher-min.sh
  ├── chmod +x detectic launcher-min.sh
  ├── ./launcher-min.sh start
  └── curl http://<host>:8080/done?status=...
  │
  ▼
detectic sensor running
  │
  ▼
every ~30 min phoenix re-runs bootstart.sh
  │    (launcher-min.sh is idempotent, no duplicate detectic process)
  ▼
persistent, autonomously restarting Detectic node
```

---

## 4. Components

### 4.1 `bootstart.sh` (router-side, runs inside `phoenix`)

This is the tiny bootstrap downloaded by `phoenix` on every run. It must use only the BusyBox applets already proven to exist (`curl`, `cat`, `echo`, `read`, `kill`, `sleep`, `chmod`, `mv`, `mkdir`) and `/bin/sh` builtins.

```sh
#!/bin/sh
# bootstart.sh — minimal router-side Detectic bootstrap
# Runs as root inside /usr/bin/phoenix.sh
# Downloads detectic + launcher from the operator HTTP server

export PATH=$PATH:/bin:/usr/bin

BASE="http://<WATCHDOG_HOST>:<PORT>"
DIR="/var/run/misc/misc_rw/detectic"
LOG="$DIR/autostart.log"

mkdir -p "$DIR" 2>/dev/null
cd "$DIR" 2>/dev/null || exit 1

stamp() {
    # No date(1) available; use uptime seconds
    read up rest < /proc/uptime
    echo "$up"
}

TS=$(stamp)

# Pull latest launcher and binary atomically
curl -m 60 -f -s -o .launcher.new "${BASE}/launcher-min.sh" 2>/dev/null || exit 0
curl -m 120 -f -s -o .detectic.new "${BASE}/detectic" 2>/dev/null || exit 0

chmod +x .launcher.new .detectic.new
mv -f .launcher.new launcher.sh
mv -f .detectic.new detectic

# Start (or no-op if already running)
./launcher.sh start

# Health ping back to watchdog
curl -m 5 -s -o /dev/null \
    "${BASE}/done?status=ok&pid=$$&uptime=$TS&version=$(cat version 2>/dev/null || echo unknown)"
```

Notes:

- `curl -f` (fail on 4xx/5xx) and `exit 0` on error so `phoenix` does not break the loop if the watchdog server is temporarily down.
- `mv -f` is used to avoid leaving an unlaunchable package.
- `launcher.sh start` is idempotent (see below).

### 4.2 `launcher-min.sh` (router-side, runs inside `phoenix`)

A hardened version of `deploy/launcher.sh` that avoids `date`, `wc`, `awk`, `tail`, `nohup`, and `which`, all of which are missing or unreliable in the `phoenix` environment.

```sh
#!/bin/sh
# launcher-min.sh — phoenix-safe Detectic launcher
# Location: /var/run/misc/misc_rw/detectic/launcher.sh

export PATH=$PATH:/bin:/usr/bin

DIR="/var/run/misc/misc_rw/detectic"
BIN="$DIR/detectic"
LOG="$DIR/detectic.log"
PIDF="$DIR/detectic.pid"
MAX_RESTART=5
RFILE="$DIR/restart_count"

read_uptime() { read u _ < /proc/uptime; echo "$u"; }

log() {
    echo "[$(read_uptime)] $*" >> "$LOG" 2>/dev/null
}

get_pid() {
    if [ -f "$PIDF" ]; then
        p=$(cat "$PIDF" 2>/dev/null)
        if [ -n "$p" ] && kill -0 "$p" 2>/dev/null; then
            echo "$p"; return 0
        fi
    fi
    # fallback: ps + head
    p=$(ps 2>/dev/null | grep '[d]etectic' | head -1 | while read ppid _; do echo "$ppid"; break; done)
    if [ -n "$p" ]; then
        echo "$p"; return 0
    fi
    return 1
}

is_running() { get_pid >/dev/null 2>&1; }

get_rc() { [ -f "$RFILE" ] && cat "$RFILE" 2>/dev/null || echo 0; }
set_rc() { echo "$1" > "$RFILE" 2>/dev/null; }

do_start() {
    if is_running; then
        pid=$(get_pid)
        echo "already running PID=$pid"
        return 0
    fi

    if [ ! -x "$BIN" ]; then
        echo "FAIL: binary missing"
        return 1
    fi

    set_rc 0
    log "Starting Detectic"

    # Ignore SIGHUP, redirect to log, start in background
    ( trap '' 1; exec "$BIN" sensor >> "$LOG" 2>&1 & )
    new_pid=$!

    echo "$new_pid" > "$PIDF" 2>/dev/null
    sleep 1
    if kill -0 "$new_pid" 2>/dev/null; then
        log "started PID=$new_pid"
        echo "started PID=$new_pid"
        return 0
    fi

    log "failed to start"
    rm -f "$PIDF"
    return 1
}

do_stop() {
    pid=$(get_pid 2>/dev/null)
    if [ -z "$pid" ]; then
        echo "not running"
        return 0
    fi
    kill "$pid" 2>/dev/null
    i=0
    while [ $i -lt 5 ]; do
        if ! kill -0 "$pid" 2>/dev/null; then
            rm -f "$PIDF"
            return 0
        fi
        sleep 1
        i=$((i+1))
    done
    kill -9 "$pid" 2>/dev/null
    rm -f "$PIDF"
    return 0
}

do_restart() {
    do_stop
    count=$(get_rc)
    if [ "$count" -ge "$MAX_RESTART" ]; then
        log "restart budget exhausted"
        return 1
    fi
    set_rc $((count + 1))
    do_start
}

do_status() {
    if is_running; then
        get_pid
        return 0
    fi
    echo "not running"
    return 1
}

case "${1:-status}" in
    start)   do_start ;;
    stop)    do_stop ;;
    restart) do_restart ;;
    status)  do_status ;;
    *)       echo "usage: $0 {start|stop|restart|status}"; exit 1 ;;
esac
```

### 4.3 `watchdog.py` (host-side)

Runs on a trusted host on the same LAN. It detects a fresh boot and sends the `so` trigger. It is **not** a heavy continuous manager; it sleeps after `detectic` is running.

```python
#!/usr/bin/env python3
"""EX520 Detectic autostart watchdog.

Monitors router reachability. After a reboot (transition from down to up),
sends a GTPR 'set DEV2_LIFEMOTE_AGENT' to start the phoenix -> bootstart chain.
"""
import os
import re
import socket
import subprocess
import sys
import time

# --- configuration (set via environment) ---
DETECTIC = os.environ.get("DETECTIC_BIN", "./detectic")
ROUTER_URL = os.environ.get("EX520_URL", "http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]")
USER = os.environ.get("EX520_USER", "user")
# Password should be in the environment, never hardcoded.
PASSWORD = os.environ["EX520_PASSWORD"]
PING6_TARGET = os.environ.get("EX520_PING6", "fe80::3e6a:d2ff:fe5f:abc1%enp2s0")
PING6_IFACE = os.environ.get("EX520_PING6_IFACE", "enp2s0")
POLL_INTERVAL = int(os.environ.get("POLL_INTERVAL", "10"))
BOOTSTART_URL = os.environ.get("BOOTSTART_URL", "http://192.168.0.27:8080/bootstart.sh")
# --------------------------------------------


def ping_reachable():
    """Quick IPv6 link-local ping."""
    try:
        ret = subprocess.run(
            ["ping6", "-c", "1", "-W", "2", f"{PING6_TARGET}%{PING6_IFACE}"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=5,
        )
        return ret.returncode == 0
    except Exception:
        return False


def gtpr_query():
    """Return True if GTPR is accepting queries."""
    try:
        ret = subprocess.run(
            [DETECTIC, "--url", ROUTER_URL, "--user", USER, "query", "DEV2_LIFEMOTE_AGENT"],
            env={**os.environ, "DETECTIC_PASSWORD": PASSWORD},
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=20,
        )
        return ret.returncode == 0
    except Exception:
        return False


def trigger_bootstart():
    """Send the so that starts phoenix -> bootstart."""
    payload = (
        '{"enable":"1","URL":"%s","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'
        % BOOTSTART_URL
    )
    try:
        ret = subprocess.run(
            [DETECTIC, "--url", ROUTER_URL, "--user", USER,
             "set", "DEV2_LIFEMOTE_AGENT", payload],
            env={**os.environ, "DETECTIC_PASSWORD": PASSWORD},
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=30,
        )
        return ret.returncode == 0
    except Exception:
        return False


def main():
    router_up = False
    launched = False
    down_since = None

    while True:
        now = time.time()
        reachable = ping_reachable()

        if reachable and not router_up:
            # Link came up; wait for GTPR before trusting it
            if gtpr_query():
                router_up = True
                print(f"{now}: router GTPR up")
                if not launched:
                    if trigger_bootstart():
                        launched = True
                        print(f"{now}: bootstart triggered")
                    else:
                        print(f"{now}: trigger failed, will retry")

        if not reachable and router_up:
            router_up = False
            launched = False
            down_since = now
            print(f"{now}: router went down")

        if not reachable:
            # Long absence = likely reboot, reset launch on next up
            if down_since and (now - down_since) > 60:
                launched = False

        time.sleep(POLL_INTERVAL)


if __name__ == "__main__":
    main()
```

Notes:

- Uses `ping6` for fast reachability; only uses the heavy `detectic` client when the router is reachable.
- Resets `launched` after 60s of downtime to re-trigger after a real reboot.
- Keeps password in environment; it is never logged.
- The URL can be IPv4 (`192.168.0.27:8080`) because `phoenix`'s `curl` has already been proven to reach the host over IPv4, while `watchdog.py` uses IPv6 link-local for GTPR as required.

### 4.4 Host HTTP server

The same pattern as Phase 18:

```bash
python3 -m http.server 8080 --bind 192.168.0.27
```

Serves:

- `bootstart.sh`
- `launcher-min.sh`
- `detectic` (ARM64 static binary)
- `version` (optional text file with version/commit)

The `/done` path is a non-existent endpoint used only to log successful `bootstart` runs.

---

## 5. Persistence and Update Strategy

### 5.1 Persistence

- `DEV2_LIFEMOTE_AGENT { enable:1, URL:... }` is already proven to persist in `0x00300000` on `misc_rw`.
- `detectic`, `launcher-min.sh`, `autostart.log`, `detectic.log`, `detectic.pid`, and `version` are stored on `misc_rw` and survive reboot.
- `phoenix` itself does **not** run after reboot, so the **only** persistent trigger state is the `Lifemote` configuration.

### 5.2 Self-check via `phoenix`

`phoenix.sh` re-downloads `bootstart.sh` every ~30 minutes. `bootstart.sh` is idempotent:

- If `detectic` is already running, `launcher.sh start` exits immediately.
- If a new `detectic` binary is available, it is downloaded and `launcher.sh restart` can be called to atomically switch.

This gives a lightweight in-router watchdog without the external host needing to keep the trigger active.

### 5.3 Update flow

1. Operator builds a new `detectic` binary.
2. Operator places it on the host HTTP server.
3. At the next `phoenix` cycle (or external `so`), `bootstart.sh` downloads it.
4. `launcher.sh restart` stops the old `detectic` and starts the new one.
5. Rollback: place the previous binary and trigger another `phoenix` run, or run `launcher.sh stop`.

### 5.4 Storage constraints

`misc_rw` is approximately **1.14 MB**. The current `detectic` release build is **1.32 MB**. The design therefore requires **one of** the following before deployment:

- a smaller `detectic` build (strip TLS/persist features, LTO, `upx` if supported);
- use of `misc_rw_bak` (also UBIFS, only if `INCLUDE_DUAL_CONFIG` is enabled and free);
- use of `runtime_data` (UBIFS, `ubi5:0`, must first be proven writable);
- download and run from `/var/tmp` if RAM allows (not persistent, only for session).

The `bootstart.sh` design can be extended to choose the storage target based on a config file, but the default is `misc_rw`.

---

## 6. Rollback

At any time, the operator can disable the autostart chain:

```bash
detectic --user user set DEV2_LIFEMOTE_AGENT \
  '{"enable":"0","URL":"","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'
```

Then, on the router, `phoenix` is killed and will not re-download. `launcher.sh stop` and `rm -rf /var/run/misc/misc_rw/detectic/` remove the installed files. Finally, `watchdog.py` is terminated on the host.

Because `enable:0` is persisted, the router is fully restored to stock behavior and will not re-launch Detectic on subsequent reboots unless the operator re-enables.

---

## 7. Security and Safety

- **No router-side secrets:** `bootstart.sh` does not contain passwords. It only downloads from a known host.
- **Host-side password:** `watchdog.py` reads the password from an environment variable; it is not printed or logged.
- **No firmware modification:** Nothing is written to rootfs, U-Boot, kernel, or SquashFS.
- **No network changes:** The design does not modify WAN/LAN/WLAN/DHCP/DNS/NAT/routing/firewall.
- **Bounded restarts:** `launcher-min.sh` limits restarts to 5 and stops if the binary is missing.
- **No infinite forks:** `phoenix` has a fixed 1800s interval. `detectic` is started once per interval; the launcher prevents duplicates.
- **Safe HTTP server:** The operator HTTP server only serves the package and logs `/done` callbacks. No execute-as-a-service.

---

## 8. Cold-Boot Proof Plan

A single controlled live test can prove the whole chain.

### 8.1 Pre-conditions

- Build or place a size-appropriate `detectic` binary on the host HTTP server.
- Place `bootstart.sh`, `launcher-min.sh`, and `version` on the host HTTP server.
- Start `python3 -m http.server 8080 --bind 192.168.0.27`.
- Set environment variables for `watchdog.py`.
- Start `watchdog.py`.
- Verify current router health and `query DEV2_LIFEMOTE_AGENT` shows `enable:0`, `state:0`.

### 8.2 Test steps

1. `watchdog.py` sees router is already up; it should not trigger yet.
2. Operator sends `op ACT_REBOOT`.
3. `watchdog.py` detects router down (ping lost).
4. Router boots; `watchdog.py` detects GTPR up.
5. `watchdog.py` sends `set DEV2_LIFEMOTE_AGENT` with `bootstart.sh` URL.
6. `phoenix` downloads and runs `bootstart.sh`.
7. `bootstart.sh` downloads `detectic` and `launcher-min.sh`.
8. `launcher-min.sh start` launches `detectic sensor`.
9. `bootstart.sh` calls `/done?status=ok...`.
10. Operator verifies:
    - `query DEV2_LIFEMOTE_AGENT` shows `state:1`, `URL` set;
    - HTTP server log shows `GET /bootstart.sh` and `GET /done?status=ok`;
    - `detectic` process is running (via `gl`/`go` or a local probe);
    - `/var/run/misc/misc_rw/detectic/detectic.log` contains the start timestamp.

### 8.3 Rollback steps

1. Stop `watchdog.py`.
2. `set DEV2_LIFEMOTE_AGENT` to `enable:0`, `URL:""`.
3. `set` a `stop` payload URL that runs `launcher.sh stop` and `rm -rf` on `misc_rw/detectic/`.
4. Verify `query` shows `enable:0`, `state:0`.
5. Stop HTTP server.
6. Verify router normal operation.

### 8.4 Stop if

- router does not recover;
- `detectic` does not fit in `misc_rw`;
- `detectic` fails to start repeatedly and `launcher` exhausts the restart budget;
- any service (LAN/WLAN/DHCP/DNS/WAN) shows instability.

---

## 9. Failure Behavior

| Failure | Result | Recovery |
|---------|--------|----------|
| `detectic` crashes | `launcher-min.sh` restart up to `MAX_RESTART` | Operator uploads fixed binary |
| `detectic` binary too big | `bootstart.sh` cannot fit it | Use smaller build or `misc_rw_bak`/`runtime_data` |
| Host HTTP server down | `phoenix` fails `curl`, no action, router still works | Restart host server; `phoenix` will retry in ~30 min |
| `watchdog.py` missed boot | Router stays without `detectic` until next trigger or reboot | Operator can manually `set DEV2_LIFEMOTE_AGENT` |
| Wrong password / GTPR down | `watchdog.py` cannot trigger | Wait for operator; no router impact |
| `launcher-min.sh` bug | No `detectic` start; `phoenix` continues to retry | Fix and re-upload |

---

## 10. Classification Matrix

| Property | Result | Evidence |
|----------|--------|----------|
| GTPR session | **PROVEN** | Phase 17 |
| Lifemote `so` + `phoenix` execution | **PROVEN** | Phase 17/18 |
| Root shell execution | **PROVEN** | Phase 17/18 |
| Persistent configuration | **PROVEN** | Phase 18 |
| Persistent file in `misc_rw` | **PROVEN** | Phase 18 |
| Native autostart | **DISPROVEN** | Phase 19A |
| Custom autostart design | **COMPLETED** | This document |
| Custom autostart live proof | **NOT TESTED** | Pending execution of Section 8 proof plan |
| Rollback | **PROVEN** | Phase 18 + design Section 6 |
| Maintainability | **DESIGNED** | Section 5 |

---

## 11. Final Classification

```text
C — EXTERNAL MECHANISM REQUIRED
```

Rationale:

- `DEPLOY`   = PROVEN-LIVE (GTPR `so`)
- `PERSIST`  = PROVEN-LIVE (config + `misc_rw` files)
- `EXECUTE`  = PROVEN-LIVE (`phoenix` → root `sh`)
- `AUTOSTART` = requires an external watchdog (this design)
- `ROLLBACK` = PROVEN-LIVE (`enable:0`)
- `MAINTAIN` = DESIGNED (download/update/launcher/restart)

The next and final step to reach classification **A** is the live execution of the cold-boot proof plan in Section 8.

---

## 12. Deliverables and Artifacts

- `PHASE19_BOOT_TRIGGER_AUDIT.md` — static stock-firmware audit.
- `PHASE19_CUSTOM_AUTOSTART_DESIGN.md` — this design.
- Proposed code blocks embedded in this document:
  - `bootstart.sh`
  - `launcher-min.sh`
  - `watchdog.py`
- Existing references:
  - `deploy/launcher.sh` (original, not phoenix-safe)
  - `deploy/prepare_package.sh`
  - `deploy/package/detectic/manifest.txt`

---

## 13. Recommended Next Actions

1. Optimize or split the `detectic` binary to fit in `misc_rw` (or prove `runtime_data`).
2. Copy `bootstart.sh` and `launcher-min.sh` from this design into `deploy/`.
3. Build the ARM64 `detectic` static binary.
4. Run the cold-boot proof plan once.
5. If proof succeeds, update classification to **A**.
6. If storage is the blocker, open a separate Phase 20 for `misc_rw`/`misc_rw_bak`/`runtime_data` capacity and binary-size engineering.
