# PHASE 12 — SAFETY GATE Y VALIDACIÓN EN VIVO

## 12.0 SAFETY GATE

Pre-requisitos antes de tocar router vivo:
- [ ] Router sirviendo clientes en producción
- [ ] WAN/LAN/WLAN estado conocido y documentado
- [ ] Ruta de rollback confirmada: backupcfg pristine exportado y almacenado fuera del router
- [ ] Console/UART recovery disponible y probado
- [ ] NO firmware modification

Comandos de seguridad:
```bash
# Exportar config actual
# Necesita shell activo
# Guardar backupcfg.bin y config XML
cp /var/run/misc/misc_rw/0x00300000 /tmp/misc_rw_dump.bin
# O vía web UI backup

# Verificar servicios
ps | grep -E 'httpd|cos|dropbear|telnetd'
ifconfig
iw dev
```

## 12.1 LIVE MISC_RW DISCOVERY

Conectar vía Telnet/SSH después de habilitar.

Comandos:
```bash
df -h
mount | grep misc_rw
cat /proc/mtd
ubinfo -d 2 -a
du -sh /var/run/misc/misc_rw
ls -la /var/run/misc/misc_rw
```

Cálculo necesario:
- Detectic binario ARM64 estático ~2-5 MB
- DB runtime ~1 MB
- Logs ~1 MB
- Offline queue ~5 MB
- Safety margin 50%

Umbral: misc_rw libre > 15 MB → suficiente

## 12.2 SAFE WRITE PROBE

```bash
mkdir -p /var/run/misc/misc_rw/detectic_test
echo "marker_$(date +%s)" > /var/run/misc/misc_rw/detectic_test/marker.txt
cat /var/run/misc/misc_rw/detectic_test/marker.txt
chmod +x /var/run/misc/misc_rw/detectic_test/probe
```

## 12.3 REBOOT PERSISTENCE TEST

```bash
echo "test" > /var/run/misc/misc_rw/detectic_persist
sha256sum /var/run/misc/misc_rw/detectic_persist > /tmp/checksum
reboot
# tras reboot
sha256sum -c /tmp/checksum
```

## 12.4 TELNET CONFIG VALIDATION

Flujo:
1. Backup pristine guardado
2. Modificar backup XML para habilitar Telnet: `Device.X_TP_AppCfg.TelnetCfg.Enable = true`, Port 23
3. Re-encrypt con clave derivada
4. Restaurar vía web UI
5. Verificar `telnetd` en proceso

## 12.5 TELNET + MISC_RW INTEGRATION

Verificar permisos:
```bash
ls -ld /var/run/misc/misc_rw
touch /var/run/misc/misc_rw/test_write
rm /var/run/misc/misc_rw/test_write
```

## 12.6 DETECTIC DEPLOYMENT PROBE

Probar binario mínimo:
```bash
/var/run/misc/misc_rw/detectic_probe --version
ps | grep detectic
```

## 12.7 REAL DETECTIC DEPLOYMENT

```bash
scp detectic_arm64 /var/run/misc/misc_rw/detectic
sha256sum -c detectic.sha256
chmod +x /var/run/misc/misc_rw/detectic
/var/run/misc/misc_rw/detectic --daemon
```

## 12.8 EXTERNAL LAUNCHER

Controller logic:
- Detect router via ping
- Telnet connect
- Check process `ps | grep detectic`
- If absent → start
- Health check cada 60s

## 12.9-12.15 Pruebas de recuperación

Se describen en checklist. Requiere acceso físico.

## 12.16 SECURITY HARDENING

- Telnet LAN-only, no port forward
- Credenciales fuertes
- Firewall router bloquea WAN→Telnet
- Binary checksum verificado
- Command allowlist en controller

## 12.17 OPERATIONAL METRICS

Medir:
- boot → Detectic start latency
- RAM: `cat /proc/meminfo`
- CPU: `top`
- Storage: `df`

## 12.18 FINAL DECISION

Si todos los tests críticos pasan → PRIMARY = external launcher + misc_rw

**Estado actual:** No se puede completar sin acceso live al EX520. Los pasos 12.1-12.18 requieren router físico con Telnet habilitado.

Próximo paso requerido: habilitar Telnet vía backup config y obtener acceso shell.
