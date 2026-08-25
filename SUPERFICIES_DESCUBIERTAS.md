# 1. DESCUBRIR SUPERFICIES — TP-Link EX520V

## Filesystem writable

- `/var/run/misc/misc_rw` — UBI vol rw, montado por rcS. Contiene data model binario `0x00300000`. **Persistente** tras reboot.
- `/var/run/misc/misc_rw_bak` — UBI vol rw si DUAL_CONFIG. Persistente.
- `/var/run/misc/misc_ro` — UBI vol ro.
- `/var/run/misc/misc_isp` — UBI vol ro si OPTION66.
- `/var/run/runtime_data` — UBI vol rw si RUNTIME_DATA_SECTION. Persistente.
- `/var/tmp` — directorio creado en rcS, típicamente tmpfs pero puede persistir en flash según implementación. No confiable para persistencia.
- `/var/log` — creado rcS, tmpfs.
- `/var/lock` — tmpfs.
- `/var/run` — tmpfs.
- `/tmp` → symlink a `/var/tmp`.

**Conclusión:** única escritura persistente real en UBI: `misc_rw`, `misc_rw_bak`, `runtime_data` opcional.

## /etc /etc/init.d /etc/rc*

- RootFS SquashFS/UBI ro. Archivos no modificables en runtime.
- `/etc/init.d/rcS` — script de arranque principal, ro.
- `/etc/init.d/rcS.model` — sourced por rcS, ro.
- `/etc/init.d/init_console.sh`, `firmware.sh` — ro.
- `/etc/rcS_hook/` — existe pero vacío `.gitkeep`. No se ejecuta en rcS extraído.

## Startup scripts

- Init via BusyBox `inittab` → `::sysinit:/etc/init.d/rcS`
- No hay `/etc/rc.local`
- No hay directorio de hooks user-writable en rcS
- `rcS` no lee archivos de configuración de usuario para ejecutar scripts, solo monta volúmenes y copia `mfg_config.bin` a `misc_rw` en primer boot.

## Config database

- Data model binario en `/var/run/misc/misc_rw/0x00300000`
- Backup/restore XML → data model vía `dm_restoreCfg`/`dm_saveCfg`
- No UCI. Uso de data model propietario TP-Link.

## NVRAM / UCI-like storage

- No NVRAM tradicional. Persistencia vía UBI misc_rw + data model binario.
- Valor DeviceInfo usado para clave de backup almacenado en data model.

## UBIFS writable volumes

- `misc_rw`, `misc_rw_bak`, `runtime_data`
- Montados en rcS con `mount -t ubifs ubiX:...`
- Permanecen tras reboot, sobreviven service restart, no sobreviven factory reset? Factory reset borra misc_rw.

## /tmp + mecanismos de reconstrucción

- `/tmp` → `/var/tmp`, creado en rcS.
- Recreated cada boot. No persistente.

## cron / crond / at

- BusyBox `crond` compilado pero no iniciado en rcS.
- No crontab directory writable en rootfs.
- Podría iniciarse manualmente desde shell con `-c <dir>` pero no persistiría tras reboot sin hook.

## watchdogs

- Kernel watchdog y `MULTICORE_WATCHDOG` opcional.
- `cos` daemon actúa como supervisor.

## hotplug / netifd / wireless events

- `/etc/hotplug.d/iface/` — scripts ejecutados en up/down de interfaces
  - `00-netstate`, `10-sysctl`, `20-firewall`, `25-ddns`, `30-teql`, `40-streamboost`, `50-mcproxy`, `50-miniupnpd`, `60-dnsmasq`, `65-pppoe`, `70-quagga`, `40-dhcp6c`
- `/etc/hotplug.d/net/` — `20-wsplcd`, `30-hyd`
- `/etc/hotplug.d/button/` — `00-button`
- `/etc/hotplug.d/usb/` — `10-usb`
- `/etc/hotplug.d/dhcp6c/` — `10-dnsmasq`, `20-radvd`
- `/etc/hotplug.d/firewall/` — `10-nat-reflection`, `20-streamboost`
- Scripts son de rootfs ro, no user-writable.

## Existing daemon supervisor

- `cos` — TP-Link COS daemon, iniciado en rcS
- `cmmsyslogd`
- `ve_vtsp_main`

## TP-Link proprietary service manager

- COS + data model apply handlers
- Aplicación de config dispara scripts vía handlers

## web/CGI-triggered execution

- Web UI configura `DEV2_SSH_CFG`, `DEV2_TELNET_CFG`
- Aplicar config ejecuta `dropbear`/`telnetd` vía handler
- Posible ejecución vía `Aginet` app triggers

## backupcfg / conf.bin

- Configuración persistente cifrada DES
- No ejecución arbitraria, solo config

## GDPR configuration persistence

- `INCLUDE_LOGIN_GDPR_ENCRYPT=y`
- Config guardada en data model, persiste en misc_rw

## DHCP/network hooks

- Hotplug iface scripts mencionados

## DNS/firewall hooks

- Hotplug firewall scripts

## USB/storage auto-mount hooks

- `/etc/hotplug.d/usb/10-usb`

## firmware upgrade restore hooks

- `rcS` verifica existencia de `0x00300000`, copia `mfg_config.bin` si falta
- Upgrade borra misc_rw? Dependiente

## external controller-triggered execution

- Aginet app V2, Cloud, TR-069 CWMP pueden aplicar config remota

## Resumen superficies candidatas

1. **Persistencia de datos**: `/var/run/misc/misc_rw`, `runtime_data`
2. **Ejecución en arranque**: Ninguna hook user-writable en rootfs
3. **Ejecución en evento**: Hotplug scripts son ro, no modificables
4. **Ejecución vía config**: Habilitar Telnet/SSH vía backup/restore → shell runtime
5. **Ejecución vía CGI**: Posible inyección en handlers si existe vulnerabilidad

Siguientes pasos: clasificar cada mecanismo con criterios de persistencia.
