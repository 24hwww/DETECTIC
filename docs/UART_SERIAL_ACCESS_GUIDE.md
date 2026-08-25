# EX520 UART Serial Console — Guía Completa de Acceso

> **Fecha:** 2026-08-25
> **Plataforma:** MediaTek MT7981 (ARM64)
> **Objetivo:** Acceso UART para persistencia total (shell + autostart)

---

## 1. Resumen Ejecutivo

El EX520 tiene UART serial **integrado en el firmware**. No es un exploit — es la consola de desarrollador de MediaTek. El firmware confirma:

| Parámetro | Valor | Fuente |
|-----------|-------|--------|
| Consola | `ttyS0` | `bootargs` en U-Boot |
| Baudrate | `115200` | `bootargs` en U-Boot |
| Formato | `8N1` (8 data, no parity, 1 stop) | Estándar MediaTek |
| Getty | `::askfirst:/sbin/getty -L ttyS0 115200 vt100` | `inittab` |
| UART0 base | `0x11002000` | `earlycon=uart8250,mmio32,0x11002000` |
| Driver | `mediatek,mt7981-uart` / `mediatek,mt6577-uart` | Device Tree |
| Pin RX | `UART0_RXD` (GPIO31) | Pin control en firmware |
| Pin TX | `UART0_TXD` (GPIO32) | Pin control en firmware |

**¿Por qué UART?**
- Acceso shell completo y permanente
- No depende de phoenix.sh ni GTPR
- Funciona incluso si el router está brick
- U-Boot accesible para recovery
- Persistencia total: autostart de servicios al boot

---

## 2.硬件: Pinout Físico del EX520

### 2.1 Ubicación del header UART

El EX520 (idéntico al EX220) tiene pads de UART en el PCB. Basado en el análisis del EX220 (mismo platform MT7981):

```
┌─────────────────────────────────────────────┐
│  TP-Link EX520 — Vista del PCB (superior)   │
│                                              │
│  ┌──────────┐     ┌─────────────────────┐   │
│  │ MT7981   │     │  Pads UART          │   │
│  │ SoC      │     │  (cerca del botón    │   │
│  │          │     │   Reset o LAN)       │   │
│  │          │     │                      │   │
│  │  [UART0]─┼─────┤  ○ TX               │   │
│  │          │     │  ○ RX               │   │
│  │          │     │  ○ GND              │   │
│  │          │     │  (○ VCC — NO USAR)  │   │
│  └──────────┘     └─────────────────────┘   │
│                                              │
│  Puertos LAN: [1][2][3][4]                  │
│  Puerto WAN:  [WAN]                         │
└─────────────────────────────────────────────┘
```

### 2.2 Pinout exacto (3 cables necesarios)

| Pin en PCB | Función | Cable al adapter USB-TTL |
|-----------|---------|-------------------------|
| **TX** | Transmit (datos salen del router) | → **RX** del adapter |
| **RX** | Receive (datos entran al router) | → **TX** del adapter |
| **GND** | Ground (referencia común) | → **GND** del adapter |
| VCC | Alimentación 3.3V | **NO CONectar** |

**⚠️ IMPORTANTE:** TX del router va a RX del adapter y viceversa. Nunca conectar VCC.

### 2.3 Identificar los pads en el PCB

Los pads UART suelen estar en una de estas ubicaciones:

1. **Junto al botón Reset** — 3-4 pads en línea
2. **Junto a los puertos LAN** — pads dorados en el borde
3. **Bajo el disipador de calor** — puede requerir remoción
4. **Junto al chip flash** — pads pequeños

**Para identificarlos:**
1. Buscar 3 pads en línea (o 4 con VCC)
2. Uno tiene serpentina (trace) que va directo al SoC MT7981
3. El pad GND generalmente tiene conexión al plano de tierra (área grande)
4. Usar multímodo en modo continuidad: GND will beep con cualquier punto de tierra

### 2.4 Adapter USB-TTL requerido

| Adapter | Chip | Velocidad | Precio |
|---------|------|-----------|--------|
| CP2102 | CP2102 | hasta 1M baud | ~$3 |
| CH340G | CH340 | hasta 2M baud | ~$2 |
| FT232RL | FTDI | hasta 3M baud | ~$8 |
| PL2303 | Prolific | hasta 1.2M baud | ~$3 |

