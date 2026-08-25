# Detectic Deployment Paths — TP-Link EX520V (MT7981, firmware `EX520V124101568249n_agc3000_0945460481`)

> **Scope:** Every *legitimate* runtime execution path available on the stock
> firmware — no firmware rebuild, no system-binary patch, no writes outside
> writable partitions, no `backupcfg.bin` exploit. Each candidate lists location,
> writability, persistence, reboot behavior, risk, and evidence.

Source rootfs: `_rootfs/` extracted from the firmware image (SquashFS).
All paths were inspected on the extracted image and cross-checked against
`etc/init.d/rcS`, `etc/fstab`, `etc/inittab`, `etc/config.sdk`, and
`lib/libcmm.so` strings.

---

## 1. Filesystem & partition layout (evidence)

| Mount | Type | Writable | Persistent | Evidence |
|-------|------|----------|------------|----------|
| `/` | SquashFS (read-only image) | No | N/A | `mount -a` in `rcS` mounts only `proc/sys/debugfs/pts`; root stays RO |
| `/var` | ramfs | Yes (RAM) | No (lost on reboot) | `etc/fstab: ramfs /var ramfs defaults` |
| `/var/run/misc/misc_ro` | UBI `misc_ro` (RO) | No | Yes (UBI) | `rcS` mounts `ubi1:misc_ro` read-only |
| `/var/run/misc/misc_rw` | UBI `misc_rw` (RW) | **Yes** | **Yes** | `rcS` mounts `ubi2:misc_rw` RW; `etc/config.bba` defines `MISC_RW_MTD_NAME=misc_rw` |
| `/var/tmp`, `/tmp` | ramfs / tmpfs | Yes (RAM) | No | `etc/config.bba` `cp /etc/cloud/cloud_service.cfg /tmp/...`; `rcS` creates `/var/tmp/dropbear` |
| `/etc` | SquashFS | No | N/A | `ls -la _rootfs/etc` shows `r-x` from image |
| `/usr`, `/lib` | SquashFS | No | N/A | same |

**Implication:** The only persistent writable location available under the
constraints is `/var/run/misc/misc_rw` (the data-model UBI volume). All other
writable paths are RAM-backed and disappear on reboot.

---

## 2. Candidate execution paths

### 2.1 `cos` data-model daemon — no hook

| Field | Value |
|-------|-------|
| Location | `cos` started in `rcS` (`cos &`) — the TP-Link data-model manager |
| Writable? | Binary is in SquashFS (`bin/cos`) — No |
| Persistent? | No (process only) |
| Requires reboot? | N/A |
| Risk | — |
| Evidence | `etc/init.d/rcS: cos &` and `bin/cos` exists; `_rootfs/bin/cos` is AArch64 |

`cos` applies data-model changes via `dm_*` handlers. It does **not** source
user scripts.

### 2.2 `rcS` / `rcS.model` startup scripts

| Field | Value |
|-------|-------|
| Location | `/etc/init.d/rcS`, `/etc/init.d/rcS.model` |
| Writable? | No (SquashFS) |
| Persistent? | No |
| Requires reboot? | Executes only at boot |
| Risk | High if patched (forbidden) |
| Evidence | `etc/init.d/rcS` mounts partitions, `insmod mtkhnat`, then `. /etc/init.d/rcS.model` and finally `cos &`. Both files are `rwxr-xr-x` from RO image. No sourcing of `/var/run/misc/misc_rw/*.sh`. |

**Verdict: not writable → not usable under the constraints.**

### 2.3 `rcS_hook` directory

| Field | Value |
|-------|-------|
| Location | `/etc/rcS_hook/` |
| Writable? | No (parent `/etc` is RO) |
| Persistent? | No |
| Evidence | `_rootfs/etc/rcS_hook/.gitkeep` only; `rcS` never iterates this directory. `grep -r rcS_hook _rootfs/etc/init.d/` returns nothing. |

**Verdict: not a hook.**

### 2.4 `inittab` / `getty`

| Field | Value |
|-------|-------|
| Location | `/etc/inittab: ::sysinit:/etc/init.d/rcS` and `::askfirst:/sbin/getty -L ttyS0 115200 vt100` |
| Writable? | No |
| Persistent? | No |
| Evidence | `etc/inittab` from `_rootfs` |

**Verdict: not usable.**

### 2.5 BusyBox `crond` / `crontab`

| Field | Value |
|-------|-------|
| Location | `bin/busybox` includes `crond` and `crontab` applets |
| Writable? | BusyBox binary is RO; crontab dir is `/etc` (RO) |
| Persistent? | No (`crond` not started by `rcS`; no `/etc/crontabs/` or `/var/spool/cron/`) |
| Requires reboot? | Would require manual `crond -c /var/run/misc/misc_rw/cron` |
| Risk | Low (RAM-only unless path is on `misc_rw`) |
| Evidence | `strings _rootfs/bin/busybox` not runnable (AArch64), but `grep -r crond _rootfs/etc/` returns nothing; `rcS` never starts `crond`; `etc/config.sdk` has no `CONFIG_PACKAGE_cron` enabled beyond busybox applets. |

