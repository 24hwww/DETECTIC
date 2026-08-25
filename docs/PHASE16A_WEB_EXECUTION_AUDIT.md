# PHASE 16A — Web / HTTP / HTTPS / RPC / CWMP / USP Execution Audit

## Detectic — TP-Link EX520V

**Date:** 2026-08-24 (static analysis)  
**Method:** Static analysis of the extracted `_rootfs/` only. No live traffic, no router writes, no reboot, no credential extraction, no brute-force, no firmware modification.  
**Agent:** Agent A, Research Track A  
**Scope:** Find a *legitimate, reproducible, reversible* path from an external HTTP/HTTPS request to process/code execution on the EX520, without modifying the router.

---

## 1. Executive summary

- The HTTP/HTTPS management plane is **active and authenticated** through the TP-Link GTPR/GDPR API (`/cgi/getGDPRParm`, `/cgi_gdpr?9`, `/cgi_gdpr`, `gl`/`go`/`gs`/`so` operations). This was already **PROVEN-LIVE** in Phase 14/15.
- Static analysis of `httpd`, `libcmm.so`, `libcutil.so`, `libgdpr.so`, `obuspa`, `cwmp` and the web JavaScript shows **one clearly promising execution surface** that originates from an authenticated HTTP request:
  - `POST /cgi_gdpr?9` with `operation:"so"` / `operation:"gs"` / `operation:"go"` reaches `libcmm.so` data-model routines.
  - The data model defines `Device.X_TTNET.Configuration.Shell.` and `Device.SoftwareModules.ExecutionUnit.{i}.`.
  - `libcmm.so` and `libcutil.so` contain the `util_exec_system` / `popen` / `fork`+`execvp` primitives.
- I cannot **prove** statically that writing `X_TTNET.Configuration.Shell` or invoking `SoftwareModules.ExecutionUnit.{i}` causes an arbitrary command to run. The execution hook is in the compiled data-model layer, and the exact callback semantics need a benign live `so`/`op` test.
- All other examined paths (firmware upload, diagnostic CGI, backup/restore, cloud JSON-RPC, Lua, CWMP Download/Upload, USP MQTT/CoAP without credentials) are either **signed/restricted**, **not user-arbitrary**, or **require out-of-scope credentials**.
- **Conclusion:** `X_TTNET.Configuration.Shell` / `Device.SoftwareModules.ExecutionUnit` via the authenticated GTPR `/cgi_gdpr` API is the **STRONG-CANDIDATE** for a legitimate HTTP→execution path. A safe, authorized `so`/`op` probe is the recommended next experiment. If the live test fails, the Phase 15 `D — EXTERNAL SENSOR REQUIRED` classification remains in force.

---

## 2. HTTP/HTTPS endpoint inventory with risk classification

### 2.1 Server-side handlers

The web server is `/bin/httpd` (ELF aarch64, links `libtrk.so`, `libcmm.so`, `libcutil.so`, `libcJSON.so`, `libgdpr.so`, `libssl.so.1.1`, `libcrypto.so.1.1`). Source files referenced in `httpd` strings:

- `httpd` source files: `src/http_cgi.c`, `src/http_cgi_gdpr.c`, `src/http_rpm_softup.c` (E-16A-WEB-01)
- Handler functions: `http_cgi_main`, `http_cgi_gdpr_main`, `http_rpm_softup`, `http_rpm_softburn`, `handleGLRequestGDPR`, `handleGSRequestGDPR`, `http_cgi_json` (E-16A-WEB-02)

### 2.2 CGI / RPC endpoints found in `httpd` strings

