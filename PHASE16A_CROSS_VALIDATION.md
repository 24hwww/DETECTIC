# PHASE 16A — Cross-Validation: Agent A vs. Agent B

## TP-Link EX520V Resident Execution Research

**Date:** 2026-08-24
**Method:** Cross-check of `PHASE16A_WEB_EXECUTION_AUDIT.md` (Agent A) and `PHASE16A_FIRMWARE_EXECUTION_AUDIT.md` (Agent B), plus additional repo evidence (`admin_shell_access.md`, `m4_3_execution_paths.md`). No live tests in this cross-validation.

---

## 1. Convergent findings (both agents agree)

| Finding | Agent A evidence | Agent B evidence | Status |
|---------|------------------|------------------|--------|
| `util_exec_system` / `popen` in `libcutil.so` is the shared execution primitive | E-16A-WEB-16 | E-16A-FIRM-01 | **PROVEN-STATIC** |
| `libcmm.so` is the data-model engine that routes GTPR/GDPR operations to apply handlers | E-16A-WEB-10 | E-16A-FIRM-28 | **PROVEN-STATIC** |
| No user-supplied command path from `misc_rw` exists | E-16A-WEB-04/16 | E-16A-FIRM-02/04/05 | **PROVEN-STATIC** |
| `rcS` does not execute user/writable scripts; `rcS_hook` is orphaned | — | E-16A-FIRM-34 | **PROVEN-STATIC** |
| `procd`/`ubus` compiled but not used as init | — | E-16A-FIRM-35 | **PROVEN-STATIC** |
| `crond` not started, no `crontabs` | — | E-16A-FIRM-37 | **PROVEN-STATIC** |
| `lua5.1` not a user-reachable execution channel | E-16A-WEB-21/22/23 | E-16A-FIRM-38 | **PROVEN-STATIC** |
| Cloud/CWMP/USPP channels require credentials and are not arbitrary | E-16A-WEB-24/25/26/29/30 | E-16A-FIRM-25/26 | **PROVEN-STATIC** |
| Firmware upgrade is signed/verified image-only | E-16A-WEB-20 | E-16A-FIRM-29/30 | **PROVEN-STATIC** |
| `httpd` → `libgdpr.so` → `libcmm.so` is the authenticated GTPR path | E-16A-WEB-02/09 | — | **PROVEN-STATIC** |
| `oal_setTelnetd` → `telnetd -p %d` is the data-model daemon-launch chain | — | E-16A-FIRM-28 | **PROVEN-STATIC** |

---

## 2. Contradictions and gaps resolved

### 2.1 Agent A found `X_TTNET_CONF_SHELL`; Agent B did not discuss it

- **Resolution:** `X_TTNET_CONF_SHELL` is in `_rootfs/web/js/oid_str.js` and in `strings _rootfs/lib/libcmm.so`. Agent B did not report it because its scan prioritized *persistent artifact* → *execution* chains and did not enumerate every data-model object. The object is nevertheless part of the same `libcmm.so` data model that Agent B analyzed.
- **Status:** `X_TTNET_CONF_SHELL` is an **additional, plausible HTTP→execution candidate** on top of the `telnetd`/`dropbear` path highlighted by Agent B. It should be ranked alongside the Lifemote/Phoenix and Telnet chains.

### 2.2 Agent B found `misc_rw` size hard blocker; Agent A did not model it

- **Resolution:** `misc_rw` is 6 MiB in MTD layout but only **1,144 KiB usable** in practice (Phase 14.1). The stock Detectic binary is ~1.26 MiB. Agent A's `X_TTNET_CONF_SHELL` and SoftwareModules candidates can run *commands*, but the resulting process still needs a place to live. If the binary cannot persist, DEPLOY of the current binary is blocked.
- **Mitigation:** A minimal resident agent could fit if it is < 1,144 KiB, or if it is stored in `misc_rw_bak` (space not verified). Alternatively, a shell-based or `phoenix`-fetched payload can run from `/tmp` (non-persistent) and rely on the URL persisting in `misc_rw` instead.

### 2.3 Neither Agent A nor B analyzed the Lifemote/Phoenix path in depth

- **Resolution:** `admin_shell_access.md` and `m4_3_execution_paths.md` already contain a **controlled live test** of `DEV2_LIFEMOTE_AGENT` → `phoenix.sh` → `curl` download → `sh /tmp/lifemote_cpe_daemon.sh`. This is a concrete `HTTP→script execution` chain. It is separate from `X_TTNET_CONF_SHELL` and `DEV2_TELNET_CFG`.
- **Status:** This is the **strongest live-proven** candidate. It should be included in the final ranked list.

### 2.4 Agent A says execution likely; Agent B says no legitimate path

- **Reconciliation:**
  - Agent A focused on the *HTTP→`libcmm`→`util_exec_system`* surface and found it promising.
  - Agent B focused on *persistence/autostart* and found no safe, no-NAND-write, autonomous path.
  - Both can be true: there are HTTP-reachable execution primitives, but using them safely and persistently is not yet proven.

---

## 3. Consolidated candidate ranking

