# M11-D — Boot / Startup Mechanisms (stock TP-Link EX520V)

**Status: STATIC ANALYSIS ONLY — no reboot, no persistence install performed.**
This document records what startup hooks *would* be required and why none are
legitimate/available on stock firmware. It is deliberately a design record, not
an executable change.

## Authorized safety constraint
- No reboot / power-cycle / factory-reset / flash.
- No modification of init scripts, squashfs, partitions, or firmware config.
- Detectic remains an **external / off-router** observer.

Therefore M11-D (actual persistent install) and M11-F (reboot validation) are
**BLOCKED** and documented as procedures only. See `m11_reboot_validation.md`.

## Firmware init architecture (from `_rootfs/`)
- Stock EX520V uses **BusyBox init**, not `procd`/`OpenWrt`.
- Root filesystem is **SquashFS (read-only)**; `/etc/init.d/` lives there.
- Persistent writable area: only `/var/run/misc/misc_rw` (UBI). Everything else
  writable is RAM/volatile and lost on reboot.
- `router/detectic.initd` (in this repo) is **aspirational only** — it targets a
  hypothetical OpenWrt-style init that does not exist on this device.

## Legitimate launch-mode design space (`persistence::LaunchMode`)
| Mode | Realizable on stock? | Notes |
|------|----------------------|-------|
| `StockManual` | YES (default) | Operator starts the sensor after each reboot. No router change. |
| `VendorService` | NO | Would require writing a service under read-only `/etc/init.d/`. Forbidden. |
| `Procd` | NO | Device does not use `procd`. Not applicable. |
| `SupportedService` | NO | No vendor hook exposed to user-space. |
| `ExternalLauncher` | YES (off-box) | A separate always-on device supervises/restarts the sensor. No router change. |

## Conclusion
`AUTO_START_SUPPORTED = false` on stock firmware. The only legitimate, safe mode
is `StockManual` (dev/client). The `detectic launcher` CLI therefore **refuses**
`VendorService`/`Procd` install (returns `Refused`) and performs a safe no-op for
`StockManual`/`ExternalLauncher`.

## Future (only if a supported hook is later provided)
If TP-Link/ISP ships a supported service hook (e.g. a documented init script
path on a writable partition), add a `LaunchMode` variant and implement
`DetecticLauncher::install` for it — without ever touching read-only squashfs.