| Endpoint | Visible evidence | Static risk | Classification |
|----------|------------------|-------------|----------------|
| `/` | `web/index.htm`, login | Low | PROVEN-STATIC (read-only landing) |
| `/cgi/getGDPRParm` | `httpd` strings, `AGENTS.md` | Low (public RSA params) | PROVEN-LIVE |
| `/cgi_gdpr` `/cgi_gdpr?9` | `httpd` strings; `web/js/proxy.js:62-65`, `820`; `web/js/gdprProxy.js:62`, `802` | **High** (authenticated RPC to data model) | STRONG-CANDIDATE path |
| `/cgi/login` | `httpd` strings | Low | PROVEN-STATIC |
| `/cgi/log` `/cgi/route` | `web/js/proxy.js:64-65` | Low (log/event router) | PROVEN-STATIC |
| `/cgi/softup` `/cgi/softburn` `/cgi/localAgentSoftup` `/cgi/localMeshUpgrade` `/cgi/localMeshsoftburn` | `httpd` strings; firmware helpers `do_upgrade.sh` / `firmware.sh` (Phase 15) | High if signature bypassed; otherwise medium (signed) | PROVEN-STATIC, **not arbitrary** |
| `/cgi/confup` `/cgi/bnr` `/cgi/dbup` `/cgi/iqosbnr` `/cgi/ispup` `/cgi/lteispup` | `httpd` strings | High if parser bug; otherwise config-only | POSSIBLE file/dm action, no proven exec |
| `/cgi/conf.bin` `/cgi/ansi` `/cgi/auth` `/cgi/setPwd` `/cgi/wanBlock` `/cgi/openvpn` `/cgi/https` | `httpd` strings | Low-Medium | PROVEN-STATIC |
| `/cgi/getParm` `/cgi/getTokenc` `/cgi/getBusy` `/cgi/clearBusy` `/cgi/getBindStatus` `/cgi/checkCloudConn` `/cgi/getEwebUrl` | `httpd` strings | Low | PROVEN-STATIC |
| `/cgi/localAgentSoftup` `localAgentSoftupProxy` | `httpd` strings; curl uploads | Medium (file upload, runs curl) | PROVEN-STATIC, not arbitrary exec |
| `/cgi/%sV%d%s%s` | `httpd` strings | Unknown (format-string endpoint) | UNKNOWN, not reachable | PROVEN-STATIC (pattern only) |
| `/cgi/e` | `httpd` strings | Unknown / short alias | UNKNOWN |

(E-16A-WEB-03)

### 2.3 Web UI / JavaScript endpoints

- `web/js/proxy.js:1-9` defines `ACT_GET/SET/GL/GS/OP/CGI/SIG` and the `$.dm.Proxy` that posts all data-model set/get operations to `/cgi_gdpr?9` (E-16A-WEB-04).
- `web/js/proxy.js:848-878` maps `$.dm.set` → `operation:"so"`, `$.dm.get` → `operation:"go"`, `$.dm.getList` → `operation:"gl"`, `$.dm.getSubList` → `operation:"gs"` (E-16A-WEB-05).
- `web/js/oid_str.js:2188` declares `var X_TTNET_CONF_SHELL = "X_TTNET_CONF_SHELL"` (E-16A-WEB-06).
- `web/main/tr369.htm` exposes a TR-369/USP MQTT configuration page (E-16A-WEB-07).

---

## 3. CGI/RPC handler analysis and call chains

### 3.1 `httpd` → `cgi_gdpr` → GTPR/GDPR decrypt

```
HTTP POST /cgi_gdpr?9
    -> httpd: http_cgi_gdpr_main (src/http_cgi_gdpr.c)
       -> libgdpr.so: gdpr_decrypt, token/seq/signature validation
          -> JSON { "operation": "so", "oid": "...", "data": {...} }
             -> libcmm.so: rdp_setObj / rdp_action / dm_setObj
                -> libcutil.so: util_exec_system / util_exec_systemExt
```

- `httpd` strings contain `http_cgi_gdpr_main`, `sendJsonObjGDPR`, `buildAndSendJsonGDPR`, `handleGLRequestGDPR`, `handleGSRequestGDPR` (E-16A-WEB-02).
- `libgdpr.so` is a pure AES/RSA helper; no exec primitives (E-16A-WEB-08).
- `web/js/proxy.js:138-162` and `web/js/proxy.js:183-199` encrypt the body and decrypt the response with AES-CBC, preserving the `TokenID` header and `JSESSIONID` cookie (E-16A-WEB-09).

### 3.2 Data model dispatcher (`libcmm.so`)

