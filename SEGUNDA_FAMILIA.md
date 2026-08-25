# 5. SEGUNDA FAMILIA — Alternativas sin persistencia de filesystem

Si la persistencia de filesystem no existe → explorar mecanismos indirectos.

## A. Configuración persistente

- Data model puede almacenar strings/paths arbitrarios
- Almacenar path a binario en `misc_rw` vía config
- Al aplicar config, handler puede ejecutar comando? No documentado
- Riesgo: datos persisten, ejecución no garantizada

## B. Backup / config restore

- Backup/restore es vector para cambiar config
- Puede usarse para habilitar Telnet/SSH cada reboot
- Habilitar Telnet persistente → shell disponible cada boot
- Con shell, ejecutar Detectic manualmente o vía script en `/var/tmp` copiado desde `misc_rw` cada boot
- Requiere interacción manual o automatización externa

## C. Eventos del sistema

- Hotplug iface: al subir interfaz WiFi, ejecuta scripts ro
- No modificable, pero puede observarse
- Eventos de wireless: `iw` events, `netifd` hooks
- Podría usar `inotify` sobre data model para disparar acción

## D. Supervisor existente

- `cos` daemon gestiona servicios
- Analizar `cos` para ver si acepta plugins o lee config para lanzar procesos
- Posible registrar servicio en data model y `cos` lo lanza
- Requiere ingeniería inversa

## E. API / CGI

- Web UI puede ejecutar comandos vía `rsl_sys_*` handlers
- CGI puede disparar ejecución si existe vulnerabilidad de inyección
- `INCLUDE_WEB_TELNET=y` ya permite control via web

## F. External launcher

- Controlador externo (Cloud, TR-069) puede aplicar config periódicamente
- Forzar re-aplicación de config que habilita Telnet → mantener acceso
- No ejecución directa, solo habilitación

## G. Staged launcher + persistent config

1. Config persistente habilita Telnet
2. Al iniciar, router crea `/var/tmp` vacío
3. Script externo copia binario Detectic desde `misc_rw` a `/var/tmp` y ejecuta
4. Requiere agente externo que se conecte tras boot

## H. Fallback launcher + primary launcher

- Primary: intento de autostart vía firmware mod
- Fallback: si no arranca, SSH habilitado y script de recuperación copia binario

## Evaluación rápida

| Método | Persistencia | Ejecución | Riesgo |
|--------|--------------|-----------|--------|
| Config persistente | Sí | No | Bajo |
| Backup/restore habilita Telnet | Sí | Manual | Medio |
| Eventos hotplug | No | No | Bajo |
| Supervisor cos | ? | ? | Alto |
| API/CGI | Sí | Indirecto | Alto |
| External launcher | Sí | Indirecto | Medio |

Conclusión: la familia más prometedora sin modificar firmware es **habilitar Telnet/SSH vía config persistente** y usarlo como puerta de entrada para ejecutar Detectic desde almacenamiento persistente. El autostart seguirá siendo manual o requiere agente externo.
