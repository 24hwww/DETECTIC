# PHASE13.6_AUTOSTART_EXHAUSTION.md

## Surfaces checked

/etc/init.d → RO PROVEN-FROM-SOURCE
/etc/rc.d → RO
/etc/rcS → RO
/etc/rc.local → RO or absent
/etc/profile → RO
/etc/hotplug.d → RO
/etc/preinit → RO
/etc/cron* → no evidence
/etc/udev* → not present
/etc/network → RO
/etc/config → RO
/etc/default → RO
/etc/inittab → RO

System calls: strings found in binaries but no writable path linkage proven

Writable paths:
/var/run/misc/misc_rw → RW PROVEN-FROM-SOURCE
/tmp → temporary
/var/tmp → temporary

Execution chain from writable path → trigger → parser → exec → PROVEN: none

All init scripts hardcode executable paths in rootfs RO.

Conclusion: No legitimate autostart surface consuming user-controlled file from misc_rw.

Classification: PROVEN-FROM-SOURCE for RO paths, PROVEN-OFFLINE for no RW→EXEC