- `libcmm.so` symbols include `dm_setObj`, `rdp_setObj`, `rdp_setObjStruct`, `rdp_action`, `dm_getObj` (E-16A-WEB-10).
- It references `util_exec_system` (from `libcutil.so`) for external process launch (E-16A-WEB-11).
- `libcmm.so` defines the vendor tree:
  - `Device.SoftwareModules.`
  - `Device.SoftwareModules.ExecEnv.{i}.`
  - `Device.SoftwareModules.DeploymentUnit.{i}.`
  - `Device.SoftwareModules.ExecutionUnit.{i}.` (E-16A-WEB-12)
  - `Device.X_TTNET.Configuration.Shell.` (E-16A-WEB-13)
  - `X_TTNET_CONF_SHELL` (E-16A-WEB-14)
- `cos` also uses `util_exec_system` and calls `rdp_action`, `rdp_setObj` (E-16A-WEB-15).

### 3.3 Execution primitives in `libcutil.so`

- `strings lib/libcutil.so` shows the implementation in `src/cutil_exec.c`:
  - `util_exec_system` / `util_exec_systemExt`
  - `popen`, `pclose`, `pipe` handling
  - `args null(%p)(%p)` / `pipe(%s) format fail` / `rpipe(%s) open fail` / `System pCall termination, exit status = %d`
  - `killall %s` (E-16A-WEB-16)
- This is the **only** shared execution primitive used by `httpd`, `cos`, `obuspa`, `cwmp`, `cloud_client`, `cloud_https`, `wanconnd2`, `meshMonitor`, `mapAgent`, `tdpd`.

---

## 4. Execution primitive map

### 4.1 Candidate 1 — GTPR `so` set on `X_TTNET.Configuration.Shell`

```text
External HTTP POST /cgi_gdpr?9
    JSON body: { "operation":"so", "oid":"X_TTNET_CONF_SHELL",
                 "data":{ "value":"/bin/sh /var/tmp/harmless.sh" },
                 "stack":"0,0,0,0,0,0", "pstack":"0,0,0,0,0,0" }
    |
    v
httpd -> libgdpr (decrypt/verify signature) -> httpd dispatcher
    |
    v
handleGSRequestGDPR / http_cgi_gdpr_main
    |
    v
rdp_setObj("X_TTNET_CONF_SHELL") in libcmm.so
    |
    v
set-hook for Device.X_TTNET.Configuration.Shell. (unproven callback)
    |
    v
util_exec_system(value) in libcutil.so
    |
    v
popen(value) or fork + execvp(value)
```

**Status:** `STRONG-CANDIDATE` — every hop above is **PROVEN-STATIC** except the set-hook itself, which is compiled but not disassembled. The final `popen`/`execvp` primitive is **PROVEN-STATIC**.

### 4.2 Candidate 2 — `Device.SoftwareModules.ExecutionUnit.{i}` `Run()`

```text
CWMP/USP SetParameterValues / AddObject / Operate
    on Device.SoftwareModules.ExecutionUnit.{i}.Command / .Run()
    |
    v
cwmp or obuspa -> libcmm.so -> rdp_action / rdp_setObj
    |
    v
Start to execute %s in libcmm.so
    |
    v
util_exec_system(command)
```

- `libcmm.so` contains `Device.SoftwareModules.ExecutionUnit.{i}.` and the string `Start to execute %s` (near `Device.SoftwareModules.` and upgrade-exe handling) (E-16A-WEB-17).
- The surrounding strings mention `/var/upgrade_exe.tar`, `tar -zxf /var/upgrade_exe.tar -C /var`, `/var/upgrade_exe`, `/etc/downgrade_exe` (E-16A-WEB-18). This indicates the `Start to execute %s` primitive is tied to **firmware package execution units**, not arbitrary shell.
- **Status:** `POSSIBLE` / `UNKNOWN` for arbitrary user commands; `PROVEN-STATIC` that it runs signed/verified package executables.

### 4.3 Candidate 3 — HTTP firmware / config upload

```text
POST /cgi/softup or /cgi/confup
    |
    v
httpd -> http_rpm_softup / http_rpm_softburn
    |
    v
file upload to /var/tmp/...
    |
    v
do_upgrade.sh / do_confirm.sh (RSA/MD5 verified in Phase 15)
```

