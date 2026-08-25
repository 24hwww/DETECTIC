# Detectic — Persistence Validation (EX520V)

## Investigation objective

Determine whether the stock firmware provides an *official, writable* mechanism
to start Detectic automatically on every boot, without modifying the firmware or
using a shell.

## Methods considered

1. Boot scripts under `/etc/init.d/`
2. `/etc/rcS_hook` directory
3. `procd` services (`/etc/rc.common`)
4. BusyBox `crond` / crontab
5. `backupcfg.bin` restore path
6. Vendor app platform (`INCLUDE_PORTABLE_APP`, `INCLUDE_AGINET_APP_V2`)

## Evidence

### 1. `/etc/init.d/rcS` is read-only and has no user hook

`/etc/init.d/rcS`:

- Sources `/etc/config.bba`.
- Mounts `misc_ro`, `misc_rw`, optional `misc_rw_bak` and `runtime_data` UBI partitions.
- Copies `/etc/mfg_config.bin` to `/var/run/misc/misc_rw/0x00300000` only if it does not exist.
- Creates volatile `/var/tmp`, `/var/run`, etc.
- Loads kernel modules.
- Sources `/etc/init.d/rcS.model` (also read-only).
- Starts `cos`, `cmmsyslogd`, and other vendor daemons.

No line in `rcS` executes a user-writable hook file.

### 2. `/etc/init.d/rcS.model` is model-specific but read-only

`rcS.model` only:

- Brings up `eth0`/`eth1`.
- Inserts `mii.ko`.
- Creates `/dev` nodes for flash, ttyUSB, voip.

No hook for third-party executables.

### 3. `/etc/rcS_hook` is empty

```text
$ ls -la _rootfs/etc/rcS_hook
total 12
drwxr-x--- 2 ...
-rw-r----- 1 ... .gitkeep
```

Only a `.gitkeep` file; no executable scripts are sourced from this directory.

### 4. `firmware.sh` (procd service) only runs read-only hotplug scripts

`firmware.sh`:

```sh
#!/bin/sh /etc/rc.common
START=15
USE_PROCD=1

start_service() {
    [ -f /etc/hotplug.d/firmware/11-mtk-wifi-e2p ] && sh /etc/hotplug.d/firmware/11-mtk-wifi-e2p
    [ -f /etc/hotplug.d/firmware/12-mtk-wifi-e2p ] && sh /etc/hotplug.d/firmware/12-mtk-wifi-e2p
}
```

Both files are in the read-only rootfs. There is no path for a user to add a
new hotplug script.

### 5. BusyBox `crond` is not started at boot

`rcS` does not start `crond`. The default crontab directory is `/etc` (read-only).
A shell could start `crond -c <writable dir>`, but that requires a shell *and*
survives reboot only if a startup hook exists to start it again.

### 6. `backupcfg.bin` is configuration-only

Detailed reverse engineering in `<investigations/BACKUPCFG_ANALYSIS.md>` proves:

- `backupcfg.bin` is DES-ECB encrypted, zlib-compressed XML.
- `restore` calls `dm_restoreCfg` → `dm_saveCfg`; it writes the data model to
  `/var/run/misc/misc_rw/0x00300000`.
- No `system`, `popen`, `exec`, or arbitrary file write is invoked.
- It can potentially *enable* `telnetd` or `dropbear` (runtime shell), but cannot
  *persist* a third-party binary.

### 7. App platform flags are for TP-Link Aginet, not third-party apps

`INCLUDE_PORTABLE_APP` and `INCLUDE_AGINET_APP_V2` refer to the TP-Link/ISP
mobile-app integration. No third-party app installation path exists.

## Summary table

| Mechanism | Can Detectic use it? | Evidence |
|---|---|---|
| `rcS` | No | read-only, no user hook |
| `rcS.model` | No | read-only, only modules/devices |
| `/etc/rcS_hook` | No | empty `.gitkeep` only |
| `procd` / `firmware.sh` | No | calls read-only hotplug scripts |
| `crond` | No | not started, `/etc` read-only |
| `backupcfg` | No | configuration-only, no arbitrary execution |
| `misc_rw` UBI | No for persistence | writable, but nothing reads it at boot |

## Conclusion on persistence

The stock firmware does **not** expose a legitimate, writable startup hook for
third-party executables. Persistence would require one of:

1. Reflashing a modified firmware image.
2. A runtime shell plus an unsupported reboot-persistence trick.
3. A previously unknown, signed, vendor-provided startup mechanism.

None of these are available within the M4 hard constraints.
