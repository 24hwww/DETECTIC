# 7. COMBINACIONES — Arquitecturas A-H

## A. Binary + init
Binary estático en `runtime_data`, init script modificado en firmware para ejecutarlo.
Pros: autostart completo.
Contras: requiere firmware mod, riesgo alto.
Viabilidad: baja sin acceso a firmware.

## B. Binary + existing supervisor
Binary en `runtime_data`, registrar en data model para que `cos` lo lance.
Pros: sin modificar init.
Contras: requiere ingeniería inversa de `cos`.
Viabilidad: media-baja.

## C. Binary + config persistence
Binary en `runtime_data`, config persistente habilita Telnet. Usuario ejecuta manualmente tras boot.
Pros: sin firmware mod.
Contras: sin autostart.
Viabilidad: alta.

## D. Binary + watchdog
Watchdog reinicia router si Detectic muere. Usar script externo que monitoree.
Pros: resiliencia.
Contras: watchdog puede reiniciar router innecesariamente.
Viabilidad: baja.

## E. Binary + network event
Disparar ejecución al hacer up de wlan0 vía hotplug. Hotplug scripts son ro, no modificables.
Viabilidad: muy baja.

## F. Binary + external launcher
Agente externo se conecta por SSH tras boot y lanza Detectic.
Pros: persistencia sin modificar router, control centralizado.
Contras: dependencia de conectividad y SSH habilitado.
Viabilidad: alta.

## G. Staged launcher + persistent config
Etapa 1: config persistente habilita SSH
Etapa 2: SSH permite copiar binario a `runtime_data`
Etapa 3: script de inicio en router verifica binario y ejecuta
Pros: combinación de persistencia y ejecución.
Contras: requiere script de inicio.
Viabilidad: media.

## H. Fallback launcher + primary launcher
Primary: intentar autostart vía firmware
Fallback: si no arranca, SSH recovery.
Viabilidad: alta complejidad.

**Recomendación preliminar:** Combinación C + F.
- C: binary en `runtime_data` persistente
- F: external launcher por SSH
- Opcional: config persistente habilita SSH automáticamente

Score combinado: persistencia de datos + ejecución controlada externamente.
