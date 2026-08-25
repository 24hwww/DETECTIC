# M4.1-F — HAL Runtime Smoke Test

## Status

**Not performed.** No legitimate shell access to the EX520V is available.

## What was prepared

The HAL prototype exists at `prototypes/mtk_hal_probe/`. It was built for the
host architecture during M3. For M4.1, it would need to be cross-compiled for
`aarch64-unknown-linux-musl` and executed on the router.

## Planned read-only tests (pending shell access)

The following tests were designed but could not be executed:

### 1. `list_associated()`

- Call the reconstructed ioctl to list associated Wi-Fi clients.
- Record: ioctl return code, buffer size, number of records, per-station RCPI/RSSI.
- No configuration changes.

### 2. `scan_results()`

- Call the reconstructed ioctl to retrieve scan results.
- Record: ioctl return code, buffer size, number of BSSIDs, channels, signal levels.
- No configuration changes.

### 3. `unassoc_metrics(mac)`

- Use a MAC already observed through the router's associated-device data.
- Call the directed ioctl for unassociated station link metrics.
- Record: ioctl return code, buffer size, RCPI/RSSI, any errors.
- No configuration changes.

## What is known from M3

From the M3 milestone:

- MediaTek HAL ioctl interfaces have been reconstructed from disassembly.
- `getScanResult` and `getUnassocStaLinkMetrics` are local HAL interfaces.
- `getUnassocStaLinkMetrics` requires a known MAC.
- The ioctl ABI was verified against the firmware's binary code, but not against
  live hardware.

## Conclusion

HAL runtime access remains **unverified**. The ioctl ABI reconstruction is
complete, but no runtime test on actual hardware was possible without shell
access.