- `httpd` strings include `curl -F "filename=@%s" ... /cgi/localAgentSoftup` and `popen` calls (E-16A-WEB-19).
- Phase 15 found `do_upgrade.sh` performs signature checks (`firmware.sh`, `rdp_verifyFirmware`) (E-16A-WEB-20).
- **Status:** `DISPROVEN` for arbitrary execution; `PROVEN-STATIC` for vendor-signed payloads only.

### 4.4 Disproven or low-value routes

| Input | Handler | Primitive | Verdict |
|-------|---------|-----------|---------|
| `/cgi/log`, `/cgi/route` | log/event handler | none | DISPROVEN |
| `/cgi/info`, `/cgi/getParm` | read-only DM get | none | DISPROVEN |
| `/cgi/ansi`, `/cgi/auth`, `/cgi/login` | auth/session | none | DISPROVEN |
| `/main/diagnostic.htm` `ACT_OP_DIAG_*` | `diagTool` message | CMSG_DIAG_TOOL_COMMAND, no user command string | PROVEN-STATIC, **not arbitrary** |
| Cloud JSON-RPC `method/params` | `cloud_client` / `cloud_https` | `util_exec_system` for cloud firmware/config only | PROVEN-STATIC, not arbitrary |

---

## 5. Lua findings

- `/usr/bin/lua5.1` is present; `sbin/wifi` is a Lua script; `jshn` is an ELF helper, not a Lua interpreter (E-16A-WEB-21).
- Real vendor Lua modules under `/lib/wifi/` call `os.execute`:
  - `lib/wifi/wifi_services.lua:10-188` uses `os.execute("rm -rf ...")`, `os.execute("miniupnpd ...")`, `os.execute("killall ...")` etc. (E-16A-WEB-22)
  - `lib/wifi/mtwifi.lua:30-225` uses `os.execute("brctl ...")`, `os.execute("ifconfig ...")`, `os.execute("/etc/init.d/wpad start")` etc. (E-16A-WEB-23)
- These scripts are invoked by `/sbin/wifi` or the `wifi` init helpers, **not** from `httpd` or the web API.
- No web endpoint was found that accepts a user-supplied `.lua` file or a Lua expression.
- `obuspa`, `cwmp`, `httpd`, `cloud_client` and `cos` do **not** link or call `lua5.1` in a way that is visible in their dynamic symbols or strings.

**Verdict:** `PROVEN-STATIC` that Lua is an internal runtime; `DISPROVEN` that it is a user-reachable execution channel.

---

## 6. Cloud / CWMP / TR-069 / USP findings

- `/bin/cwmp` is a full TR-069 agent: `SetParameterValues`, `GetParameterValues`, `AddObject`, `DeleteObject`, `Download`, `Upload`, `Reboot`, `FactoryReset` (E-16A-WEB-24).
- `/bin/cwmp` checks `TR069 ACS is not in parameter %s's access list` and uses digest/Basic auth (E-16A-WEB-25).
- `/usr/bin/obuspa` is a USP (TR-369) agent: supports MQTT (`libmosquitto.so.1`), CoAP (`COAP_SERVER_Start`), DTLS, WebSocket, STOMP (E-16A-WEB-26).
- `obuspa` is started by `cos` (`cos` strings: `obuspa &`) (E-16A-WEB-27).
- `obuspa` defaults to `/usr/local/var/obuspa/usp.db`; references `tauc-mqtt-broker.tplinkcloud.com` and `/tpuc/tr369controller` (E-16A-WEB-28).
- `cloud_client` and `bin/cloud_https` use `https://n-device-api.tplinkcloud.com`, JSON-RPC (`src/cloud_jsonRpc.c`) and `util_exec_system` (E-16A-WEB-29).
- `cloud_https` handles cloud push; `cloud_client` handles bind/unbind/firmware queries (E-16A-WEB-30).
- None of these binaries expose an unauthenticated command-injection path in the static rootfs. All cloud/CWMP/USP channels require TP-Link cloud credentials, ACS credentials, or controller certificates.

