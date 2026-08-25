# M5-N — Persistence Strategy

## Date
2026-08-23

## Objective
Document the persistence strategy for the Detectic sensor on the TP-Link EX520V.
The stock firmware does not offer a mechanism for automatic startup of user-provided
binaries after reboot. This document outlines the available approaches and
recommends a path forward.

## Current State

### What Works (M4.4 + M5)
- Detectic binary executes correctly on the router (aarch64 musl static)
- GTPR API communication works (login, gl, go, so, op, cgi)
- Sensor runtime polls continuously, detects changes, pseudonymizes data
- Resource footprint is minimal (~1 MB RSS, 1 thread, 1.1 MB binary)
- Offline spool to `/tmp/detectic_buffer.jsonl` (bounded, JSONL)

### What Does Not Work
- **No auto-start**: The stock firmware has no `init.d`, `systemd`, or `cron`
  mechanism for user binaries. After reboot, the Detectic process is gone.
- **No persistent storage**: `/tmp` is tmpfs (RAM-backed, cleared on reboot).
  The only persistent partition is `/dev/mtdblock` (flash, read-only for user
  data without firmware modification).

## Persistence Approaches

### Approach 1: Lifemote Agent (Current — Manual Start)

**How it works**: The firmware's Lifemote Agent feature downloads and executes
a shell script from a URL. We use this to start `telnetd` on port 8888, then
manually deploy and start Detectic.

**Pros**:
- No firmware modification required
- Uses a built-in firmware feature
- Works with stock firmware

**Cons**:
- Requires manual intervention after every reboot
- Requires a local HTTP server to serve the bootstrap script
- Not suitable for production deployment
- Lifemote agent is a debugging feature that may be removed in future firmware

**Verdict**: Suitable for development and testing only.

### Approach 2: Firmware Modification (Recommended for Production)

**How it works**: Modify the firmware image to include Detectic in the rootfs
and add a startup script to the init sequence.

**Steps**:
1. Extract the firmware image (squashfs + kernel)
2. Unsquash the rootfs
3. Add `detectic` binary to `/usr/bin/`
4. Add startup script to `/etc/init.d/` or equivalent
5. Resquash and repackage the firmware
6. Flash the modified firmware

**Pros**:
- Detectic starts automatically on boot
- Survives reboots
- No external dependencies
- Clean integration with the system

**Cons**:
- Requires firmware modification (risky)
- Need a recovery strategy (TFTP, serial console)
- May void warranty
- Firmware updates will overwrite the modification

**Verdict**: The correct approach for production, but requires careful
preparation and a recovery strategy.

### Approach 3: U-Boot Bootloader Hook

**How it works**: Modify the U-Boot bootloader to download and execute
Detectic before the main firmware boots.

**Pros**:
- Independent of firmware updates
- Very early execution

**Cons**:
- Extremely risky (bricking risk)
- Requires UART access
- Complex to implement
- Not recommended

**Verdict**: Not recommended at this stage.

### Approach 4: External Cron / Watchdog

**How it works**: Use an external device on the network to periodically
check if Detectic is running on the router, and re-deploy if not.

**Pros**:
- No firmware modification
- Simple to implement

**Cons**:
- Requires an always-on external device
- Not truly autonomous
- Adds a dependency

**Verdict**: Suitable as a temporary measure during development.

## Recommended Strategy

### Phase 1 (Current — M5): Manual Deployment
- Use the Lifemote agent approach for testing
- Document the deployment procedure
- Verify all sensor functionality

### Phase 2 (M6): Firmware Modification
- Extract and analyze the firmware image structure
- Identify the init system and startup sequence
- Create a modified firmware with Detectic auto-start
- Test with a recovery strategy (TFTP flash, serial console)
- Document the firmware modification procedure

### Phase 3 (M7+): Over-the-Air Updates
- Design a mechanism for updating Detectic without reflashing
- Consider a dual-partition approach (A/B firmware)
- Implement signed updates for security

## State Persistence (Within a Boot Session)

Even without firmware modification, the sensor can persist state within a
boot session using the tmpfs filesystem:

| Path | Purpose | Lifetime |
|------|---------|----------|
| `/tmp/detectic_buffer.jsonl` | Offline upload spool | Until reboot |
| `/tmp/detectic_state.json` | Last snapshot (for diff) | Until reboot |
| `/var/tmp/detectic` | Binary location | Until reboot |

After reboot, the sensor starts fresh (no previous snapshot) and emits
`DeviceJoined` events for all observed devices on the first poll. This is
acceptable behavior — the backend can correlate the join events with the
sensor restart.

## Configuration Persistence

Configuration is provided via environment variables at launch time. For
the manual deployment approach, a wrapper script can set the env vars:

```sh
#!/bin/sh
# /var/tmp/detectic_start.sh
export DETECTIC_PASSWORD='...'
export DETECTIC_SECRET='...'
export DETECTIC_SENSOR_ID='home-001'
export DETECTIC_INTERVAL='30'
export DETECTIC_BACKEND_URL='https://api.detectic.example/upload'
/var/tmp/detectic sensor &
```

For the firmware modification approach, configuration would be stored in a
read-only file in the rootfs (e.g. `/etc/detectic.conf`) with secrets loaded
from a separate writable partition or NVRAM.

## Conclusion

The stock firmware does not support automatic startup of user binaries.
For production deployment, firmware modification is the recommended approach.
Until then, the Lifemote agent provides a working manual deployment path for
development and testing.