| Rank | Candidate | Evidence | DEPLOY | EXECUTE | PERSIST | AUTOSTART | Maint | Revers | Safety | Status |
|------|-----------|----------|--------|---------|---------|-----------|-------|--------|--------|--------|
| 1 | **Lifemote Agent (`DEV2_LIFEMOTE_AGENT`) → `phoenix.sh` → remote shell script** | `m4_3_execution_paths.md`, `admin_shell_access.md`, `phoenix.sh`, `libcmm.so` strings | 4 | 5 | 3 | 2 | 3 | 4 | 3 | **PROVEN-LIVE for execute** |
| 2 | **`X_TTNET_CONF_SHELL` / `Device.X_TTNET.Configuration.Shell` via GTPR `so`** | E-16A-WEB-06/10/13/14/16, E-16A-FIRM-01/28 | 2 | 4 | 1 | 0 | 2 | 3 | 3 | **STRONG-CANDIDATE, unproven live** |
| 3 | **`DEV2_TELNET_CFG` → `oal_setTelnetd` → `telnetd` (interactive shell after login)** | E-16A-FIRM-28, `m4_3_execution_paths.md` | 3 | 3 | 4 | 4 | 2 | 2 | 2 | **PROVEN-LIVE for telnet enablement** |
| 4 | **`Device.SoftwareModules.ExecutionUnit.{i}.Run()`** | E-16A-WEB-12/17/18 | 1 | 2 | 1 | 0 | 2 | 3 | 3 | **POSSIBLE, likely signed/verified** |
| 5 | **`backupcfg.bin` → `dm_restoreCfg` → `telnetd`/`dropbear`** | E-16A-FIRM-18/31/32/33 | 2 | 3 | 5 | 4 | 2 | 2 | 1 | **STRONG-CANDIDATE but requires live NAND write** |
| 6 | `crond` (not started) | E-16A-FIRM-37 | — | — | — | — | — | — | — | **DISPROVEN as autostart** |
| 7 | `rcS_hook` / `procd` / `ubus` | E-16A-FIRM-34/35 | — | — | — | — | — | — | — | **DISPROVEN / NOT ENABLED** |
| 8 | `LD_PRELOAD` / `.so` plugins | E-16A-FIRM-20/21/22/24 | — | — | — | — | — | — | — | **DISPROVEN** |

**Scale (0–5):** 0 = none, 5 = fully proven / ideal. Persistence/autostart scores are low because no candidate has been shown to start the agent automatically after a cold boot in a way that survives without operator action.

---

## 4. Selected safest next experiment

### 4.1 Why Lifemote/Phoenix is the safest starting point

1. It is **already proven live** in the repository (`admin_shell_access.md`).
2. It does **not** require modifying `backupcfg.bin` or writing UBI directly; it uses a documented data-model `so` operation and a stock shell script.
3. It is **fully reversible** by setting `DEV2_LIFEMOTE_AGENT.enable=0`, clearing the URL, and killing `phoenix.sh`.
4. The payload can be **benign**: a script that only writes a heartbeat file to `/var/tmp` and exits.

### 4.2 Proposed controlled test

**Objective:** Verify that `DEV2_LIFEMOTE_AGENT` can execute a benign script that places a file in `misc_rw` and starts a minimal resident watchdog, without enabling Telnet or changing passwords.

**Steps:**
1. Host a benign script on a LAN HTTP server:
   ```sh
   #!/bin/sh
   mkdir -p /var/run/misc/misc_rw/detectic
   echo "DETECTIC_LIFEMOTE_PROBE" > /var/run/misc/misc_rw/detectic/probe_$(date +%s)
   ```
2. Send `gtpr so DEV2_LIFEMOTE_AGENT {"enable":"1","URL":"http://<host>/probe.sh"}`.
3. Wait for `phoenix.sh` to poll/ download (or trigger via `ACT_SAVE_CFG`).
4. Verify the probe file exists in `misc_rw`.
5. Revert: `so DEV2_LIFEMOTE_AGENT {"enable":"0","URL":""}`.

**Stop conditions:** any WAN/WLAN/DHCP/DNS/management UI degradation.

---

## 5. Evidence index (E-16A-CROSS-xx)

| ID | Description | Source | Classification |
|----|-------------|--------|----------------|
| E-16A-CROSS-01 | Convergence: `util_exec_system` is the shared primitive | `PHASE16A_WEB_EXECUTION_AUDIT.md` E-16A-WEB-16 + `PHASE16A_FIRMWARE_EXECUTION_AUDIT.md` E-16A-FIRM-01 | PROVEN-STATIC |
| E-16A-CROSS-02 | Convergence: `libcmm.so` routes GTPR to apply handlers | E-16A-WEB-10 + E-16A-FIRM-28 | PROVEN-STATIC |
| E-16A-CROSS-03 | `X_TTNET_CONF_SHELL` is an additional candidate not fully covered by Agent B | E-16A-WEB-06/13/14 | STRONG-CANDIDATE |
| E-16A-CROSS-04 | `misc_rw` usable space is ~1,144 KiB, smaller than current Detectic binary | `PHASE14.1_MIMO_EXECUTION_PATH_AUDIT.md` | PROVEN-STATIC |
| E-16A-CROSS-05 | Lifemote/Phoenix live test exists in repo evidence | `admin_shell_access.md`, `m4_3_execution_paths.md` | PROVEN-LIVE (prior test) |
| E-16A-CROSS-06 | Ranked candidate list consolidates web, firmware, and prior repo evidence | This file | SYNTHESIS |
| E-16A-CROSS-07 | Recommended next experiment is Lifemote/Phoenix benign probe | This file | RECOMMENDATION |

---

## 6. Conclusion

The two independent research tracks are **consistent** where they overlap and **complementary** where they differ. The most important conclusion is that the EX520 does expose HTTP-reachable data-model objects that can reach the `util_exec_system` / `popen` primitives, but none of them have been shown to be a **safe, autonomous, persistent, and reversible** application platform for Detectic. The Lifemote/Phoenix chain is the only one with live-proven execution and should be the first controlled experiment. The `X_TTNET_CONF_SHELL` and `backupcfg→telnetd` candidates should be treated as secondary, higher-risk probes.
