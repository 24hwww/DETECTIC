# 3. PRUEBA MÍNIMA — Plan de validación

> NO instalar Detectic todavía. Solo probar mecanismos de persistencia/ejecución.

## Prueba A: Marker persistente

Objetivo: confirmar que `/var/run/misc/misc_rw` sobrevive reboot.

Pasos:
1. SSH/Telnet al router
2. `touch /var/run/misc/misc_rw/.detectic_marker && ls -l`
3. Reboot router
4. `ls -l /var/run/misc/misc_rw/.detectic_marker`

Éxito: archivo presente.

Fallo: archivo desaparece → misc_rw no persiste o es recreado.

## Prueba B: Marker en /var/tmp

Objetivo: confirmar volatilidad.

1. `touch /var/tmp/.detectic_marker`
2. Reboot
3. `ls /var/tmp/.detectic_marker`

Esperado: ausente.

## Prueba C: Marker en /var/run/runtime_data (si existe)

1. `test -d /var/run/runtime_data && touch /var/run/runtime_data/.marker`
2. Reboot
3. Verificar.

## Prueba D: Ejecución manual persistente

1. Copiar binario tiny a `/var/run/runtime_data/detectic_test`
2. `chmod +x /var/run/runtime_data/detectic_test`
3. Ejecutar manualmente → verificar salida
4. Reboot → intentar ejecutar desde misma ruta

Éxito: binario presente y ejecutable.

## Prueba E: Autostart vía config

1. Crear backup modificado que habilite Telnet
2. Restaurar
3. Verificar `telnetd` corre
4. Reboot → verificar Telnet vuelve

## Prueba F: Hook de inicio

1. Inspeccionar si rcS lee algún archivo desde misc_rw para ejecutar
2. Buscar en rcS: `grep -n misc_rw` → solo copia mfg_config, no ejecuta
3. Buscar referencias a `/etc/rcS_hook` en binarios: `strings` en `cos`, `init`
4. Si existe lector de scripts en misc_rw, probar colocar script y reboot.

## Criterios de paso

- **Nivel 1**: marker sobrevive reboot en misc_rw → persistencia de datos confirmada
- **Nivel 2**: binario en runtime_data sobrevive y ejecuta → persistencia de código posible
- **Nivel 3**: autostart sin intervención manual → hook descubierto

Si falla Nivel 1 → no hay persistencia de datos, revisar montaje UBI.

Si pasa Nivel 1 pero falla Nivel 2 → ejecutables no permitidos o permisos.

Si pasa Nivel 2 pero falla Nivel 3 → necesario firmware mod o explotación de hook.

## Resultado esperado según análisis estático

- Marker en misc_rw: **SÍ** sobrevive
- Marker en /var/tmp: **NO** sobrevive
- Autostart sin modificar firmware: **NO** esperado
- Habilitar Telnet vía config: **SÍ**

Próximo: Failure Analysis.
