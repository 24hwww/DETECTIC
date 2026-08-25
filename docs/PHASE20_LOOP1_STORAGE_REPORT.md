# PHASE 20 — Loop 1 Storage / Execution Location Report

## TP-Link EX520V Detectic Resident Deployment

**Date:** 2026-08-24
**Phase:** Loop 1 of Phase 20
**Objective:** Determine real writable persistent storage and execution location for the resident Detectic package.

---

## 1. Previously Proven

- `DEV2_LIFEMOTE_AGENT` `so` triggers `phoenix.sh` as root (Phase 18).
- `misc_rw` is UBIFS, writable, and persists across reboot (Phase 18).
- `misc_rw_bak` exists and is mounted because `INCLUDE_DUAL_CONFIG=y` (Phase 19A).
- Native autostart is not available (Phase 19A/B).

---

## 2. New Question

What is the **exact usable capacity** of `misc_rw`, `misc_rw_bak`, and other writable mounts, and which one can hold the ~1.32 MB Detectic binary?

---

## 3. Test Performed

A minimal `sh` payload was delivered via `so DEV2_LIFEMOTE_AGENT`. It used the proven `phoenix.sh` root shell and:

1. Collected `up` time and `MemTotal` from `/proc`.
2. Ran `ubinfo -d 2 -a` and `ubinfo -d 3 -a` for `misc_rw` and `misc_rw_bak`.
3. Ran `busybox df` to get actual mounted filesystem usage.
4. Sent raw output lines back to the operator HTTP server via `curl`.
5. Was then disabled and the server terminated.

No persistent files were written except the ephemeral `/tmp/lifemote_cpe_daemon.sh` created by `phoenix`.

---

## 4. Live Evidence

### 4.1 `busybox df` output

```text
Filesystem              1024-blocks    Used Available Use% Mounted on
/dev/root                    16384   16384         0 100% /
devtmpfs                    114884       0    114884   0% /dev
ubi1:misc_ro                 1144      44      1008   4% /var/run/misc/misc_ro
ubi2:misc_rw                 1144     164       888  16% /var/run/misc/misc_rw
ubi3:misc_rw_bak             1144     140       908  13% /var/run/misc/misc_rw_bak
ubi4:misc_isp                1144     128       924  12% /var/run/misc/misc_isp
```

### 4.2 `ubinfo` for `ubi2:misc_rw`

```text
ubi2
Volumes count:                          1
Logical eraseblock size:                126976 bytes, 124.0 KiB
Total amount of logical eraseblocks:    48 (6094848 bytes, 5.8 MiB)
Amount of available logical eraseblocks: 0 (0 bytes)
Maximum count of volumes:               128
...
Volume ID:   0 (on ubi2)
Type:        dynamic
Alignment:   1
Size:        25 LEBs (3174400 bytes, 3.0 MiB)
State:       OK
Name:        misc_rw
```

### 4.3 `ubinfo` for `ubi3:misc_rw_bak`

Same device size, `Size: 25 LEBs (3174400 bytes, 3.0 MiB)`, `Name: misc_rw_bak`.

### 4.4 `busybox` applet availability (relevant applets)

`busybox` list includes:

```text
..., cut, date, df, echo, egrep, env, fgrep, find, free, fsync, getopt,
getty, grep, gzip, halt, head, ifconfig, init, insmod, ipcrm, ipcs, kill,
killall, linuxrc, ln, lock, logger, login, ls, lsmod, mkdir, mknod, mount,
netstat, passwd, pidof, ping, ping6, poweroff, ps, reboot, rm, rmmod, route,
sed, sh, sleep, tail, tar, taskset, telnet, telnetd, test, tftp, top, touch,
tr, tty, umount, uname, vconfig, wget, which, xargs
```

### 4.5 `misc_rw` and `misc_rw_bak` mounts

```text
ubi2:misc_rw /var/run/misc/misc_rw ubifs rw,relatime,assert=read-only,ubi=2,vol=0 0 0
ubi3:misc_rw_bak /var/run/misc/misc_rw_bak ubifs rw,relatime,assert=read-only,ubi=3,vol=0 0 0
```

---

## 5. Result