**Cualquiera funciona.** Configurar a **115200 baud, 8N1**.

---

## 3. Conexión Paso a Paso

### 3.1 Preparar el hardware

```bash
# 1. Desmontar el EX520 (quitar tornillos inferiores, abrir clips)
# 2. Localizar pads UART en el PCB
# 3. Soldar header de3 pines (o usar agujas/pinza cocodrilo)
# 4. Conectar cables:
#    Router TX  → Adapter RX
#    Router RX  → Adapter TX  
#    Router GND → Adapter GND
# 5. NO conectar VCC
```

### 3.2 Conectar al host

```bash
# En Linux:
ls /dev/ttyUSB*          # Verificar que el adapter aparece
# Tipicamente /dev/ttyUSB0

# Velocidad del adapter (si es necesario):
sudo stty -F /dev/ttyUSB0 115200 raw -echo

# Conectar con minicom:
minicom -D /dev/ttyUSB0 -b 115200

# O con screen:
screen /dev/ttyUSB0 115200

# O con picocom:
picocom -b 115200 /dev/ttyUSB0

# O con screen (más simple):
sudo screen /dev/ttyUSB0 115200
```

### 3.3 Encender el router

```bash
# 1. Conectar el EX520 a la alimentación
# 2. Observar el boot log en la terminal serial
# 3. Aparecerá algo como:

U-Boot SPL 2022.10 (...
...
DRAM: 256 MiB
...
NAND: ...
...
Starting kernel ...
[    0.000000] Booting Linux on physical CPU 0x0000000000 [0x0ec95120]
[    0.000000] Linux version 5.4.217 ...
...
[    0.000000] Machine model: MediaTek EX520V124101568249n
...
:: sysinit: /etc/init.d/rcS
...
rcS init done!
```

### 3.4 Login

```bash
# Al final del boot aparece:
EX520 login:

# Intentar credenciales:
# Opción 1: admin / <password del web>
# Opción 2: root / <password del web>
# Opción 3: root / (sin password)
# Opción 4: admin / admin
# Opción 5: root / root

# Si no funciona, U-Boot puede interrumpirse con Enter/ESC durante el boot
# para acceder al shell de U-Boot.
```

---

## 4. Post-Login: Persistencia Total

### 4.1 Verificar el entorno

```bash
# Una vez dentro del shell:
uname -a
cat /proc/cpuinfo
free
df -h
mount
ps
ip addr
iw dev
ls -la /var/run/misc/misc_rw/
```

### 4.2 Habilitar SSH permanente

```bash
# Crear directorio de dropbear
mkdir -p /var/tmp/dropbear

# Generar host keys (si no existen)
dropbearkey -t rsa -f /var/tmp/dropbear/dropbear_rsa_host_key
dropbearkey -t ecdsa -f /var/tmp/dropbear/dropbear_ecdsa_host_key

# Iniciar dropbear en puerto 22
dropbear -R -p 22 \
    -r /var/tmp/dropbear/dropbear_rsa_host_key \
    -r /var/tmp/dropbear/dropbear_ecdsa_host_key &

# Verificar que esté corriendo
ps | grep dropbear
```

### 4.3 Crear script de autostart (persiste reboot)

```bash
# El directorio misc_rw es persistente (UBI)
mkdir -p /var/run/misc/misc_rw/detectic

# Crear script de autostart
cat > /var/run/misc/misc_rw/detectic/autostart.sh << 'EOF'
#!/bin/sh
# Detectic autostart — ejecutado por init o cron

# Iniciar dropbear si no está corriendo
if ! pgrep dropbear > /dev/null 2>&1; then
    mkdir -p /var/tmp/dropbear
    [ -f /var/tmp/dropbear/dropbear_rsa_host_key ] || \
        dropbearkey -t rsa -f /var/tmp/dropbear/dropbear_rsa_host_key 2>/dev/null
    [ -f /var/tmp/dropbear/dropbear_ecdsa_host_key ] || \
        dropbearkey -t ecdsa -f /var/tmp/dropbear/dropbear_ecdsa_host_key 2>/dev/null
    dropbear -R -p 22 \
        -r /var/tmp/dropbear/dropbear_rsa_host_key \
        -r /var/tmp/dropbear/dropbear_ecdsa_host_key 2>/dev/null &
fi

# Iniciar crond si no está corriendo
if ! pgrep crond > /dev/null 2>&1; then
    crond -b 2>/dev/null &
fi
EOF

chmod +x /var/run/misc/misc_rw/detectic/autostart.sh
```

