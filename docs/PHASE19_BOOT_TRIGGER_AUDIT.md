# PHASE 19A — EX520V Stock Boot Trigger Audit

## TP-Link EX520V — Detectic Resident Path

**Date:** 2026-08-24
**Method:** Static analysis of extracted `_rootfs` boot scripts, `cos`/`libcmm.so` strings, `hotplug2` rules, `config.bba`, and live Phase 18 reboot evidence.
**Conclusion:** **NATIVE_AUTOSTART = NOT_AVAILABLE**

---

## 1. Executive Summary

The objective of this phase was to determine whether the **stock** EX520V firmware contains any boot, event, or service mechanism that can be influenced without modifying the firmware to launch a persistent Detectic process automatically after a cold boot.

After auditing the init chain, `cos` data-model startup, `hotplug2` events, `init.d` scripts, `cron`, `procd`/`ubus`, `mdev`, `watchdog`, and TP-Link-specific services, no such trigger was found.

The root cause of the Phase 18 autostart failure is now understood:

- `DEV2_LIFEMOTE_AGENT` is initialized by `rsl_initDev2LifemoteAgentObj` at boot.
- The `phoenix.sh` daemon is started only by `rsl_setDev2LifemoteAgentObj` in response to a GTPR `so` operation.
- The boot-time `init` path does not call `set` or apply a persistent `enable:1` state.
- Therefore, `enable:1` and `URL` survive in `misc_rw`, but `phoenix` does not run until an external `so` is sent.

---

## 2. Init Chain

### 2.1 `inittab`

```text
::sysinit:/etc/init.d/rcS
::askfirst:/sbin/getty -L ttyS0 115200 vt100
```

PID 1 runs `rcS` once at `sysinit`; then a serial console is the only other service. There is no `respawn` for user scripts and no `procd`.

### 2.2 `etc/init.d/rcS`

Full `rcS` was read and analyzed. Key operations:

- mounts `misc_ro`, `misc_rw`, `misc_rw_bak`, `runtime_data`, `misc_isp`;
- creates `/var/...` directories;
- loads kernel modules;
- sources `etc/init.d/rcS.model`;
- starts `cos &` (the data-model / web daemon);
- starts `cmmsyslogd &` (syslog);
- runs `udevtrigger &`;
- does **not** start `cron`, `procd`, `ubus`, `dropbear`, `telnetd`, or any user-service manager.

Relevant `rcS` final section:

```sh
# BBA default data model
if [ -n "$INCLUDE_MTD_TYPE_FS" ]; then
if [ ! -f /var/run/misc/misc_rw/0x00300000 ] ; then
    echo Warning: userconfig not exists, use manufacture config!!
    cp -v /etc/mfg_config.bin /var/run/misc/misc_rw/0x00300000
else
    echo userconfig exists, do nothing.
fi
fi

# ...

cos &
cmmsyslogd &
```

`rcS` does **not** source any file from `misc_rw` or any user-writable location. There is no `rcS_hook` or `rc.local` equivalent.

### 2.3 `etc/init.d/rcS.model`

```sh
#!/bin/sh
ifconfig eth0 up
ifconfig eth1 up
insmod /lib/modules/kmdir/kernel/drivers/net/mii.ko
# ... mknod for /dev/ttyUSB*, /dev/voip ...
```

This is model-specific low-level bring-up. No daemons, no event hooks.

### 2.4 `etc/init.d/rcS.openwrt-21.02.mtk`

Empty file in this firmware; not used.

### 2.5 `etc/init.d/firmware.sh`

Uses `START=15` and `USE_PROCD=1` syntax, but `procd` binary is **absent** and this script is **not invoked** by `rcS`. It appears to be a carry-over for `firmware` hotplug, not an active boot trigger.

### 2.6 `etc/init.d/init_console.sh`

Adjusts serial console tx/rx control from U-Boot env. Not a user hook.

---

## 3. `cos` / Data-Model Boot Behavior

### 3.1 `cos` strings

From `/bin/cos`:

- `cos_init`
- `dm_init`
- `dm_shmInit`
- `Init misc_isp error`
- `Start`
- `rsl_checkAndRestartDetectProcess`

From `/lib/libcmm.so`:

- `rsl_initDev2LifemoteAgentObj`
- `rsl_getDev2LifemoteAgentObj`
- `rsl_setDev2LifemoteAgentObj`
- `rsl_killLifemoteDeployerAndAgent`
- `DEV2_LIFEMOTE_AGENT`
- `DEV2_X_TP_LIFEMOTE_EXT`
- `Device.X_TP_LIFEMOTE_EXT.LifemoteAgent.`

### 3.2 Why `phoenix.sh` does not start at boot

