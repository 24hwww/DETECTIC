# Remote Access Configuration Objects — TP-Link EX520V

> **Scope:** Every configuration object related to remote administration
> discovered on the stock firmware. No success is claimed without evidence.
> Firmware: `EX520V124101568249n_agc3000_0945460481` (MT7981).

Sources:
- `web/js/oid_str.js` (OID name registry, build flags)
- `etc/reduced_data_model.xml` (encrypted; accessed via `strings` when possible)
- `lib/libcmm.so` (RSL/OAL handlers, data-model paths)
- `etc/config.sdk` (build-time feature flags)
- `etc/default_config.xml` / `etc/mfg_config.bin` (factory defaults, encrypted)

---

## 1. Build flags (compile-time feature selection)

| Flag | Value | Location | Meaning |
|------|-------|----------|---------|
| `INCLUDE_SSH_ACCESS` | `0` | `web/js/oid_str.js:3796` | SSH UI/handler disabled in this build |
| `INCLUDE_WEB_TELNET` | `1` | `oid_str.js:2460` | Web-initiated telnet path compiled in |
| `INCLUDE_REMOTE_TELNET` | `1` | `oid_str.js:2462` | Remote telnet path compiled in |
| `INCLUDE_AGC_DIS_TELNETFULLRESET` | `1` | `oid_str.js:2410` | Aginet telnet disable-on-reset flag |
| `INCLUDE_TELNET_LOGIN_WAIT` | `1` | `oid_str.js:3042` | Telnet login wait behavior |
| `INCLUDE_WEB_TELNET_FOR_USER` | `0` | `oid_str.js:3794` | Web telnet not exposed to `user` role |
| `INCLUDE_SEC_ALLOW_APP_ENABLE_SSH` | `0` | `oid_str.js:4610` | App-initiated SSH allowance disabled |
| `CONFIG_PACKAGE_dropbear` | `y` | `etc/config.sdk:1489` | Dropbear present in image |
| `CONFIG_DEFAULT_dropbear` | `y` | `etc/config.sdk:75` | Dropbear default-enabled at build |

**Interpretation:** The image ships `dropbear` but the web flag `INCLUDE_SSH_ACCESS=0`
hides the SSH UI. Telnet paths are compiled in (`WEB_TELNET=1`, `REMOTE_TELNET=1`)
but gated by role/flag.

---

## 2. Data-model objects

### 2.1 `DEV2_SSH_CFG` — SSH (Dropbear) configuration

| Field | Value |
|-------|-------|
| OID constant | `DEV2_SSH_CFG = "DEV2_SSH_CFG"` | 
| Location | `web/js/oid_str.js:838` |
| Data-model path | `Device.X_TP_AppCfg.SSHCfg.` |
| Evidence | `strings libcmm.so | grep SSHCfg` → `Device.X_TP_AppCfg.SSHCfg.` and `DEV2_SSH_CFG` |
| Handlers | `oal_dropbearRestart`, `rsl_restartDropbear`, `rdp_restartDropbear` in `libcmm.so` |
| Runtime files | `/var/tmp/dropbear/config` (referenced in `libcmm.so`) |
| Related flags | `INCLUDE_SSH_ACCESS=0` (UI disabled), but handlers and binary present |

**What is known:** The object and its restart handlers exist. The exact field
names (e.g. `Enable`, `Port`, `Access`, `Lifetime`) are not extractable from
the static image because `reduced_data_model.xml` and `default_config.xml` are
encrypted (same `backupcfg.bin` key). No claim is made about the field set
without a live `gl` of `DEV2_SSH_CFG`.

**What is not claimed:** That setting this object will start Dropbear in this
build. `INCLUDE_SSH_ACCESS=0` suggests the apply handler may no-op or be
guarded. Requires live verification.

### 2.2 `DEV2_TELNET_CFG` — Telnet configuration

| Field | Value |
|-------|-------|
| OID constant | `DEV2_TELNET_CFG = "DEV2_TELNET_CFG"` |
| Location | `web/js/oid_str.js:876` |
| Data-model path | `Device.X_TP_AppCfg.TelnetCfg.` |
| Evidence | `strings libcmm.so | grep TelnetCfg` → `Device.X_TP_AppCfg.TelnetCfg.` and `DEV2_TELNET_CFG`; `grep TelnetCfg libcmm.so` also yields `rsl_initTelnetCfgObj`, `rsl_setDev2TelnetCfgObj`, `rsl_getDev2TelnetCfgObj`, `oal_setTelnetd`, `oal_app_setLocalTelnetAccess`, `oal_app_setRemoteTelnetAccess` |
| Command template | `telnetd -p %d &` in `libcmm.so` strings |
| Related flags | `INCLUDE_WEB_TELNET=1`, `INCLUDE_REMOTE_TELNET=1` |

