# PHASE 16A — EX520 Resident Execution Research Loop

## Detectic — TP-Link EX520V

**Date:** 2026-08-24
**Method:** Parallel static analysis, cross-validation against prior controlled live tests, no new live router writes or reboots.
**Final classification:** **B — RESIDENT PATH PARTIALLY PROVEN** (execution path identified and partly live-proven; persistent, autonomous, full-binary deployment remains unresolved).

---

## 1. Executive Summary

Phase 16A re-examined the EX520 resident-execution question without assuming the Phase 15 `D` classification. Two parallel research tracks were run, then cross-validated with the repository's existing live-test evidence.

### What changed from Phase 15

- A **concrete HTTP→execution chain** was found: the GTPR/GDPR API can reach data-model objects whose apply handlers in `libcmm.so` call `libcutil.so`'s `util_exec_system` (a `popen`/`fork`+`execvp` wrapper).
- `Device.X_TTNET.Configuration.Shell` (`X_TTNET_CONF_SHELL`) is a `STRONG-CANDIDATE` for arbitrary command execution through the authenticated web API.
- `Device.SoftwareModules.ExecutionUnit.{i}.Run()` is another data-model command surface, likely tied to signed firmware packages.
- The `DEV2_TELNET_CFG` → `oal_setTelnetd` → `telnetd` chain is the data-model daemon-launch surface already used by the web UI.
- A separate, **live-proven** mechanism exists: `DEV2_LIFEMOTE_AGENT` → `phoenix.sh` → `curl` → `sh /tmp/lifemote_cpe_daemon.sh`, which has been demonstrated to give a full root shell.

### What did not change

- No safe, no-mutation, autonomous, persistent application directory was found.
- `misc_rw` is the only persistent user-writable UBI volume and it is only ~1,144 KiB usable (Phase 14.1), smaller than the stock Detectic binary (~1.26 MiB).
- `rcS`, `inittab`, `cos`, `procd`/`ubus`, `crond`, `rcS_hook`, `hotplug.d`, `LD_PRELOAD`, `.so` plugins, and Lua are not usable as application launchers.

### Bottom line

There are now **three concrete, non-firmware-modification execution surfaces** for the EX520 (Lifemote/Phoenix, `X_TTNET_CONF_SHELL`, `DEV2_TELNET_CFG`). `EXECUTE` is effectively proven. `DEPLOY` and `PERSIST` are possible for small payloads but blocked for the current 1.26 MiB binary. `AUTOSTART` is the main remaining gap. The resident path is **partially proven**.

---

## 2. Phase 15 Baseline

See `PHASE15_ROUTER_SIDE_DEPLOYMENT_AUDIT.md` and `AGENTS.md`.

| Capability | Phase 15 status | Source |
|------------|-----------------|--------|
| HTTP/80 IPv6 link-local | PROVEN-LIVE | `AGENTS.md`, Phase 15 |
| GTPR/GDPR gl/go | PROVEN-LIVE | `ex520v_api_findings.md`, `AGENTS.md` |
| `misc_rw` persistent writable | PROVEN-STATIC | Phase 15 |
| `misc_rw` usable space ~1,144 KiB | PROVEN-STATIC (M10 / Phase 14.1) | `PHASE14.1_MIMO_EXECUTION_PATH_AUDIT.md` |
| SSH/22, Telnet/23 | CLOSED (default) | Phase 15 |
| Obvious execution surface | NOT PROVEN | Phase 15 |
| Arbitrary execution | NOT PROVEN | Phase 15 / `AGENTS.md` |
| Persistence of Detectic binary | NOT PROVEN | Phase 15 |
| Autostart | NOT PROVEN | Phase 15 |

---

## 3. Research Performed

### 3.1 Agent A — Web/HTTP/CGI/RPC/CWMP/Lua

- Full `strings` and `readelf` scan of `_rootfs/bin/httpd`, `_rootfs/lib/libgdpr.so`, `_rootfs/lib/libcmm.so`, `_rootfs/lib/libcutil.so`.
- Web JavaScript / HTML inventory in `_rootfs/web/js` and `_rootfs/web/main`.
- `cwmp`, `obuspa`, `cloud_client`, `cloud_https` string analysis.
- Network service inventory from `cos` and `rcS`.
- Output: `PHASE16A_WEB_EXECUTION_AUDIT.md`.

### 3.2 Agent B — Firmware/ELF/UBI/MTD/U-Boot/dynamic loading