The `rsl_*` naming convention (`init`, `get`, `set`) strongly indicates the data-model lifecycle:

```text
rsl_initDev2LifemoteAgentObj   -> called by cos at boot; loads config from 0x00300000
rsl_setDev2LifemoteAgentObj    -> called by cos on a GTPR so; applies changes and starts phoenix
rsl_getDev2LifemoteAgentObj    -> returns current state
rsl_killLifemoteDeployerAndAgent -> used to stop phoenix on enable:0
```

Phase 18 confirmed this live:

- After a `so`, `state` became `1` and `GET /phase18.sh` arrived from `phoenix`.
- After `op ACT_REBOOT`, `enable:1` and `URL` persisted, but `state` was `0` and no `GET` arrived.

Therefore, the only way to transition `state 0 -> 1` is a GTPR `so` on `DEV2_LIFEMOTE_AGENT`. `cos` boot does not perform this transition automatically.

---

## 4. `hotplug2` and Event Hooks

### 4.1 `hotplug-call` hardcodes `/etc/hotplug.d`

```sh
#!/bin/sh
export HOTPLUG_TYPE="$1"
. /lib/functions.sh
PATH=/bin:/sbin:/usr/bin:/usr/sbin
LOGNAME=root
USER=root
export PATH LOGNAME USER

[ \! -z "$1" -a -d /etc/hotplug.d/$1 ] && {
    echo "/etc/hotplug.d/$1"
    for script in $(ls /etc/hotplug.d/$1/* 2>&-); do (
        [ -f $script ] && . $script
    ); done
}
```

The `hotplug.d` directory is on the read-only rootfs. The call does not look at `/var/run/misc/misc_rw/hotplug.d` or any other writable path. We cannot add user scripts.

### 4.2 `hotplug2` rules

`etc/hotplug2.rules`:

- `button` subsystem: `exec kill -USR1 1`
- `platform` subsystem: `exec /sbin/hotplug-call %SUBSYSTEM%`
- `input`, `net`, `usb`, etc. are commented out.

`etc/hotplug2-init.rules`:

- `button` subsystem: `exec kill -USR1 1`

`etc/hotplug2-common.rules`:

- Device node creation for `/dev/`.
- `watchdog` device: `exec /sbin/watchdog -t 5 /dev/watchdog`

No user-writable hook is invoked. The `net` and `iface` `hotplug.d` scripts exist but the `hotplug2` framework does not dispatch to them in this firmware.

### 4.3 `hotplug.d` contents

Subsystems present:

- `button`
- `dhcp6c`
- `firewall`
- `ieee1394`
- `iface`
- `net`
- `usb`

All are read-only vendor scripts. None are user-callable.

### 4.4 `watchdog` device

`hotplug2` starts `/sbin/watchdog -t 5 /dev/watchdog` when the watchdog device appears. This is a kernel watchdog for `cos`, not a user-programmable service.

---

## 5. `cron`, `procd`, `ubus`, `mdev`

### 5.1 `cron`

No `crond`, `crontab`, or `/etc/cron*` files exist in the running firmware. `config.bba` does not define `INCLUDE_CRON`.

### 5.2 `procd` / `ubus`

- No `procd`, `ubusd`, or `ubus` binaries found in `_rootfs`.
- `firmware.sh` references `USE_PROCD=1` but is not used.
- `etc/config.sdk` contains procd strings (for SDK/toolchain) but not the firmware image.
- `rcS` does not start `procd`.

### 5.3 `mdev`

No `/etc/mdev.conf` and no `mdev` reference. Device node creation is handled by `hotplug2`.

---

## 6. `config.bba` Feature Audit

Relevant feature flags:

- `INCLUDE_LIFEMOTE=1` — the Lifemote agent is present.
- `INCLUDE_CLOUD`, `INCLUDE_CLOUD_V1`, `INCLUDE_CLOUD_V2`, `INCLUDE_AGINET_APP_V2` — cloud/Aginet app present but not a user hook.
- `INCLUDE_MTD_TYPE_FS` — enables `misc_rw`.
- `INCLUDE_DUAL_CONFIG` — enables `misc_rw_bak`.
- No `INCLUDE_PROCD`, `INCLUDE_UBUS`, `INCLUDE_CRON`, `INCLUDE_HOTPLUG_USER`, `INCLUDE_MDEV`.

---

## 7. Lifemote / Phoenix Details

### 7.1 `phoenix.sh` trigger path

`/usr/bin/phoenix.sh` is started by `rsl_setDev2LifemoteAgentObj` only. It:

- downloads the `URL` to `/tmp/lifemote_cpe_daemon.sh`;
- runs `sh /tmp/lifemote_cpe_daemon.sh &`;
- sleeps for `CHECK_INTERVAL` (default 1800s) and repeats.

