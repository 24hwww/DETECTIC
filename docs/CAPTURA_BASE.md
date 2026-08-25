# 0. CAPTURA BASE — TP-Link EX520V / AGC3000

## Firmware / Build / Bootloader / Filesystem / Mounts

**Device:** TP-Link EX520V, firmware identifier `EX520V124101568249n_agc3000_0945460481`
**SoC:** MediaTek MT7981
**CPU Arch:** ARM64, little endian
**Kernel:** ARM64
**Flash:** SPI NAND 128M, UBI
**RootFS:** SquashFS/UBI read-only, `rootfsA` 50 MiB, dual image `kernelA/B`, `rootfsA/B`
**Partitions:**
- misc_ro → `/var/run/misc/misc_ro`  ro ubifs
- misc_rw → `/var/run/misc/misc_rw`  rw ubifs  ← persistent config/data model
- misc_rw_bak → `/var/run/misc/misc_rw_bak`  rw ubifs  (if DUAL_CONFIG)
- misc_isp → `/var/run/misc/misc_isp`  ro ubifs  (if OPTION66)
- runtime_data → `/var/run/runtime_data`  rw ubifs  (optional)

Build config `etc/config.bba`:
- `INCLUDE_SPEC_EX520=y`
- `INCLUDE_MTK=y`, `INCLUDE_MTK_CHIP_MT7981=y`
- `INCLUDE_ARM64=y`
- `INCLUDE_FLASH_SPINAND=y`, `INCLUDE_UBI_SYSTEM=y`, `INCLUDE_DUAL_IMAGE=y`, `INCLUDE_DUAL_CONFIG=y`
- `INCLUDE_MTD_TYPE_FS=y`
- `INCLUDE_DIGITAL_SIGNATURE` not set → backup/restore accepts unsigned config
- `INCLUDE_WEB_TELNET=y`, `INCLUDE_REMOTE_TELNET=y`, `INCLUDE_SSH_ACCESS` not set
- `INCLUDE_TELNET_LOGIN_WAIT=y`
- `INCLUDE_AUTH_PASSWORD=y`
- `INCLUDE_PORTABLE_APP=y`, `INCLUDE_AGINET_APP_V2=y`

Boot:
- U-Boot based, `rcS` is init script executed by BusyBox linuxrc
- `rcS` mounts sysfs, debugfs, UBI volumes, creates `/var/lock /var/log /var/run /var/tmp /var/tmp/dropbear`
- Early copy: `cp /etc/mfg_config.bin /var/run/misc/misc_rw/0x00300000` on first boot if missing
- Init script loads kernel modules: `tp_board.ko tp_gpio.ko tp_domain.ko ivi.ko` etc.

## Procesos / init / servicios / sockets / puertos

From extracted rootfs:
- Init: BusyBox `linuxrc -> bin/busybox`
- Init script: `/etc/init.d/rcS`, `/etc/init.d/rcS.model`, `/etc/init.d/init_console.sh`, `firmware.sh`
- Daemons started in `rcS`:
  - `cos &`  → TP-Link COS daemon
  - `cmmsyslogd &`
  - `ve_vtsp_main &` (commented out in extracted version)
- Service supervisor: proprietary `cos` / `cmmsyslogd`
- Dropbear host keys dir: `/var/tmp/dropbear` created at boot
- SSH/Telnet binaries present in firmware image: `dropbear`, `telnetd`
- Data-model apply handlers enable SSH/Telnet via config objects:
  - `DEV2_SSH_CFG / Device.X_TP_AppCfg.SSHCfg.` → `dropbear -p %d -r %s -d %s -A %s &`
  - `DEV2_TELNET_CFG / Device.X_TP_AppCfg.TelnetCfg.` → `telnetd -p %d &`

## APIs CGI / GDPR / configuración / backupcfg

Backup/restore:
- Format: DES-ECB encrypted, zlib compressed XML, MD5 digest prefix
- Key derivation: hard-coded constant `74 8d a5 0b f9 3e 2d cf` XORed with 8-hex chars of 32-bit DeviceInfo value @ offset 0x51c, optional MD5 of password
- Decrypt → MD5 verify → decompress → `dm_restoreCfg` → `dm_saveCfg` → writes to `misc_rw` data model
- No arbitrary file write, config-only

Web CGI / GDPR:
- `INCLUDE_LOGIN_GDPR_ENCRYPT=y`
- `INCLUDE_BACKUP_RESTORE_WITH_PASSWORD=y`
- Web UI handles config, backup/restore, telnet/ssh enable via data model
- `etc/reduced_data_model.xml`, `etc/default_config.xml`, `etc/mfg_config.bin`

## Observaciones clave para Detectic

- RootFS read-only, no writable init hooks in `/etc/init.d`
- Único área persistente RW: `/var/run/misc/misc_rw` (data model binary) y `/var/run/runtime_data` opcional
- No hay overlay writable estándar, no hay cron en rcS, no hay directorio de hooks de inicio user-writable
- Hotplug: `/etc/hotplug.d/iface/`, `/etc/hotplug.d/button/`, `/etc/hotplug.d/usb/`, `/etc/hotplug.d/net/`
- Possibles superficies de ejecución: hotplug scripts, rcS_hook (vacío), CGI-triggered execution via data model apply handlers
- Telnet/SSH pueden habilitarse por config → acceso shell runtime
- Sin signature verification en backup → fácil crear config personalizada si se conoce key

**Conclusión 0:** Base capturada. Sistema cerrado, read-only root, persistencia solo vía misc_rw data model. Ningún mecanismo de inicio user-writable evidente en rootfs extraído.