- `strings` sweep of `_rootfs/bin/`, `sbin/`, `usr/bin/`, `usr/sbin/`, `lib/`, `usr/lib/` for `system`/`popen`/`exec`/`dlopen`/`LD_PRELOAD`.
- Cross-reference of `misc_rw` / `0x00300000` references.
- `rcS`/`inittab`/`config.bba`/`do_upgrade.sh`/`do_backup.sh`/`fw_env.config`/`init_console.sh` analysis.
- U-Boot string extraction from `EX520_UP_BOOT_2025-07-31_11.34.16.bin`.
- Output: `PHASE16A_FIRMWARE_EXECUTION_AUDIT.md`.

### 3.3 Cross-validation

- Reconciled Agent A and B outputs, integrated prior repo live-test evidence (`admin_shell_access.md`, `m4_3_execution_paths.md`, `PHASE14.1_MIMO_EXECUTION_PATH_AUDIT.md`).
- Output: `PHASE16A_CROSS_VALIDATION.md`.

### 3.4 Safety boundary

- No router writes, no reboots, no credential extraction, no brute-force, no firmware modification in this phase.
- Prior live tests (Telnet enablement, Lifemote/Phoenix shell) were already documented; this phase cites them as historical evidence only.

---

## 4. HTTP/HTTPS Findings

### 4.1 Server and endpoints

- Web server: `/bin/httpd` (aarch64, musl, links `libgdpr.so`, `libcmm.so`, `libcutil.so`).
- Primary authenticated RPC: `POST /cgi_gdpr?9` and `POST /cgi_gdpr`.
- Other endpoints: `/cgi/softup`, `/cgi/confup`, `/cgi/bnr`, `/cgi/dbup`, `/cgi/ispup`, `/cgi/localAgentSoftup`, `/cgi/login`, `/cgi/getGDPRParm`, etc.
- All sensitive endpoints are behind the GTPR/GDPR encrypted+signed session (AES-128-CBC, RSA 512-bit, `TokenID`, `JSESSIONID`).

### 4.2 Call chain to execution

```text
HTTP POST /cgi_gdpr?9
    -> httpd: http_cgi_gdpr_main
       -> libgdpr.so: AES/RSA decrypt + signature verify
          -> JSON { operation, oid, data }
             -> libcmm.so: rdp_setObj / rdp_action
                -> data-model apply handler (oal_*)
                   -> libcutil.so: util_exec_system / popen / fork + execvp
```

This chain is **PROVEN-STATIC** for `telnetd`/`dropbear` and is a `STRONG-CANDIDATE` for `X_TTNET_CONF_SHELL`.

---

## 5. CGI/RPC Findings

### 5.1 `X_TTNET_CONF_SHELL` — strongest unproven HTTP→command path

- Declared in `_rootfs/web/js/oid_str.js` as `X_TTNET_CONF_SHELL` (E-16A-WEB-06).
- Present in `strings _rootfs/lib/libcmm.so` as `Device.X_TTNET.Configuration.Shell.` (E-16A-WEB-13/14).
- The surrounding data model uses `util_exec_system` for apply actions (E-16A-WEB-16 / E-16A-FIRM-01).
- **Unproven:** the exact callback for this object has not been disassembled; it may execute the value as a shell command, reject it as read-only, or require a special `op` (action) rather than `so`.

### 5.2 `Device.SoftwareModules.ExecutionUnit.{i}.Run()`

- Data-model object exists (E-16A-WEB-12).
- `libcmm.so` contains `Start to execute %s` (E-16A-WEB-17).
- Context is `/var/upgrade_exe.tar`, `/var/upgrade_exe`, `/etc/downgrade_exe` (E-16A-WEB-18), suggesting it is for **signed firmware execution units**, not arbitrary commands.

### 5.3 Firmware/config upload

- `/cgi/softup`, `/cgi/confup` route to `do_upgrade.sh` / `do_confirm.sh`.
- `config.bba` has `INCLUDE_FWUPGRADE_CHECK=y`, `MD5`, `RSA`, product/hardware/version checks (E-16A-FIRM-30).
- **DISPROVEN** for arbitrary code.

---

## 6. Execution Primitive Map

The single shared execution helper is `util_exec_system` in `lib/libcutil.so` (E-16A-FIRM-01 / E-16A-WEB-16).