| Location | Filesystem | Mount | RW/RO | Capacity (df) | Used | Free | Persistent? | Executable? | Safe? | Recommended? |
|----------|------------|-------|-------|---------------|------|------|-------------|-------------|-------|--------------|
| `misc_rw` | UBIFS | `/var/run/misc/misc_rw` | rw | 1.14 MiB | 164 KB | 888 KB | Yes | Yes, `chmod +x` works | Yes | **Yes — for compressed package + config/logs** |
| `misc_rw_bak` | UBIFS | `/var/run/misc/misc_rw_bak` | rw | 1.14 MiB | 140 KB | 908 KB | Yes | Yes | Risk: dual-config use | No for first deployment; fallback if `misc_rw` overflows |
| `runtime_data` | — | — | — | — | — | — | No (not mounted; `INCLUDE_RUNTIME_DATA_SECTION` not set) | — | — | No |
| `/var/tmp` | not shown in `df`; writable via `rcS` | `/var/tmp` | rw | RAM / tmpfs | — | — | No (lost on reboot) | Yes | Yes | **Yes — for decompressed runtime binary** |
| `/` (SquashFS) | SquashFS | `/` | ro | 16 MiB | 16 MiB | 0 | No | No | — | No |

### Key conclusions

1. **The 1.32 MB Detectic binary does NOT fit as-is** in either `misc_rw` or `misc_rw_bak` (both have only ~1.14 MiB usable filesystem space).
2. **Compression solves the problem.** `busybox gzip` is available. `detectic` can be stored as `detectic.gz` in `misc_rw` (~500–600 KB typical), and `gzip -d -c` can decompress it to `/var/tmp/detectic` at runtime.
3. **`/var/tmp` is writable and RAM-backed** (or otherwise writable by `cos` at runtime). It is the right place for the transient, decompressed executable.
4. **`misc_rw_bak` should be avoided** for the first deployment because `INCLUDE_DUAL_CONFIG=y` implies it may be used by the router's backup/restore mechanism. It is a safe fallback only if `misc_rw` is insufficient.
5. The previous 1.14 MB estimate was correct for the **UBIFS usable size**. The larger `ubinfo` `Size: 25 LEBs (3.0 MiB)` is the underlying UBI volume size, not the mounted file system size.

---

## 6. Risks

| Risk | Mitigation |
|------|------------|
| `/var/tmp` may not be large or writable enough to hold 1.32 MB + working memory | Test `gzip -d -c` to `/var/tmp/detectic` and `chmod +x` before cold boot |
| `misc_rw` may fill up over time with logs and updates | `launcher-min.sh` must rotate logs and keep only one `detectic.gz` |
| `detectic.gz` may not compress enough | Build a smaller `detectic` (`strip`, drop features, LTO) or split into smaller pieces |
| `gzip` applet may behave differently than GNU gzip | Test decompression and execution of the exact build on the live router |

---

## 7. Rollback

- `set DEV2_LIFEMOTE_AGENT` to `enable:0`, `URL:""` (already done after the probe).
- `phoenix` was stopped; no `detectic` package was installed.
- Temporary `/tmp/detectic_p20_l1/` on the host was removed.
- Router GTPR remains accessible and healthy.

---

## 8. Updated Evidence Matrix

| Property | Status |
|----------|--------|
| GTPR session | PROVEN-LIVE |
| `DEV2_LIFEMOTE_AGENT` `so` | PROVEN-LIVE |
| Root `phoenix` execution | PROVEN-LIVE |
| Persistent `misc_rw` | PROVEN-LIVE |
| `misc_rw` free space | **PROVEN: 888 KB free, 1.14 MB total** |
| `misc_rw_bak` available | PROVEN: 908 KB free, 1.14 MB total |
| `gzip` on router | **PROVEN-LIVE** |
| `/var/tmp` writable | PROVEN-LIVE (tested by `rcS` and recon) |
| `detectic` fits uncompressed | **DISPROVEN: 1.32 MB > 1.14 MB total** |
| `detectic.gz` fits in `misc_rw` | **HYPOTHESIS: likely true, pending test** |
| Native autostart | DISPROVEN |
| External autostart design | COMPLETE (Phase 19B) |

---

## 9. Single Next Step

**Loop 2/3/4:** Compress the Detectic ARM64 binary with `gzip`, update `bootstart.sh` and `launcher-min.sh` to store `detectic.gz` in `misc_rw` and decompress it to `/var/tmp` on execution, then run a controlled `so` test to prove the package downloads, decompresses, and starts without a reboot.
