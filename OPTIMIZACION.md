# 6. OPTIMIZACIÓN — Puntuación de métodos viables

Puntuación = persistence_score + deployment_simplicity + boot_reliability + resource_efficiency + rollback + firmware_compatibility - operational_risk

Escala 0-10 por criterio.

## Método 1: Firmware mod + init script

- persistence: 10
- deployment simplicity: 2
- boot reliability: 9
- resource efficiency: 9
- rollback: 3
- firmware compatibility: 3
- operational risk: 8
Score: 10+2+9+9+3+3-8 = 28

## Método 2: Binary en runtime_data + Telnet habilitado por config

- persistence: 6 (binario persiste, ejecución no)
- deployment simplicity: 7
- boot reliability: 4 (requiere login manual)
- resource efficiency: 8
- rollback: 9
- firmware compatibility: 9
- operational risk: 5
Score: 6+7+4+8+9+9-5 = 38

## Método 3: Config persistente habilita Telnet + script de recuperación externo

- persistence: 7
- deployment simplicity: 6
- boot reliability: 5
- resource efficiency: 8
- rollback: 9
- firmware compatibility: 9
- operational risk: 6
Score: 7+6+5+8+9+9-6 = 38

## Método 4: Hotplug hook modificado vía firmware

- persistence: 10
- deployment simplicity: 3
- boot reliability: 8
- resource efficiency: 7
- rollback: 4
- firmware compatibility: 4
- operational risk: 7
Score: 10+3+8+7+4+4-7 = 29

## Método 5: Supervisor cos plugin vía data model

- persistence: 8
- deployment simplicity: 4
- boot reliability: 7
- resource efficiency: 7
- rollback: 6
- firmware compatibility: 5
- operational risk: 8
Score: 8+4+7+7+6+5-8 = 29

## Método 6: Backup/restore solo para datos, sin ejecución

- persistence: 9
- deployment simplicity: 8
- boot reliability: 9
- resource efficiency: 9
- rollback: 9
- firmware compatibility: 9
- operational risk: 2
Score: 9+8+9+9+9+9-2 = 51

**Conclusión:** Método 6 es más seguro pero no ejecuta. Entre métodos ejecutables sin firmware mod, Método 2/3 lidera con 38 puntos.

Siguiente: Combinaciones.