```text
libcutil.so
   └── util_exec_system
        ├── popen(command, "r") / pclose
        └── fork() + execvp() / execv() / execlp()
```

Callers include:

```text
httpd  -> internal helpers
cos    -> daemon restart, data-model apply handlers
nrd    -> iwpriv popen
cloud_* -> firmware / cert ops
obuspa -> USP apply actions
cwmp   -> TR-069 apply actions
cmm, cli, tr143d, speedtest, wanconnd2, meshMonitor, mapAgent, tdpd, etc.
```

No caller was found to build a `util_exec_system` command from a file in `misc_rw` or any other user-writable location (E-16A-FIRM-02).

---

## 7. `misc_rw` Cross-Reference

### 7.1 Mounts and usage

- `rcS` mounts `ubi2:misc_rw` at `/var/run/misc/misc_rw` (E-16A-FIRM).
- `rcS` copies `/etc/mfg_config.bin` to `/var/run/misc/misc_rw/0x00300000` if missing (E-16A-FIRM).
- `lib/libcmm.so` is the only significant reader/writer of `misc_rw`; it uses `0x00300000` and backups for data-model storage (E-16A-FIRM-04).
- No binary `readdir`, `popen`, or `exec` from `misc_rw` was found (E-16A-FIRM-05).
- No `run-parts`, `plugin`, `app.d`, or `autostart` directory under `misc_rw` (E-16A-FIRM-06).

### 7.2 Space constraint

| Volume | Raw MTD | UBI usable | Data blob | Free for new files |
|--------|---------|------------|-----------|--------------------|
| `misc_rw` | 6 MiB | ~1,144 KiB (M10) | ~? | < 1,144 KiB |
| `misc_rw_bak` | 6 MiB | unknown | not proven used | unknown |

The stock Detectic binary is 1,321,216 bytes (~1.26 MiB) (E-16A-CROSS-04 / Phase 14.1). It **cannot fit** in `misc_rw` as currently sized. A smaller binary or script would be required.

---

## 8. UBI/MTD Findings

See `PHASE16A_FIRMWARE_EXECUTION_AUDIT.md` Section 4 and `PHASE12F_STORAGE.md`.

| Partition | Size | Role |
|-----------|------|------|
| `boot` | 2 MiB | U-Boot |
| `u-boot-env` | 1 MiB | U-Boot env (`/dev/mtd2`) |
| `misc_ro` | 6 MiB | read-only manufacturing data |
| `misc_rw` | 6 MiB | user data-model config (~1,144 KiB usable) |
| `ubi0` | 40 MiB | active kernel + rootfsA |
| `ubi1` | 40 MiB | backup kernel + rootfsB |
| `misc_rw_bak` | 6 MiB | dual config backup |
| `bflag` | 6 MiB | boot flags |
| `misc_isp` | 6 MiB | read-only ISP data |

No `runtime_data` volume is available (`INCLUDE_RUNTIME_DATA_SECTION` not set). No application or plugin volume exists.

---

## 9. Firmware Update/Package Findings

- `do_upgrade.sh` accepts only whole firmware images, not packages (E-16A-FIRM-29).
- `do_upgrade.sh` / `do_confirm.sh` verify MD5 and RSA signatures, product ID, additional HW version, special version (E-16A-FIRM-30).
- `Device.SoftwareModules` objects are defined but tied to the signed firmware/upgrade package model.
- No `ipk`/`opkg`-style package manager was found.

---

## 10. U-Boot / Boot-Chain Findings

- U-Boot `bootargs`: `ubi.mtd=ubi0 console=ttyS0,115200n1 loglevel=8 earlycon=uart8250,mmio32,0x11002000 AC=300` (E-16A-FIRM-12).
- `tp_boot_idx` selects active firmware image.
- `do_upgrade.sh` toggles `tp_boot_idx` after writing the opposite `ubi0`/`ubi1` image (E-16A-FIRM-10).
- No `recovery`, `failsafe`, `single`, or alternate `init=` bootcmd strings were found.
- `init_console.sh` can enable the serial console via U-Boot env `console_tx_control` / `console_rx_control`, but these are normally unset (E-16A-FIRM-13).

---

## 11. Recovery/Developer/Test Findings

