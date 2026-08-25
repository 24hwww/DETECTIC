# PHASE13.6_BOOT_EXECUTION_GRAPH.md

## BootROM → Bootloader → Kernel → Init → rcS → Services

## Evidence

BootROM: UNKNOWN
Bootloader: UNKNOWN, inferred U-Boot
Kernel: UNKNOWN version
Init: BusyBox inittab PROVEN-FROM-SOURCE
rcS: PROVEN-FROM-SOURCE _rootfs/etc/init.d/rcS

rcS stages PROVEN-FROM-SOURCE:
- mount sysfs, debugfs
- UBI attach misc_ro, misc_rw, misc_rw_bak, misc_isp
- mount ubifs
- create /var/run subdirs
- copy mfg_config.bin → misc_rw
- modprobe modules
- /proc sys tweaks
- ifconfig lo up
- source /etc/init.d/rcS.model
- start cos & , cmmsyslogd &

Trust boundary at bootloader/kernel UNKNOWN
Rootfs RO PROVEN-FROM-SOURCE
misc_rw RW PROVEN-FROM-SOURCE

No RW→EXEC chain proven.

## Classification
PROVEN-FROM-SOURCE: rcS execution
PROVEN-OFFLINE: mount points
UNKNOWN: BootROM, bootloader verification
INFERRED: U-Boot
SIMULATED: boot timing
