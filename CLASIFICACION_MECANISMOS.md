# 2. CLASIFICAR CADA MECANISMO — Criterios

## Matriz de evaluación

Para cada mecanismo:
- writable?
- executable?
- survives reboot?
- survives service restart?
- survives config restore?
- survives firmware upgrade?
- survives factory reset?
- requires root?
- requires SSH/UART?
- requires firmware modification?
- requires changing network behavior?
- CPU/RAM/storage cost?
- detection latency?
- operational risk?

## 1. /var/run/misc/misc_rw — data model UBI

- writable: Sí, binario data model
- executable: No, solo datos config
- survives reboot: Sí
- survives service restart: Sí
- survives config restore: Sí, es destino
- survives firmware upgrade: No, upgrade reemplaza volúmenes
- survives factory reset: No
- requires root: Sí
- requires SSH/UART: Sí para escribir
- requires firmware modification: No
- changing network behavior: No
- cost: bajo
- latency: N/A
- risk: bajo, corrupción de config posible

**Uso:** almacenar binario Detectic? No ejecutable. Solo datos.

## 2. /var/run/misc/misc_rw_bak

- Igual que misc_rw, backup adicional
- Persistencia similar

## 3. /var/run/runtime_data

- writable: Sí si habilitado
- executable: Sí, directorio writable
- survives reboot: Sí
- survives service restart: Sí
- survives config restore: Sí
- survives firmware upgrade: No
- survives factory reset: No
- requires root: Sí
- requires SSH/UART: Sí
- requires firmware modification: No si habilitado
- cost: bajo
- risk: medio

**Uso:** candidato para almacenar binario estático y config.

## 4. /var/tmp / /tmp

- writable: Sí
- executable: Sí
- survives reboot: No
- survives service restart: Sí
- survives config restore: Sí
- survives firmware upgrade: No
- survives factory reset: No
- requires root: No
- requires SSH/UART: Sí
- cost: bajo
- risk: bajo

**Uso:** solo runtime temporal.

## 5. /etc/init.d / rcS

- writable: No, rootFS ro
- executable: Sí
- survives reboot: Sí
- survives service restart: N/A
- survives config restore: Sí
- survives firmware upgrade: No
- survives factory reset: Sí
- requires root: Sí
- requires SSH/UART: Sí
- requires firmware modification: Sí

**Uso:** necesita modificar firmware.

## 6. Hotplug scripts /etc/hotplug.d/*

- writable: No, rootFS ro
- executable: Sí
- survives reboot: Sí
- requires firmware modification: Sí

## 7. Habilitar Telnet/SSH vía backup/restore

- writable: N/A
- executable: Habilita daemon
- survives reboot: Sí, config persistente
- survives service restart: Sí
- survives config restore: Sí
- survives firmware upgrade: No
- survives factory reset: No
- requires root: No, via web
- requires SSH/UART: No
- requires firmware modification: No
- cost: bajo
- risk: medio-alto (expone shell)

**Uso:** puerta de entrada a shell runtime.

## 8. Cron / crond iniciado manualmente

- writable: Dir crontab necesita writable
- executable: Sí
- survives reboot: No sin hook de inicio
- requires root: Sí
- requires firmware modification: No
- risk: medio

## 9. Config persistence via data model

- writable: Sí vía API
- executable: No directo
- survives reboot: Sí
- survives factory reset: No
- risk: bajo

## 10. Web CGI / Aginet trigger

- writable: No
- executable: Indirecto vía handlers
- risk: alto si vulnerabilidad

## Resumen rápido

- **Persistencia de código:** No hay ubicación writable+executable que sobreviva reboot sin modificar firmware. `runtime_data` podría serla si existe y es writable+executable.
- **Persistencia de datos:** `misc_rw` sí.
- **Ejecución persistente:** Requiere modificar rootFS o encontrar hook ejecutable en UBI writable que sea sourced en rcS. `rcS_hook` vacío sugiere posibilidad histórica pero no implementada.
- **Ejecución temporal:** Shell vía Telnet/SSH habilitado por config.

Conclusión: Mecanismo viable sin firmware mod es: habilitar SSH/Telnet → shell runtime → ejecutar Detectic desde ubicación writable temporal, sin persistencia de arranque. Persistencia real requiere firmware mod o encontrar hook ejecutable en UBI que rcS lea.