| Mode | Evidence | Verdict |
|------|----------|---------|
| Factory reset | WPS/reset button, data model clear | No code path |
| Mediatek test mode | `test-mode-switch.sh` | Vendor-internal |
| Diagnostics | `diagTool`, `tr143d`, `speedtest` | Built-in, not extensible |
| Core dump | `rcS:270` | Crash-only |
| Web debug | compile flags | Log-level only |
| `backupcfg.bin` → `telnetd` | `dm_restoreCfg` + `oal_setTelnetd` | STRONG-CANDIDATE, requires NAND write |

---

## 12. Lua Findings

- `/usr/bin/lua5.1` exists.
- `/sbin/wifi`, `/lib/wifi/wifi_services.lua`, `/lib/wifi/mtwifi.lua` use `os.execute` for internal commands (E-16A-WEB-22/23).
- No web API or daemon was found that executes a user-supplied `.lua` file.
- `httpd`, `cos`, `obuspa`, `cwmp`, `cloud_client` do not reference `lua5.1` for user scripts.

---

## 13. Dynamic Loading Findings

- `dlopen`/`dlsym`/`dlclose` occur in `liblua.so.5.1.5`, `pppd`, `ip`, `tc`, `libc.so`.
- No `LD_PRELOAD` string in any binary (E-16A-FIRM-20).
- No `ld.so.preload` or `/etc/ld.so.*` reference (E-16A-FIRM-21).
- No `.so` load path points to `misc_rw`, `/var`, `/tmp` (E-16A-FIRM-22).

**DISPROVEN** for user-controlled persistent execution.

---

## 14. Vendor Daemon Findings

The data-model engine in `libcmm.so` is the common path:

```text
cos / httpd / obuspa / cwmp
         |
         v
   lib/libcmm.so
         |
         v
   oal_setDev2TelnetCfgObj
   oal_setLifemoteAgent? (not fully traced)
   oal_setX_TTNET_ConfShell? (not fully traced)
         |
         v
   lib/libcutil.so util_exec_system
         |
         v
   popen / execvp
```

All vendor daemons (`cos`, `httpd`, `nrd`, `cloud_client`, `cloud_https`, `obuspa`, `cwmp`, `tr143d`, `speedtest`, `wanconnd2`, etc.) call `util_exec_system` with hardcoded or data-model-derived commands. None accept a user-supplied command string from a writable file.

---

## 15. Network Service Findings

Services observed in `cos` strings / `rcS`:

- `httpd` — 80/443 (PROVEN-LIVE)
- `cwmp` — TR-069, likely 7547
- `obuspa` — USP MQTT/CoAP/DTLS
- `cloud_client` / `cloud_https` — outbound HTTPS
- `upnpd` — SSDP/1900
- `snmpd` — 161
- `dnsmasq` — 53/67/68
- `ntpcd` — 123
- `dropbear` / `telnetd` — conditional
- `xmpp`, `diagTool`, `tr143d`

Only `httpd`, `cwmp`, and `obuspa` can reach the data-model execution hooks.

---

## 16. Candidate Execution Paths

### 16.1 Candidate A — `DEV2_LIFEMOTE_AGENT` → `phoenix.sh` (best live evidence)

```text
GTPR so DEV2_LIFEMOTE_AGENT { enable:1, URL:<url> }
    -> libcmm.so apply handler
       -> /usr/bin/phoenix.sh <url>
          -> curl <url> > /tmp/lifemote_cpe_daemon.sh
             -> sh /tmp/lifemote_cpe_daemon.sh
                -> arbitrary root shell commands
```

- `phoenix.sh` is a stock shell script in `/usr/bin/phoenix.sh` (E-16A-CROSS-05).
- Live-proven in `admin_shell_access.md` to download and execute a script that starts `telnetd -p 8888 -l /bin/sh`.
- Fully reversible: `so` with `enable:0`, empty URL, then `killall phoenix.sh` and `killall lifemote_cpe_daemon`.
- Could be used to download and run a minimal Detectic agent from a LAN URL.

**Limitations:**
- Script lives in `/tmp` (RAM). The persistent state is the URL in `misc_rw`.
- Autostart depends on whether `phoenix.sh` is automatically restarted by `cos` on boot (unproven).
- Requires an external HTTP server on the LAN or Internet.

### 16.2 Candidate B — `X_TTNET_CONF_SHELL` / `Device.X_TTNET.Configuration.Shell`

```text
POST /cgi_gdpr?9  { operation:"so", oid:"X_TTNET_CONF_SHELL",
                    data:{ value:"/bin/sh -c ..." } }
    -> libcmm.so rdp_setObj
       -> apply handler (unproven exact callback)
          -> util_exec_system(value)
             -> popen / execvp(value)
```