**Relevant execution surface:** Because `cwmp` and `obuspa` both operate on the same `libcmm.so` data model, an authorized remote controller could set/operate `Device.X_TTNET.Configuration.Shell.` or `Device.SoftwareModules.ExecutionUnit.{i}.` through these protocols — the same underlying hook as the local `/cgi_gdpr` `so` operation. This is `POSSIBLE` but depends on credentials and ACS/USP reachability.

---

## 7. Network service findings

`cos` starts the following network-relevant daemons (from `bin/cos` strings and `rcS`):

- `httpd` — HTTP/80, HTTPS/443 (PROVEN-LIVE)
- `cwmp` — TR-069 ACS, likely TCP/7547
- `obuspa` — USP MQTT (TLS/8883), CoAP (5683/5684), DTLS
- `cloud_client` / `cloud_https` — outbound HTTPS to `n-device-api.tplinkcloud.com`
- `upnpd` — SSDP/1900, UPnP IGD
- `snmpd` — SNMP/161
- `dnsmasq` — DNS/53 + DHCP/67/68 (from `cos` strings: `dnsmasq is not exist!`)
- `dhcpd`/`dhcpc`
- `ntpcd` — NTP/123
- `dropbear` / `telnetd` — compiled, started conditionally (Phase 15: not currently reachable)
- `xmpp` — XMPP/5222 (binary present, status unknown)
- `diagTool`, `tr143d` — diagnostic daemons
- `qoeStatisticsHandler`, `wanconnd2`

(E-16A-WEB-31)

**Relevance to execution:**
- `cwmp` and `obuspa` are the only services beyond 80/443 that can reach the same data-model `Shell`/`ExecutionUnit` execution hooks.
- `upnpd`, `snmpd`, `xmpp` were briefly inspected; no static path to `util_exec_system` from user UDP/UPnP input was identified.

---

## 8. Candidate execution paths

| # | Path | Likelihood | Persistence | Autostart | Maintainability | Reversibility | Router safety | Notes |
|---|------|------------|-------------|-----------|-----------------|---------------|---------------|-------|
| A | **GTPR `so` on `X_TTNET_CONF_SHELL` / `Device.X_TTNET.Configuration.Shell`** | 4 | 2 | 0 | 2 | 4 | 3 | Best static candidate; needs benign live `so` test. Reversible by setting empty value. |
| B | **USP `Operate` on `Device.SoftwareModules.ExecutionUnit.{i}.Run`** | 3 | 2 | 0 | 2 | 4 | 3 | Standard TR-369 command surface; likely signed/verified. |
| C | **CWMP `SetParameterValues` on `X_TTNET.Configuration.Shell`** | 3 | 2 | 0 | 2 | 4 | 3 | Requires ACS credentials; same data model as A. |
| D | **Firmware/config upload `/cgi/softup` `/cgi/confup`** | 1 | 0 | 0 | 1 | 2 | 2 | Signature enforced. |
| E | **Cloud JSON-RPC command channel** | 1 | 0 | 0 | 1 | 2 | 2 | No arbitrary command found; requires cloud credentials. |
| F | **Diagnostic endpoints (`/cgi/ansi`, `/cgi/log`, `/cgi/route`)** | 0 | 0 | 0 | 0 | 5 | 5 | No command primitives. |
| G | **Lua script injection** | 0 | 0 | 0 | 0 | 5 | 5 | No user-supplied Lua path. |

**Likelihood** scale: 0 = none, 5 = certain. Persistence/Autostart/Maintainability are low because these are one-shot command/parameter operations, not installable services.

---

## 9. Evidence index