**What is known:** Full handler chain exists:
`rsl_initTelnetCfgObj` → `rsl_setDev2TelnetCfgObj` → `oal_setTelnetd`
→ `telnetd -p %d &`, plus split local/remote access setters.

**What is not claimed:** Field names and valid values without a live `gl`.

### 2.3 Other remote-administration objects (candidates to inspect live)

| OID / path | Evidence |
|------------|----------|
| `DEV2_USER_CFG` | `strings httpd` and `lib/libcmm.so`; user management |
| `DEV2_HTTP_CFG` | `strings httpd`; HTTP management |
| `DEV2_X_TP_EASYMESH` | EasyMesh; not remote access |
| `Device.X_TP_AppCfg.TpAppCfg.` | Parent container of SSH/Telnet objects |

Only `DEV2_SSH_CFG` and `DEV2_TELNET_CFG` are directly tied to `dropbear`/`telnetd`
launch via `lib/libcmm.so` strings.

---

## 3. Binaries on the image

| Binary | Path | Evidence |
|--------|------|----------|
| `telnetd` | `/usr/sbin/telnetd` (BusyBox applet) + `telnetd -p %d &` in `libcmm.so` | `lib/libcmm.so` strings; BusyBox provides `telnetd` |
| `dropbear` | `/usr/bin/dropbear` → `dropbearmulti` | `usr/bin/dropbear -> dropbearmulti`; `config.sdk` has `CONFIG_PACKAGE_dropbear=y` |
| `dropbearmulti` | `/usr/bin/dropbearmulti` | Multi-call binary |
| `dropbearkey` | (via dropbearmulti) | Standard Dropbear |

Telnet is a BusyBox applet; Dropbear is a standalone `dropbearmulti`. Both
are present in the SquashFS image (read-only, not writable per constraints).

---

## 4. How to inspect live (no firmware modification)

With a GDPR session (requires only web credentials, no shell):

```bash
# 1. Authenticate and obtain TokenID (see ex520-network-map-gdpr.md)
# 2. Encrypted gl for each candidate OID:
#    operation = gl, oid = DEV2_TELNET_CFG  (and DEV2_SSH_CFG)
#    Body must be AES-128-CBC + RSA sign as in src/transport.rs

# Python (using the project's crypto):
#   from python.detectic_client import GtprClient
#   c = GtprClient("http://192.168.0.1", "user", "<REDACTED>")
#   c.connect()
#   print(c.gl("DEV2_TELNET_CFG"))
#   print(c.gl("DEV2_SSH_CFG"))

# Rust (using the new layered API):
#   use detectic::transport::GtprClient;
#   let mut t = GtprClient::new("http://192.168.0.1", "user", "pass");
#   t.connect().unwrap();
#   println!("{}", t.gl("DEV2_TELNET_CFG").unwrap());
```

If the `gl` returns `errorcode 9003` or `9804`, the object exists but the
caller lacks permission or the object is not listable via `gl` (try `go`).

**Expected shape** (inferred from `Telnet` handler `telnetd -p %d`):

```json
{
  "data": {
    "TELNETCFG": {
      "Enable": 1,
      "Port": 23,
      "LocalAccess": 1,
      "RemoteAccess": 0
    }
  }
}
```

**The exact field set must be read live — do not assume it.**

---

## 5. Write path (if a shell is needed for Detectic)

If live inspection shows the object is writable, a `go`/`set` with the same
encrypted envelope could enable the daemon:

```json
{
  "data": {"TELNETCFG": {"Enable": 1, "Port": 2323}},
  "operation": "go",
  "oid": "DEV2_TELNET_CFG"
}
```

This is the **only** legitimate configuration write path that could yield a
shell without modifying firmware (data-model apply → `oal_setTelnetd` →
`telnetd -p %d &`). It is **not claimed to work** until a live test succeeds.

---

## 6. What is NOT claimed

- That `DEV2_SSH_CFG` will start Dropbear when `INCLUDE_SSH_ACCESS=0`. Handlers
  exist, but the apply path may be gated.
- Field names, value ranges, or defaults without a live `gl`.
- That `backupcfg.bin` restore can enable these daemons (it would still require
  the unknown DES key; Phase 2 forbids using it as an exploit).
- That any of the above survives reboot without the `misc_rw` deployment
  (see `DEPLOYMENT_PATHS.md` §4).

---

## 7. References

- `web/js/oid_str.js` lines 338–3800 (build flags), 838/876 (OID constants)
- `etc/config.sdk` (dropbear package flags)
- `lib/libcmm.so` strings (`Device.X_TP_AppCfg.*`, `telnetd -p %d`, `dropbear`)
- `_rootfs/usr/bin/dropbear*`, `_rootfs/bin/busybox`
- `BACKUPCFG_ANALYSIS.md` §5 and `investigations/backupcfg/REPORT.md` §1