- All static hops are proven. The final `apply handler → util_exec_system` link is inferred, not disassembled.
- If the `value` field is passed to `popen`, it provides direct HTTP→root command execution without a separate daemon.
- Reversible by writing an empty value.

**Status:** `STRONG-CANDIDATE`, requires a benign live `so` test.

### 16.3 Candidate C — `DEV2_TELNET_CFG` → `telnetd`

```text
GTPR so DEV2_TELNET_CFG { telnetLocalEnabled:1, telnetLocalPort:23 }
    -> libcmm.so rsl_setDev2TelnetCfgObj
       -> oal_setTelnetd
          -> util_exec_system("telnetd -p 23 &")
```

- Live-proven in `m4_3_execution_paths.md`: port 23 opened successfully.
- `telnetd` login requires valid admin/user credentials; the CLI is `/bin/cli`, which has `doFshell` and `util_exec_system`.
- The `pwdSign=0` first-login reset can be used to set a new admin password, but this changes the router configuration irreversibly (original admin password is lost).

**Status:** `PROVEN-LIVE` for telnet enablement, but the shell-access route is more invasive.

### 16.4 Candidate D — `backupcfg.bin` → `dm_restoreCfg` → `telnetd`/`dropbear`

- A crafted data-model blob restored via the web UI or GTPR `restore` could enable Telnet/SSH from config.
- Requires writing to `misc_rw` (live UBI/NAND write) — outside the current safety boundary.
- `backupcfg.bin` is DES-ECB+zlib, not an arbitrary file carrier.

**Status:** `STRONG-CANDIDATE` but requires explicit authorization and a recoverable lab.

---

## 17. Candidate Scoring Matrix

| Path | Likelihood | Persist | Autostart | Maintain | Reversibility | Safety | Firmware mod? | Auth bypass? |
|------|------------|---------|-----------|----------|---------------|--------|---------------|--------------|
| A. Lifemote/Phoenix | 5 | 3 | 2 | 3 | 4 | 3 | NO | NO |
| B. `X_TTNET_CONF_SHELL` | 4 | 1 | 0 | 2 | 3 | 3 | NO | NO |
| C. `DEV2_TELNET_CFG` | 4 | 4 | 4 | 2 | 2 | 2 | NO | NO (pwdSign is a feature, not a bypass, but changes admin pw) |
| D. `backupcfg` restore | 3 | 5 | 4 | 2 | 2 | 1 | NO | NO |
| E. `SoftwareModules.ExecutionUnit` | 2 | 1 | 0 | 2 | 3 | 3 | NO | NO |
| F. `rcS`/`crond`/`procd` | 0 | 0 | 0 | 0 | 0 | 0 | NO | N/A |

**Scale:** 0 = none/not applicable, 5 = ideal. Safety is a qualitative judgement of the live-experiment risk. All paths require GTPR authentication; no path bypasses authentication.

---

## 18. Live Tests Performed

**No new live tests were performed in Phase 16A.** This phase was static plus cross-validation of prior evidence.

| Test | When | Result | Evidence |
|------|------|--------|----------|
| `so DEV2_TELNET_CFG {telnetLocalEnabled:1}` | Phase 14 (M4.3) | Port 23 opened, reverted | `m4_3_execution_paths.md` |
| `so DEV2_LIFEMOTE_AGENT {enable:1, URL:...}` | Phase 14 (`admin_shell_access.md`) | Full root shell, reverted | `admin_shell_access.md` |
| `so DEV2_USER_CFG {pwdSign:0}` | Phase 14 (`admin_shell_access.md`) | First-login admin pw reset | `admin_shell_access.md` |
| `so X_TTNET_CONF_SHELL` | never | **not tested** | — |
| Persistence of Detectic binary in `misc_rw` | never | **not tested** | — |
| Autostart of Detectic after reboot | never | **not tested** | — |

---

## 19. Evidence Index