### 4.4 Configurar crond para auto-reinicio

```bash
# Crear directorio de crontabs
mkdir -p /var/run/misc/misc_rw/cron

# Crear crontab que ejecute autostart cada minuto
cat > /var/run/misc/misc_rw/cron/root << 'EOF'
* * * * * /var/run/misc/misc_rw/detectic/autostart.sh
EOF

# Iniciar crond
crond -c /var/run/misc/misc_rw/cron -b

# Verificar
crontab -l
ps | grep crond
```

### 4.5 Habilitar Telnet (alternativa a SSH)

```bash
# Iniciar telnetd en puerto 23
telnetd -p 23 -l /bin/sh &

# Verificar
ps | grep telnetd
```

### 4.6 Hacer que todo arranque en boot (rcS hook)

```bash
# TRUCO: Copiar script a una ubicación que rcS pueda ejecutar
# rcS ejecuta: . /etc/init.d/rcS.model
# rcS.model carga módulos y luego "cos &"
# Podemos crear un script que se ejecute DESPUÉS de cos

# Opción A: Usar el mecanismo de hotplug
# Opción B: Modificar init_console.sh (NO recomendado, es SquashFS)
# Opción C: Usar cron (ya configurado arriba)

# Opción D (mejor): Agregar al final de rcS.model
# NOTA: rcS.model es SquashFS (read-only). No se puede modificar.
# Pero podemos usar el mecanismo de config.bba:

# Verificar si hay algún hook en config.bba
grep -i 'autostart\|hook\|script\|exec' /etc/config.bba 2>/dev/null
```

### 4.7 Instalar Detectic binario

```bash
# Copiar binario a misc_rw (persistente)
# (desde el host, vía scp si SSH funciona, o vía TFTP/U-Boot)

# Si SSH funciona:
scp detectic-aarch64-musl root@<router_ip>:/var/run/misc/misc_rw/detectic/detectic

# Si no, usar TFTP desde U-Boot o copiar vía HTTP:
wget -O /var/run/misc/misc_rw/detectic/detectic http://<host>/detectic
chmod +x /var/run/misc/misc_rw/detectic/detectic

# Ejecutar:
DETECTIC_URL=http://127.0.0.1 \
DETECTIC_USER=admin \
DETECTIC_PASSWORD=<pass> \
DETECTIC_SECRET=<secret> \
DETECTIC_UPLOAD_URL=https://backend/api/v1/events \
/var/run/misc/misc_rw/detectic/detectic sensor &
```

---

## 5. U-Boot: Acceso de Bajo Nivel

### 5.1 Interrumpir el boot

```bash
# Durante el boot, presionar Enter o Escape repetidamente
# Aparecerá el prompt de U-Boot:

MT7981> 

# Ver entorno U-Boot:
printenv

# Verificar variables de consola:
printenv console_tx_control
printenv console_rx_control
```

### 5.2 Habilitar consola via U-Boot

```bash
# Si console_tx_control y console_rx_control no están seteados,
# init_console.sh no hace nada con /proc/tplink/console_control.

# Para forzar la consola:
setenv console_tx_control 1
setenv console_rx_control 1
saveenv
reset

# Esto hace que init_console.sh escriba a /proc/tplink/console_control
# y habilite TX/RX en el UART.
```

### 5.3 Cargar firmware via U-Boot

```bash
# Si el router está brick, U-Boot permite cargar firmware:

# Por TFTP:
setenv ipaddr 192.168.1.1
setenv serverip 192.168.1.100
tftpboot 0x48000000 firmware.bin
nand erase.part firmware
nand write 0x48000000 firmware

# Por serial (XMODEM):
# En minicom: Ctrl+A → S → XMODEM → seleccionar archivo
```

