# 8. FAILURE / RECOVERY

## Escenarios

### matar Detectic → ¿se recupera?
Con external launcher: sí, agente externo puede reiniciar.
Con autostart: depende de supervisor.

### crash → ¿reinicia?
Sin supervisor, no. Con external launcher, sí.

### binary corrupto → ¿router sigue funcionando?
Sí, router independiente. Solo Detectic falla.

### backend offline → ¿router sigue funcionando?
Sí, sensor local sigue operando, buffer local en misc_rw.

### disk full → ¿router sigue funcionando?
UBI vol limitado. Llenar misc_rw puede afectar data model → riesgo.

### reboot → ¿Detectic vuelve?
Con binary+config+SSH: no automático, requiere login.
Con firmware mod: sí.

### firmware update → ¿Detectic vuelve?
No, update reemplaza rootFS. Binario en misc_rw persiste, pero autostart se pierde si requiere firmware mod.

## Estrategias de recuperación

1. **Health check**: external agent verifica proceso cada X min
2. **Rollback**: mantener versión anterior de binario en misc_rw
3. **Buffer limitado**: circular buffer de eventos, max 10MB
4. **Watchdog software**: script que reinicia Detectic si muere
5. **Config backup**: exportar config que habilita SSH

## Requisitos

- Tamaño binario < 5MB
- Uso RAM < 32MB
- Sin dependencias externas
- Logging a syslog con rotación

Conclusión: arquitectura con external launcher proporciona mejor recuperación sin modificar firmware.
