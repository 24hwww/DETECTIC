# PHASE 12B — HARD SAFETY GATE Y PRUEBAS EN VIVO

## 12B.0 HARD SAFETY GATE

Checklist previo a tocar router:
- Acceso físico confirmado
- Router operativo sirviendo clientes
- Backupcfg pristine exportado vía web UI o shell
```bash
# Exportar
# Vía web UI: Maintenance → Backup
# Vía shell si disponible
cp /var/run/misc/misc_rw/0x00300000 /tmp/pristine.bin
```
- SHA-256 del pristine calculado y guardado OFF ROUTER
```bash
sha256sum pristine_backupcfg.bin > pristine.sha256
```
- Copia fuera del router
- UART recovery disponible y probado
- Firmware/build/version registrado:
```bash
cat /proc/version
cat /etc/platform_ver
```
- ABORT si rollback no disponible

## 12B.1 LIVE STORAGE DISCOVERY

Comandos a ejecutar vía Telnet/SSH habilitado:
```bash
df -h
mount | grep misc_rw
cat /proc/mtd
ubiinfo -d 2 -a
du -sh /var/run/misc/misc_rw
find /var/run/misc/misc_rw -type f | wc -l
```
Criterio: >=12 MB libres → PASS

## 12B.2 PERSISTENCE PROBE

```bash
MARKER=/var/run/misc/misc_rw/.detectic_marker_$(date +%s)
echo "$(date) test" > $MARKER
sha256sum $MARKER > /tmp/marker.sha256
reboot
# tras reboot
cat $MARKER
sha256sum -c /tmp/marker.sha256
```
Verificar salud router.

## 12B.3 EXECUTION PROBE

Transferir binario estático tiny:
```bash
scp probe_arm64 root@router:/var/run/misc/misc_rw/probe
ssh root@router "sha256sum /var/run/misc/misc_rw/probe"
ssh root@router "chmod +x /var/run/misc/misc_rw/probe && /var/run/misc/misc_rw/probe; echo $?"
```
Router health check.

## 12B.4 TELNET CONFIG VALIDATION

Usar pristine backup como base inmutable.
Modificar solo:
```xml
<Device>
  <X_TP_AppCfg>
    <TelnetCfg>
      <Enable>true</Enable>
      <Port>23</Port>
    </TelnetCfg>
  </X_TP_AppCfg>
</Device>
```
Re-encrypt con DES key derivada, restaurar vía web UI.
Esperar reboot, verificar WAN/LAN/WLAN, Telnet LAN access, no WAN exposure.

Si FAIL → restore pristine.

## 12B.5 TELNET PERSISTENCE

Reboot y reconectar Telnet. Verificar persistencia config.

## 12B.6 DETECTIC INSTALLATION

```bash
mkdir -p /var/run/misc/misc_rw/detectic
scp detectic_arm64 root@router:/var/run/misc/misc_rw/detectic/detectic
scp manifest.json sha256 root@router:/var/run/misc/misc_rw/detectic/
ssh root@router "sha256sum -c /var/run/misc/misc_rw/detectic/detectic.sha256"
ssh root@router "chmod +x /var/run/misc/misc_rw/detectic/detectic"
```

## 12B.7 MANUAL DETECTIC BOOT

```bash
ssh root@router "/var/run/misc/misc_rw/detectic/detectic --daemon --log /var/run/misc/misc_rw/detectic/log"
ps | grep detectic
cat /proc/<pid>/status
netstat -tulpn | grep detectic
```
Verificar backend connection y sensor.

## 12B.8 PERSISTENT BINARY TEST

Parar, reboot, verificar binary persiste y checksum ok.

## 12B.9 EXTERNAL LAUNCHER

State machine implementada en controller:
DISCOVER → AUTHENTICATE → VERIFY → DEPLOY → START → HEALTHCHECK → MONITOR

## 12B.10 REBOOT RECOVERY

Reboot router, controller detecta pérdida, espera, reconecta, verifica binary, inicia Detectic.

## 12B.11 CRASH RECOVERY

Matar proceso, controller detecta ausencia, restart con backoff exponencial.

## 12B.12 RESOURCE SAFETY

Monitor:
```bash
free
top -b -n 1
df -h /var/run/misc/misc_rw
du -sh /var/run/misc/misc_rw/detectic
```

## 12B.13 SECURITY GATE

- Telnet solo LAN
- No port forward WAN
- Auth admin
- Controller IP restringido
- Checksum verificación
- Sin shell remoto arbitrario

## 12B.14 DECISION

ALL PASS → arquitectura PROVEN
STORAGE FAIL → LOOP 12C storage optimization
TELNET FAIL → LOOP 12C management alternative
EXECUTION FAIL → LOOP 12C runtime compatibility
DETECTIC FAIL → LOOP 12C sensor/runtime debugging

**Estado:** Plan listo, requiere acceso live al EX520 para ejecutar.
