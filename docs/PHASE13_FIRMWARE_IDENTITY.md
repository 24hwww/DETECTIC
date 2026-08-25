# PHASE13_FIRMWARE_IDENTITY.md

## Hardware
- Model: TP-Link EX520V
- Firmware ID: EX520V124101568249n_agc3000_0945460481
- SoC: MediaTek MT7981
- Architecture: aarch64

## Build Config
- RUNTIME_DATA_SECTION_SIZE=0
- INCLUDE_RUNTIME_DATA_SECTION not set
- INCLUDE_DIGITAL_SIGNATURE not set

## Filesystem
- RootFS: squashfs/ubi ro
- /var/run/misc/misc_rw: ubifs rw
- /var/run/misc/misc_rw_bak: ubifs rw
- /var/run/misc/misc_isp: ubifs rw

## Init
- BusyBox inittab → /etc/init.d/rcS
- cos supervisor
- cmmsyslogd

## Classification
PROVEN-FROM-SOURCE: rootfs files, config.bba
UNKNOWN: bootloader identity, firmware container format, partition table
BLOCKED: full firmware image not extracted

## Missing Evidence
- bootloader binary
- firmware image header
- partition layout
- signature metadata