### 7.2 `DEV2_LIFEMOTE_AGENT` data fields (known)

- `enable`
- `state`
- `URL`
- `stack`
- `pstack`

No `AutoStart`, `Schedule`, or `BootRun` field exists in the data model or the web UI. `DEV2_X_TP_LIFEMOTE_EXT` is a parent object with no additional configurable children usable for autostart.

### 7.3 `rsl_init` vs `rsl_set`

`rsl_init` is boot-time initialization. It loads persisted `enable:1` and `URL` but does not start `phoenix`. `rsl_set` is the apply path triggered by GTPR `so`. The autostart gap is that `rsl_set` is not called at boot.

---

## 8. Candidate Trigger Table

| Trigger | Process | Parent | Boot? | Root? | Persistent? | GTPR accessible? | Influence? | Execute Detectic? | Safe? | Rollback | Class |
|---------|---------|--------|-------|-------|-------------|------------------|------------|-------------------|-------|----------|-------|
| `rcS` | `init` | PID 1 | Yes | Yes | Read-only | No | No | No (no user hook) | N/A | N/A | X |
| `rcS.model` | `init` | PID 1 | Yes | Yes | Read-only | No | No | No | N/A | N/A | X |
| `cos` `rsl_init` | `cos` | `init` | Yes | Yes | Config in `misc_rw` | Indirect | No (does not apply) | No | N/A | N/A | X |
| `cos` `rsl_set` via `so` | `phoenix.sh` | `cos` | No (post-so) | Yes | Config in `misc_rw` | Yes | Yes (proven) | Yes (proven) | Yes | `enable:0` | B |
| `hotplug.d` `button` | `hotplug-call` | `hotplug2` | Event | Yes | Read-only | No | No | No | N/A | N/A | X |
| `hotplug.d` `iface` | `hotplug-call` | `cos`/network | Maybe | Yes | Read-only | No | No | No | N/A | N/A | X |
| `hotplug.d` `usb` | `hotplug-call` | `hotplug2` | Not called | Yes | Read-only | No | No | No | N/A | N/A | X |
| `cron` | none | — | No | — | — | No | No | No | N/A | N/A | X |
| `procd`/`init.d` | none | — | No | — | — | No | No | No | N/A | N/A | X |
| `watchdog` | `/sbin/watchdog` | `hotplug2` | Yes | Yes | No user hook | No | No | No | N/A | N/A | X |
| `udevtrigger` | `mdev`/kernel | `init` | Yes | Yes | No user hook | No | No | No | N/A | N/A | X |
| `cloud`/`aginet` | `cos` | `init` | Yes | Yes | Cloud config | No | No | No | N/A | N/A | X |
| `Reboot/WiFi/LED/Firewall Schedule` | `rsl_*_schedule` | `cos` | No | Yes | Config in `misc_rw` | Yes (so) | Partial (fixed actions) | No (no arbitrary command) | Yes | Clear schedules | X |

### Class key

- **A** = directly usable
- **B** = usable with small reversible adaptation (e.g., `so` after boot from an external source)
- **X** = impossible or irrelevant for our purpose

Only `rsl_set` is usable, and it is not a boot trigger; it is the result of an external `so`.

---

## 9. Live Evidence

From Phase 18:

- `query DEV2_LIFEMOTE_AGENT` before reboot:

```json
{ "enable": "1", "state": "1", "URL": "http://192.168.0.27:8080/phase18.sh" }
```

- `op ACT_REBOOT`:

```json
{ "success": true, "errorcode": 0 }
```

- `query DEV2_LIFEMOTE_AGENT` after reboot:

```json
{ "enable": "1", "state": "0", "URL": "http://192.168.0.27:8080/phase18.sh" }
```

- HTTP server observed **no** automatic `GET /phase18.sh` for 180s.

This is direct disproof of any native autostart.

---

## 10. Stop Condition

All reasonable stock-firmware avenues have been exhausted:

- PID 1 / `rcS` audited;
- `cos` boot `init` path traced;
- `hotplug2` and `hotplug.d` audited;
- `init.d` scripts audited;
- `cron`/`procd`/`ubus`/`mdev` confirmed absent;
- cloud/Aginet services considered;
- schedule objects considered;
- `DEV2_LIFEMOTE_AGENT` internals traced;
- live reboot test performed.

Therefore:

```text
NATIVE_AUTOSTART = NOT_AVAILABLE
```

The investigation now transitions to the custom autostart design phase.

---

## 11. Required Next Step

Proceed to `PHASE19_CUSTOM_AUTOSTART_DESIGN.md` to define the minimal, safe, maintainable external autostart mechanism for the EX520V.
