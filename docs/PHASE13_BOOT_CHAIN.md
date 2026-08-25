# PHASE13_BOOT_CHAIN

## Boot Chain Reconstruction from rootfs

ROM → Bootloader (U-Boot, inferred)
 ↓
Firmware verification (signature/hashing, details unknown)
 ↓
Kernel boot
 ↓
BusyBox linuxrc → inittab → ::sysinit:/etc/init.d/rcS
 ↓
/etc/init.d/rcS
  - Mount sysfs, debugfs
  - UBI attach misc_ro, misc_rw, misc_rw_bak, misc_isp
  - Mount ubifs partitions
  - Create /var/run/..., /var/tmp, /var/log
  - Copy /etc/mfg_config.bin → /var/run/misc/misc_rw/0x00300000 if missing
  - Load kernel modules: tp_board, tp_gpio, tp_domain, ivi, xt_massurl, etc.
  - /proc sys tweaks
  - ifconfig lo up
  - Source /etc/init.d/rcS.model
  - Start daemons: cos &, cmmsyslogd &
 ↓
TP-Link COS supervisor
 ↓
Network services, web UI, CWMP, dnsmasq etc.

## Execute Points
- /etc/init.d/rcS is read-only rootfs
- /etc/rcS_hook exists but path is /etc/rcS_hook in rootfs, ro
- /bin/rcsHook searches /etc/rcS_hook, cannot be redirected to misc_rw without firmware change
- Hotplug scripts in /etc/hotplug.d/* are ro
- Config apply handlers can launch dropbear/telnetd via data model

## Classification
PROVEN OFFLINE: rcS mounts misc_rw, starts cos, no RW→EXEC
UNKNOWN: bootloader verification details, kernel command line
SIMULATED: full boot timing

No legitimate RW→EXEC chain exists without firmware modification.