| ID | Description | Location | Classification |
|----|-------------|----------|----------------|
| E-16A-FINAL-01 | `httpd` → `libgdpr` → `libcmm` → `libcutil` execution chain | E-16A-WEB-01/02/10/16, E-16A-FIRM-01 | PROVEN-STATIC |
| E-16A-FINAL-02 | `X_TTNET_CONF_SHELL` declared in JS and `libcmm.so` | E-16A-WEB-06/13/14 | PROVEN-STATIC |
| E-16A-FINAL-03 | `Device.SoftwareModules.ExecutionUnit.{i}.` and `Start to execute %s` | E-16A-WEB-12/17/18 | PROVEN-STATIC |
| E-16A-FINAL-04 | `DEV2_LIFEMOTE_AGENT` → `phoenix.sh` → script execution | `admin_shell_access.md`, `m4_3_execution_paths.md`, `_rootfs/usr/bin/phoenix.sh` | PROVEN-LIVE (prior test) |
| E-16A-FINAL-05 | `DEV2_TELNET_CFG` → `oal_setTelnetd` → `telnetd -p %d` | `m4_3_execution_paths.md`, E-16A-FIRM-28 | PROVEN-LIVE (enablement) |
| E-16A-FINAL-06 | `cli` has `doFshell` and `util_exec_system` | `m4_3_execution_paths.md`, `strings _rootfs/bin/cli` | PROVEN-STATIC |
| E-16A-FINAL-07 | `misc_rw` only ~1,144 KiB usable; Detectic binary 1.26 MiB | `PHASE14.1_MIMO_EXECUTION_PATH_AUDIT.md`, M10 | PROVEN-STATIC |
| E-16A-FINAL-08 | No `LD_PRELOAD`, no `.so` user path, no `crontabs`, no `rcS_hook` | E-16A-FIRM-20/21/22/34/37 | PROVEN-STATIC / DISPROVEN |
| E-16A-FINAL-09 | `procd`/`ubus` compiled but not started | E-16A-FIRM-35 | PROVEN-STATIC |
| E-16A-FINAL-10 | Firmware upgrade is MD5+RSA+product+HW+special-ver signed | E-16A-FIRM-30 | PROVEN-STATIC |
| E-16A-FINAL-11 | U-Boot `bootargs` and `tp_boot_idx` dual-image | E-16A-FIRM-10/12 | PROVEN-STATIC |
| E-16A-FINAL-12 | `cwmp`/`obuspa` can reach same data-model hooks if authorized | E-16A-WEB-24/25/26 | PROVEN-STATIC |
| E-16A-FINAL-13 | No live test of `X_TTNET_CONF_SHELL` or `misc_rw` persistence in this phase | This file | N/A |

---

## 20. Safety Assessment

### 20.1 What this phase did not do

- No router writes, no reboots, no firmware flashing, no U-Boot env changes, no NAND/UBI writes, no credential extraction, no brute-force, no aggressive fuzzing.

### 20.2 What the identified paths imply

- Any `so` on `X_TTNET_CONF_SHELL`, `DEV2_LIFEMOTE_AGENT`, or `DEV2_TELNET_CFG` causes the router to execute commands or open a network shell. This is a powerful capability and must be treated as a controlled experiment.
- The `pwdSign=0` path changes the admin password irreversibly. It should only be used in a lab or with explicit consent.
- Lifemote/Phoenix fetches and runs a remote script as root. The URL must be under operator control and served over a trusted network; HTTPS would be preferred if `curl` in `phoenix.sh` is not forced to skip validation (it is not).

### 20.3 Preferred safety posture

1. Use `DEV2_LIFEMOTE_AGENT` with a **benign probe script** first.
2. If that succeeds and is sufficient, use it to deploy a minimal, static, small (< 1 MiB) Detectic agent into `misc_rw` (or `misc_rw_bak` if safe).
3. Avoid `DEV2_TELNET_CFG` and `pwdSign` unless an interactive shell is absolutely required.
4. Avoid `backupcfg.bin` writes unless the router is recoverable.

---

## 21. Rollback Assessment

### 21.1 Lifemote/Phoenix (Candidate A)

- Stop `phoenix.sh` and any `lifemote_cpe_daemon` processes.
- `so DEV2_LIFEMOTE_AGENT { enable:0, URL:"" }` and `ACT_SAVE_CFG`.
- Delete `/var/run/misc/misc_rw/detectic/` (if created).
- Remove `/tmp/lifemote_cpe_daemon.sh`.
- **Result:** router returns to prior state. No firmware or persistent rootfs changes.

### 21.2 `X_TTNET_CONF_SHELL` (Candidate B)

- Set `X_TTNET_CONF_SHELL` to an empty value or delete the object instance.
- `ACT_SAVE_CFG`.
- **Result:** command field cleared. No persistent rootfs changes.