| ID | Description | Source / location | Classification |
|----|-------------|-------------------|----------------|
| E-16A-WEB-01 | `httpd` source files `src/http_cgi.c`, `src/http_cgi_gdpr.c`, `src/http_rpm_softup.c` | `strings _rootfs/bin/httpd` | PROVEN-STATIC |
| E-16A-WEB-02 | `http_cgi_gdpr_main`, `handleGLRequestGDPR`, `handleGSRequestGDPR` | `strings _rootfs/bin/httpd` | PROVEN-STATIC |
| E-16A-WEB-03 | `/cgi/*` endpoint list including `/cgi_gdpr`, `/cgi/softup`, `/cgi/confup`, `/cgi/bnr`, etc. | `strings _rootfs/bin/httpd` | PROVEN-STATIC |
| E-16A-WEB-04 | `$.dm.Proxy` and `$.tpAjax` wrapping `/cgi_gdpr?9` | `_rootfs/web/js/proxy.js:1-200` | PROVEN-STATIC |
| E-16A-WEB-05 | `$.dm.set` → `operation:"so"`, `$.dm.get` → `operation:"go"` | `_rootfs/web/js/proxy.js:848-878` | PROVEN-STATIC |
| E-16A-WEB-06 | `X_TTNET_CONF_SHELL` declared in JS | `_rootfs/web/js/oid_str.js:2188` | PROVEN-STATIC |
| E-16A-WEB-07 | TR-369 / USP MQTT configuration page | `_rootfs/web/main/tr369.htm` | PROVEN-STATIC |
| E-16A-WEB-08 | `libgdpr.so` is AES/RSA helper, no exec primitives | `strings _rootfs/lib/libgdpr.so` | PROVEN-STATIC |
| E-16A-WEB-09 | AES-CBC body encryption and `TokenID`/`JSESSIONID` handling | `_rootfs/web/js/proxy.js:138-199` | PROVEN-STATIC |
| E-16A-WEB-10 | `rdp_setObj`, `rdp_action`, `dm_setObj`, `dm_getObj` in `libcmm.so` | `readelf -s _rootfs/lib/libcmm.so` | PROVEN-STATIC |
| E-16A-WEB-11 | `util_exec_system` referenced by `httpd` | `readelf -d _rootfs/bin/httpd` + `strings _rootfs/lib/libcutil.so` | PROVEN-STATIC |
| E-16A-WEB-12 | `Device.SoftwareModules.ExecutionUnit.{i}.` in data model | `strings _rootfs/lib/libcmm.so` | PROVEN-STATIC |
| E-16A-WEB-13 | `Device.X_TTNET.Configuration.Shell.` in data model | `strings _rootfs/lib/libcmm.so` | PROVEN-STATIC |
| E-16A-WEB-14 | `X_TTNET_CONF_SHELL` token | `strings _rootfs/lib/libcmm.so` | PROVEN-STATIC |
| E-16A-WEB-15 | `cos` uses `util_exec_system`, `rdp_setObj`, `rdp_action` | `strings _rootfs/bin/cos` | PROVEN-STATIC |
| E-16A-WEB-16 | `util_exec_system` implementation using `popen` / `fork`+`execvp` | `strings _rootfs/lib/libcutil.so` | PROVEN-STATIC |
| E-16A-WEB-17 | `Start to execute %s` in `libcmm.so` | `strings _rootfs/lib/libcmm.so` | PROVEN-STATIC |
| E-16A-WEB-18 | `Start to execute %s` context: `/var/upgrade_exe.tar`, `/etc/downgrade_exe` | `strings _rootfs/lib/libcmm.so` | PROVEN-STATIC |
| E-16A-WEB-19 | `httpd` `popen` and `curl` upload paths for `localAgentSoftup` | `strings _rootfs/bin/httpd` | PROVEN-STATIC |
| E-16A-WEB-20 | Firmware signature verification via `rdp_verifyFirmware` / `do_upgrade.sh` | `PHASE15_ROUTER_SIDE_DEPLOYMENT_AUDIT.md` | PROVEN-STATIC (Phase 15) |
| E-16A-WEB-21 | `/usr/bin/lua5.1` and `/sbin/wifi` Lua script present; `jshn` is ELF | `file _rootfs/usr/bin/jshn`, `file _rootfs/sbin/wifi` | PROVEN-STATIC |
| E-16A-WEB-22 | `wifi_services.lua` calls `os.execute` | `_rootfs/lib/wifi/wifi_services.lua` | PROVEN-STATIC |
| E-16A-WEB-23 | `mtwifi.lua` calls `os.execute` | `_rootfs/lib/wifi/mtwifi.lua` | PROVEN-STATIC |
| E-16A-WEB-24 | `cwmp` implements full TR-069 RPC set | `strings _rootfs/bin/cwmp` | PROVEN-STATIC |
| E-16A-WEB-25 | `cwmp` parameter access list and digest auth | `strings _rootfs/bin/cwmp` | PROVEN-STATIC |
| E-16A-WEB-26 | `obuspa` supports MQTT, CoAP, DTLS, WebSocket, STOMP | `strings _rootfs/usr/bin/obuspa` | PROVEN-STATIC |
| E-16A-WEB-27 | `cos` starts `obuspa &` | `strings _rootfs/bin/cos` | PROVEN-STATIC |
| E-16A-WEB-28 | `obuspa` defaults: `/usr/local/var/obuspa/usp.db`, `tauc-mqtt-broker.tplinkcloud.com` | `strings _rootfs/usr/bin/obuspa` | PROVEN-STATIC |
| E-16A-WEB-29 | `cloud_client` / `cloud_https` use `n-device-api.tplinkcloud.com`, JSON-RPC | `strings _rootfs/bin/cloud_client`, `_rootfs/bin/cloud_https` | PROVEN-STATIC |
| E-16A-WEB-30 | `cloud_https` handles cloud push and uses `util_exec_system` | `strings _rootfs/bin/cloud_https` | PROVEN-STATIC |
| E-16A-WEB-31 | `cos` daemon start list (httpd, obuspa, cwmp, upnpd, snmpd, dropbear, telnetd, etc.) | `strings _rootfs/bin/cos` | PROVEN-STATIC |