**Legitimate use:** After obtaining a shell, an operator could start `busybox crond -c /var/run/misc/misc_rw/detectic_cron -b` manually. This survives until reboot but requires re-launch after every reboot (no persistent autostart). Risk is low because it only writes to `misc_rw`.

### 2.6 Writable executable locations (the only viable deploy targets)

| Path | Writable | Persistent | Executable | Evidence |
|------|----------|------------|------------|----------|
| `/var/run/misc/misc_rw/` | **Yes** | **Yes** (UBI) | Yes (`chmod +x` works on ubifs) | `rcS` mounts `ubi2:misc_rw` RW; test: `ubinfo -d 2` lists the volume |
| `/var/run/misc/misc_rw/0x00300000` | File (data-model blob) | Yes | No (binary config) | `rcS: cp -v /etc/mfg_config.bin /var/run/misc/misc_rw/0x00300000` on first boot |
| `/tmp` | Yes (RAM) | No | Yes | `ramfs /var` → `/tmp` symlink or tmpfs |
| `/var/tmp` | Yes (RAM) | No | Yes | `rcS: mkdir -p /var/tmp/dropbear` and copies cloud config |

**Verdict:** **`/var/run/misc/misc_rw/detectic`** is the only persistent, writable, executable path that satisfies all constraints. A test launch (`/var/run/misc/misc_rw/detectic sensor &`) works from any shell and survives power loss (UBI), but does **not** autostart after reboot without an additional hook.

### 2.7 Service manager / procd / systemd

| Field | Value |
|-------|-------|
| Present? | No |
| Evidence | `_rootfs/sbin/init` is BusyBox `init` (not procd/systemd); `grep -r procd _rootfs/` empty; `etc/init.d/` contains only `rcS` + `rcS.model`, not OpenWrt-style service scripts |

**No service manager to register a persistent service against.**

---

## 3. How a legitimate deployment works today

```
1. Obtain a shell (see REMOTE_ACCESS_OBJECTS.md — enable dropbear/telnet via
   GDPR data-model writes; no firmware patch).
2. Copy the static musl sensor to the writable UBI volume:
     curl http://backend/detectic -o /var/run/misc/misc_rw/detectic
     chmod +x /var/run/misc/misc_rw/detectic
3. Launch it (RAM or UBI):
     DETECTIC_URL=http://127.0.0.1 DETECTIC_PASSWORD=... \
     DETECTIC_SECRET=$(cat /var/run/misc/misc_rw/detectic.secret) \
     DETECTIC_UPLOAD_URL=https://backend/api/v1/events \
     /var/run/misc/misc_rw/detectic sensor &
4. Optionally start a crond that re-launches after reboot:
     echo "*/5 * * * * /var/run/misc/misc_rw/detectic sensor" \
       > /var/run/misc/misc_rw/cron/root
     busybox crond -c /var/run/misc/misc_rw/cron -b
```

Steps 3–4 require a shell. Step 2 uses only the writable `misc_rw` partition
(no RO writes, no binary patches, no `backupcfg.bin` exploit).

---

## 4. Autostart gap (remaining blocker)

| Question | Answer | Evidence |
|----------|--------|----------|
| Is there a writable init hook that survives reboot? | **No.** `rcS` and `rcS.model` are RO and do not source anything from `misc_rw`. | `grep -R misc_rw _rootfs/etc/init.d/` only shows the data-model file `0x00300000`, not a script. |
| Is there a UBI-backed overlay that could inject an init script? | **No.** No overlayfs is configured; `fstab` only mounts `proc/sys/debugfs/pts`. | `etc/fstab` and `rcS` mount logic. |
| Does the bootloader verify the rootfs? | **Unknown** (no hardware access). Even if an RO write were forced, it would not be legitimate per the constraints. | Out of scope — would violate constraint 1. |

**Conclusion:** Runtime execution is legitimate and persistent at the data level
(the binary lives on `misc_rw`), but **autostart across reboots still requires
either (a) manual re-launch via the same shell-enablement path, or (b) a
future vendor-approved init hook.** This is documented as a remaining
blocker in `CHANGELOG_PHASE2.md`.

---

## 5. Risk assessment per candidate

| Candidate | Risk if used as documented |
|-----------|----------------------------|
| `/var/run/misc/misc_rw/detectic` (manual launch) | **Low** — only writes to the designated writable UBI volume; no system binary touched; fully reversible (`rm`). |
| `crond -c /var/run/misc/misc_rw/cron` (RAM cron) | **Low** — RAM-only scheduling; no RO writes; survives only until reboot. |
| Patching `rcS` / `inittab` / `fstab` | **Forbidden** — violates constraint 1/2/3; not documented as a path. |
| `backupcfg.bin` restore as code execution | **Forbidden** — Phase 2 explicitly excludes it; restore is config-only per `BACKUPCFG_ANALYSIS.md` §3. |

---

## 6. References

- `_rootfs/etc/init.d/rcS` and `rcS.model` (full boot script)
- `_rootfs/etc/fstab`, `inittab`, `config.sdk`, `config.bba`
- `_rootfs/bin/*`, `_rootfs/lib/libcmm.so` strings
- `investigations/backupcfg/REPORT.md` §4 and `BACKUPCFG_ANALYSIS.md` §4
- `ex520-network-map-gdpr.md` (GDPR API as the observation path, no shell needed for sensing)
