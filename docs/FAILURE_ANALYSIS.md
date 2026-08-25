# 4. FAILURE ANALYSIS — Qué elimina qué

## Mecanismos de borrado / recreación

### Reboot
- RootFS se remonta ro desde UBI
- `/var/run`, `/var/tmp`, `/tmp` son tmpfs → vaciados
- UBI vols `misc_rw`, `runtime_data` permanecen montados, datos preservados
- `rcS` vuelve a copiar `mfg_config.bin` a `misc_rw/0x00300000` **solo si archivo no existe**
- No hay limpieza de directorio `misc_rw` completo

### Service restart
- Reinicio de `cos`, `cmmsyslogd` no afecta volúmenes
- Reinicio de `dropbear`/`telnetd` no borra config
- Reinicio de `dnsmasq`/`firewall` no afecta persistencia

### Config reload / restore
- `dm_restoreCfg` sobrescribe data model en `misc_rw/0x00300000`
- Restaurar backup borra datos previos del data model, pero **no borra archivos arbitrarios** en `misc_rw`
- Restaurar backup con Telnet habilitado reinicia daemon

### Watchdog
- Watchdog kernel reinicia sistema completo → equivale a reboot
- COS supervisor puede matar procesos no autorizados → Detectic correría riesgo de ser kill

### Filesystem recreation
- UBI vols se formatean solo si no existen: rcS hace `ubiformat` + `ubimkvol` cuando vol no encontrado
- Esto ocurre tras flash limpio o corrupción
- No ocurre en reboot normal

### Firmware upgrade
- Upgrade reemplaza rootFS UBI vols `kernelA/B`, `rootfsA/B`
- `misc_rw` típicamente preservado en upgrade TP-Link, pero **no garantizado**
- `misc_rw_bak` puede usarse para backup

### Factory reset
- Resetea data model a `mfg_config.bin`
- Borra `misc_rw/0x00300000` y recrea desde fábrica
- Archivos adicionales en `misc_rw` **pueden persistir** según implementación; normalmente se formatea vol completo

### Config validation
- Backup/restore verifica MD5 y DES key, rechaza archivos con checksum inválido
- No valida contenido ejecutable

## Por qué falla la persistencia de ejecución

**Causa raíz:** `rcS` no lee ni ejecuta scripts desde volúmenes UBI writable. Solo monta y copia config.

**Evidencia:**
- `grep -r misc_rw /etc/init.d/` → solo copia config
- No hay `source /var/run/misc/misc_rw/*` en rcS
- `/etc/rcS_hook` existe pero no se referencia en rcS extraído

**¿Qué lo elimina?**
- No hay eliminación, simplemente **nunca se carga**.

**Búsqueda de mecanismo superior:**
- ¿Algún binario ejecuta scripts desde misc_rw? Posible `cos` o `cmmsyslogd`
- ¿Hotplug scripts podrían ser sobrescritos via data model? No, son ro
- ¿Existe `INCLUDE_CUSTOM_INIT_SCRIPT`? No en config

## Implicaciones para Detectic

- Persistencia de **datos**: viable en `misc_rw` / `runtime_data`
- Persistencia de **ejecución**: no viable sin modificar `rcS` o encontrar binario que ejecute scripts desde UBI writable
- Alternativa: usar **ejecución disparada por evento** (hotplug, config apply) si dichos eventos pueden ser orquestados remota o automáticamente

Siguiente: Segunda familia de mecanismos.
