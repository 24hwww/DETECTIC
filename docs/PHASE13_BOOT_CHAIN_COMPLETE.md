# PHASE13_BOOT_CHAIN_COMPLETE.md

## Boot Flow

BootROM → U-Boot → kernel verification? → kernel → init → rcS

## rcS Stages
1. Mount sysfs, debugfs
2. UBI attach 4 volumes
3. Mount ubifs
4. Create /var/run subdirs
5. Copy mfg_config.bin → misc_rw
6. Load modules: tp_board, tp_gpio, tp_domain, ivi, xt_massurl
7. /proc sys tweaks
8. ifconfig lo up
9. Source /etc/init.d/rcS.model
10. Start cos, cmmsyslogd

## Execution Surfaces
- /etc/init.d/rcS → ro
- /etc/rcS_hook → ro, searched by /bin/rcsHook
- Hotplug.d scripts → ro
- cos config → ro
- misc_rw → rw, no exec trigger

## Classification
PROVEN-FROM-SOURCE: rcS content, mount points
UNKNOWN: U-Boot behavior, kernel verification
SIMULATED: full timing

No RW→EXEC chain proven.