---

## 10. Safety assessment

- No live traffic, router writes, reboots, credential extraction, or brute-force were performed.
- All evidence comes from the extracted `_rootfs/` (SquashFS/UBIFS) and the repository's documented `python/detectic_client.py`.
- No passwords, keys, session cookies, or ACS credentials were printed.
- The proposed next experiment (if authorized) is to send a **benign** `so`/`op` probe with a non-destructive command (e.g., `id` or `uptime`) and observe process spawning, then set the value back to empty.

---

## 11. Conclusion and recommended next experiment

### Conclusion

The static audit **did not prove** a fully functional external HTTP → arbitrary execution path, but it **did identify a concrete, plausible, legitimate candidate**:

> `Device.X_TTNET.Configuration.Shell.` (and, to a lesser extent, `Device.SoftwareModules.ExecutionUnit.{i}.`) is compiled into the `libcmm.so` data model, and the authenticated GTPR `/cgi_gdpr?9` endpoint is wired through `httpd` → `libgdpr` → `libcmm` → `libcutil` (`util_exec_system` / `popen`).

If the set/operate callback for `X_TTNET.Configuration.Shell` calls `util_exec_system` with the parameter value, then a single authenticated `POST /cgi_gdpr?9` is a reproducible, reversible HTTP-to-process-launch path. This matches the project's mission without requiring firmware modification.

All other investigated surfaces are either blocked by signatures, restricted to cloud/ACS credentials, or do not expose a command execution primitive from an external request.

### Recommended next experiment

1. Reuse `python/detectic_client.py` (or `src/gtpr.rs`) to authenticate to the live EX520 over the **IPv6 link-local HTTP path** (`fe80::...%enp2s0`).
2. Send a **benign** `so` (set) operation to `X_TTNET_CONF_SHELL` with a harmless payload such as `echo DETECTIC_PROBE > /var/tmp/detectic_probe_$(date +%s)` or `id > /var/tmp/detectic_probe`.
3. Check whether `/var/tmp/detectic_probe_*` is created and whether a new process is observed (this requires an additional shell/serial channel or a pre-arranged file-watch method).
4. If that fails, try `gs`/`gl`/`op` on `X_TTNET_CONF_SHELL` and `Device.SoftwareModules.ExecutionUnit.1.Run()`.
5. Reversibility: set `X_TTNET_CONF_SHELL` back to an empty string or `0`.
6. If the live probe **succeeds**, the `D — EXTERNAL SENSOR REQUIRED` classification should be re-evaluated in favor of an in-process, authenticated, data-model command path. If it **fails**, the Phase 15 classification remains in force.

**Do not proceed with destructive or unapproved commands. The test must be read-only or use a benign, non-persistent marker only.**
