# Detectic — Runtime Environment (EX520V)

## Source of evidence

All values below come from the extracted stock rootfs (`_rootfs/`) of the
TP-Link EX520V firmware `EX520V124101568249n_agc3000_0945460481`, already
recovered during previous milestones. No router command was executed because
no legitimate shell access is available in this session.

## Architecture / libc

| Item | Value |
|---|---|
| SoC family | MediaTek MT7981 (Cortex-A53, 4x ARM64) |
| ELF class | `ELF64` |
| ELF machine | `AArch64` |
| Endianness | little endian |
| C library | musl libc (`ld-musl-aarch64.so.1`) |
| BusyBox | dynamically linked, interpreter `/lib/ld-musl-aarch64.so.1` |
| Build toolchain (from `busybox` RPATH) | `aarch64_cortex-a53_gcc-8.4.0_musl` |

Evidence:

```text
$ file _rootfs/bin/busybox
_rootfs/bin/busybox: ELF 64-bit LSB executable, ARM aarch64,
  dynamically linked, interpreter /lib/ld-musl-aarch64.so.1, stripped

$ readelf -h _rootfs/bin/busybox | grep -E 'Clase|Maquina|Datos'
  Clase:      ELF64
  Datos:      complemento a 2, little endian
  Maquina:    AArch64

$ readelf -l _rootfs/bin/busybox | grep interpreter
      [Requesting program interpreter: /lib/ld-musl-aarch64.so.1]
```

## Writable and persistent paths

From `/etc/init.d/rcS`, the router mounts UBI partitions as UBIFS:

| Path | Value | Persistent |
|---|---|---|
| `/` | SquashFS / UBIFS rootfs, read-only | yes (read-only) |
| `/etc` | read-only SquashFS | no (for runtime writes) |
| `/tmp` | symlink to `/var/tmp` | no |
| `/var/tmp` | created at boot, volatile | no |
| `/var/lock` | created at boot | no |
| `/var/log` | created at boot | no |
| `/var/run` | created at boot | no |
| `/var/run/misc/misc_ro` | UBIFS, mounted read-only | yes (read-only) |
| `/var/run/misc/misc_rw` | UBIFS, mounted read-write | **yes** |
| `/var/run/misc/misc_rw_bak` | UBIFS, mounted read-write (if dual-config) | **yes** |
| `/var/run/runtime_data` | UBIFS, mounted read-write (if feature enabled) | **yes** |

The only practical persistent writable area for an uploaded binary is
`/var/run/misc/misc_rw`. It is created by `rcS` with mode `0777` and survives
reboot because it lives in the `misc_rw` UBI partition.

## Init system

- Shell: BusyBox `/bin/sh`.
- Boot script: `/etc/init.d/rcS` (read-only).
- Model-specific boot: `/etc/init.d/rcS.model` (read-only).
- Procd service: `/etc/init.d/firmware.sh` (read-only, calls only `/etc/hotplug.d/firmware` scripts).
- `rcS_hook` directory: exists at `/etc/rcS_hook` but is empty, containing only `.gitkeep`.
- BusyBox `crond` is compiled in but not started by `rcS`.
- `telnetd` and `dropbear` are present in the image (`/usr/sbin/telnetd`, `/usr/bin/dropbear`).

## Network / Wi-Fi interfaces

Actual interface names are not observable without a live shell. The firmware
references `eth0`, `eth1`, and wireless interfaces driven by MediaTek drivers
(`/lib/modules/tp_board.ko`, `RT2860AP`). `iw`/`iwinfo` presence cannot be
confirmed from the extracted rootfs alone.
