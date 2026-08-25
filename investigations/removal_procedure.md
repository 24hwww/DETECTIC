# Detectic — Removal Procedure (M4.1 Updated)

## Scope

This procedure covers rollback for any Detectic deployment scenario on the
TP-Link EX520V stock firmware. Since M4.1 found no stock persistence mechanism,
the only deployment path is manual (non-persistent) execution from a writable
directory.

## Scenario 1: Manual execution from `misc_rw`

If Detectic was manually copied to `/var/run/misc/misc_rw/` and run:

```sh
# 1. Stop the running process
killall detectic

# 2. Remove the binary
rm -f /var/run/misc/misc_rw/detectic

# 3. Remove any runtime data (if created)
rm -rf /var/run/misc/misc_rw/detectic_data

# 4. Remove any test markers
rm -f /var/run/misc/misc_rw/detectic_test_marker
rm -f /var/run/misc/misc_rw/m4_1_boot_marker

# 5. Verify no startup configuration was changed
#    (nothing to verify — no stock mechanism was used)

# 6. Reboot to confirm stock behavior
reboot
```

After reboot, the router returns to its original stock state because:

- The binary was in `misc_rw` (writable but not executed at boot).
- No startup script was modified (none can be — rootfs is read-only).
- No cron job was created (crond is not started at boot).
- No backup/restore was performed (configuration unchanged).

## Scenario 2: No deployment (current state)

No Detectic binary was installed on the router during M4.1. No configuration
changes were made. No firmware files were modified. The router is in its
original stock state.

## Verification

To confirm the router is in stock state after removal:

```sh
# Check that no Detectic process is running
ps | grep detectic

# Check that no Detectic binary exists in writable partitions
ls -la /var/run/misc/misc_rw/ | grep detectic

# Check that no test markers exist
ls -la /var/run/misc/misc_rw/ | grep marker

# Check that rootfs is unchanged (read-only, cannot be modified)
mount | grep squashfs

# Check that no startup scripts were added (impossible on read-only rootfs)
ls -la /etc/init.d/
ls -la /etc/rcS_hook/
```

## Firmware integrity

The rootfs is SquashFS/UBIFS read-only. No runtime modification is possible
without reflashing. Therefore, firmware integrity is guaranteed by the
read-only nature of the rootfs — no hash comparison is needed unless firmware
was reflashed (which is outside M4.1 constraints).

## Conclusion

Removal is trivial because no persistence mechanism exists on stock firmware.
Deleting the binary from `misc_rw` and rebooting returns the router to its
original state. No firmware, configuration, or startup script changes are
required.
