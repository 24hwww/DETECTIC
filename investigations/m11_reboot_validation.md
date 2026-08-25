# M11-F — Reboot Validation Procedure (stock TP-Link EX520V)

**Status: PROCEDURE ONLY — execution BLOCKED by the no-reboot safety rule.**
This file documents the exact steps that *would* validate auto-start after a
reboot, so they can be run later by an operator with physical access and an
explicit reboot authorization. Detectic did not (and will not) trigger a reboot.

## Preconditions (must all hold before any reboot)
1. Recovery incident (`m11_recovery_incident.md`) fully closed.
2. IPv4 management restored (or intentionally left as-is with sign-off).
3. An explicit, documented operator authorization to reboot the router.
4. A supported launch mode confirmed (`StockManual` or `ExternalLauncher` only).

## Procedure
1. **Establish a known-good baseline** (pre-reboot):
   - `detectic status` → record health.
   - `detectic launcher status` → expect `auto_start_supported=false mode=StockManual`.
   - Capture current associated-station count via `detectic map`.
2. **For `StockManual` mode** (no auto-start expected):
   - Reboot the router (operator-authorized).
   - After boot, confirm Detectic is **not** running on the router (expected —
     stock firmware does not start it). Verify no Detectic process/file on router.
   - Manually start the external sensor client; confirm it re-acquires the GTPR
     source and emits events via `detectic realtime`.
3. **For `ExternalLauncher` mode** (off-box supervisor):
   - Reboot the router.
   - Confirm the external supervisor restarts the sensor process and events
     resume within the configured interval.
4. **Post-reboot validation**:
   - `detectic health` passes.
   - Event stream monotonic `seq` continues without gaps.
   - No router-side persistence artifact created.

## Acceptance criteria
- No firmware/partition modification occurred.
- Detectic behaves exactly as documented for the chosen mode.
- All evidence written to `investigations/` before and after.

## Blocked note
As of this writing the reboot was **not** performed. M11-F remains a procedure.
The IPv4 management outage documented in `m11_recovery_incident.md` is
pre-existing and independent of this milestone.