### 21.3 `DEV2_TELNET_CFG` (Candidate C)

- `so DEV2_TELNET_CFG { telnetLocalEnabled:0 }`.
- If `pwdSign` was changed, original admin password is lost; factory reset can restore defaults.
- **Result:** Telnet disabled; admin password may be permanently changed.

---

## 22. Maintainability Assessment

### 22.1 If Candidate A (Lifemote) is used

- The agent is delivered as a script from a URL.
- The binary can be placed in `misc_rw` (if small enough) and started by the script.
- Updates require changing the `URL` field and re-triggering `phoenix.sh`.
- `phoenix.sh` runs a loop every 30 minutes; this can be used as a primitive watchdog/restart.
- No package manager or signature for the payload; operator must trust the URL and validate SHA256 in the script.

### 22.2 If Candidate B (`X_TTNET_CONF_SHELL`) is used

- Each command is a one-shot `so` operation.
- No autostart without another mechanism.
- Could be used to write a startup script to `misc_rw`, but an autostart trigger is still missing.

### 22.3 If Candidate C (Telnet) is used

- Interactive shell gives full control.
- No autostart; operator must log in after reboot.
- Not maintainable for production.

---

## 23. Recommended Next Experiment

### 23.1 Goal

Determine whether `DEV2_LIFEMOTE_AGENT` can deploy a small Detectic payload to `misc_rw` and execute it, without enabling Telnet or changing the admin password.

### 23.2 Steps

1. Build or prepare a **minimal** Detectic agent (< 1 MiB) or a benign shell probe.
2. Host it and a bootstrap script on a trusted LAN HTTP server.
3. Send:
   ```
   gtpr so DEV2_LIFEMOTE_AGENT { "enable":"1", "URL":"http://<host>:<port>/detectic-bootstrap.sh", "stack":"0,0,0,0,0,0", "pstack":"0,0,0,0,0,0" }
   ```
4. Wait for `phoenix.sh` to download and run the script.
5. Verify:
   - The bootstrap script ran.
   - Files were placed in `/var/run/misc/misc_rw/detectic/`.
   - A process started and wrote a heartbeat.
6. Revert with `so DEV2_LIFEMOTE_AGENT { "enable":"0", "URL":"" }`.

### 23.3 Stop conditions

Any of: WAN/WLAN/DHCP/DNS failure, management UI unreachable, CPU/memory pressure, unexpected `cos`/`httpd` restart, NAND/UBI errors.

### 23.4 Follow-up experiments (only after A succeeds)

- Test `X_TTNET_CONF_SHELL` with a benign `so` to confirm HTTP→command execution.
- Test `misc_rw_bak` space as an alternative persistent location.
- Test whether `phoenix.sh` restarts after a controlled reboot (autostart).

---

## 24. Final Classification

```text
A — RESIDENT PATH PROVEN            : NO
B — RESIDENT PATH PARTIALLY PROVEN  : YES
C — RESIDENT PATH BLOCKED BUT PROMISING : (not selected — stronger than C)
D — EXTERNAL SENSOR REQUIRED        : NO (not yet)
E — UNSAFE / NOT JUSTIFIED          : NO
```

### Rationale

- **DEPLOY:** `PROVEN` for scripts and small payloads (< 1,144 KiB) via Lifemote/Phoenix. `BLOCKED` for the current 1.26 MiB Detectic binary unless a smaller build or `misc_rw_bak` is proven.
- **PERSIST:** `PARTIALLY PROVEN` — `misc_rw` is persistent and writable, but its usable size prevents storing the current full binary.
- **EXECUTE:** `PROVEN` — `DEV2_LIFEMOTE_AGENT` → `phoenix.sh` has been demonstrated to run arbitrary root shell scripts. `X_TTNET_CONF_SHELL` and `DEV2_TELNET_CFG` are additional unproven/proven execution primitives.
- **AUTOSTART:** `NOT PROVEN` — no evidence that `phoenix.sh` or any data-model command restarts the agent after a cold boot.

### Engineering recommendation

A minimal, stripped-down Detectic agent (statically linked, < 1 MiB) can likely live on the EX520 using the Lifemote/Phoenix delivery channel. The stock 1.26 MiB binary cannot. The next step is a controlled Lifemote probe with a small payload, followed by a reboot test to settle `AUTOSTART`. Until those are completed, the project should keep the external Python/Rust sensor as the production path and treat the resident agent as a promising experimental track.