### 5.4 Shell U-Boot completo

```bash
# U-Boot shell disponible:
MT7981> help
MT7981> bdinfo          # Board info
MT7981> mtd info        # Flash partitions
MT7981> nand info       # NAND info
MT7981> md 0x11002000   # Read UART0 registers
MT7981> mmc info         # eMMC/SD info (if present)
```

---

## 6. Verificación Post-UART

### 6.1 Desde el host (SSH)

```bash
# Si SSH fue habilitado:
ssh -o StrictHostKeyChecking=no admin@fe80::3e6a:d2ff:fe5f:abc1%enp2s0

# Verificar que Detectic corre:
ps | grep detectic

# Verificar logs:
cat /var/run/misc/misc_rw/detectic/autostart.log
cat /var/run/misc/misc_rw/detectic/detectic.log
```

### 6.2 Desde UART (serial)

```bash
# Conectar nuevamente al UART:
screen /dev/ttyUSB0 115200

# Verificar procesos:
ps

# Verificar autostart:
ls -la /var/run/misc/misc_rw/detectic/
cat /var/run/misc/misc_rw/cron/root
crontab -l
```

---

## 7. Riesgos y Mitigaciones

| Riesgo | Mitigación |
|--------|-----------|
| Dañar PCB al soldar | Usar pinza cocodrilo o needles en vez de soldar |
| Cortocircuito | Verificar pinout con multímetro antes de conectar |
| Borrar flash | No ejecutar `nand erase` sin backup |
| Brick del router | U-Boot permite recovery vía serial |
| Perder acceso SSH | Mantener conexión UART como backup |
| Seguridad | Deshabilitar UART en producción si no se necesita |

---

## 8. Resumen de Herramientas Necesarias

| Herramienta | Propósito | Dónde |
|------------|-----------|-------|
| USB-TTL adapter (CP2102/CH340) | Conexión serial | Amazon/AliExpress ~$3 |
| Cables jumper macho-macho | Conexión física | Kit de electrónica ~$2 |
| Minicom/screen/picocom | Terminal serial | `apt install minicom` |
| Soldador + estaño (opcional) | Solder headers | Si se quiere header permanente |
| Multímodo | Verificar pines | Verificar GND/TX/RX |

---

## 9. Diagrama de Conexión Completo

```
┌──────────────┐          ┌──────────────────┐
│   HOST PC    │          │   EX520 Router   │
│              │          │                  │
│  /dev/       │          │   PCB UART Pads  │
│  ttyUSB0     │          │                  │
│              │          │   ○ TX ──────────┼──→ RX del adapter
│  USB-TTL ────┼──────────┤                  │
│  Adapter     │          │   ○ RX ──────────┼──→ TX del adapter
│              │          │                  │
│  RX ←────────┼──────────┤   ○ TX           │
│  TX →────────┼──────────┤   ○ RX           │
│  GND ────────┼──────────┤   ○ GND          │
│              │          │                  │
│  minicom -D  │          │   getty ttyS0    │
│  /dev/ttyUSB0│          │   115200 8N1     │
│  -b 115200   │          │                  │
└──────────────┘          └──────────────────┘
```

---

## 10. Referencias

- Firmware: `EX520_UP_BOOT_2025-07-31_11.34.16.bin`
- `bootargs`: `console=ttyS0,115200n1 loglevel=8 earlycon=uart8250,mmio32,0x11002000`
- `inittab`: `::askfirst:/sbin/getty -L ttyS0 115200 vt100`
- `init_console.sh`: controla `/proc/tplink/console_control`
- `tp_board.ko`: kernel module que maneja `console_rx_value` / `console_tx_value`
- Device Tree: `serial@11002000` (uart0), `serial@11003000` (uart1), `serial@11004000` (uart2)
- OpenWrt Forum: [TP-Link EX520 MediaTek MT7981 OpenWrt Support](https://forum.openwrt.org/t/tp-link-ex520-mediatek-mt7981-openwrt-support/241815)
- EX220 Reversing: [Reversing TP-Link EX220 from A1](https://xakcop.com/post/ex220/)
