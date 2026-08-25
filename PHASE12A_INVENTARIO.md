# 12A.0 INVENTARIO DE EVIDENCIA

## Evidencia disponible
- PHASE11_VALIDATION.md — validación runtime, runtime_data deshabilitado, misc_rw confirmado
- CAPTURA_BASE.md — firmware MT7981 ARM64, rootFS ro, UBI misc_rw rw
- SUPERFICIES_DESCUBIERTAS.md — superficies RW/RO, hotplug, rcS_hook
- MATRIZ_FINAL.md — arquitectura seleccionada external launcher + misc_rw
- BACKUPCFG_ANALYSIS.md — formato DES-ECB + zlib XML, key derivation
- _rootfs/ — filesystem extraído
- Cargo.toml — Detectic Rust, target aarch64-unknown-linux-musl, opt-level z, lto
- Binary existente: target/aarch64-unknown-linux-musl/release/detectic — 1.3 MB estático

## 12A.1 MISC_RW CAPACITY MODEL

Estimación de necesidades:
- Detectic binary estático: 1.3 MB
- Runtime DB: 0 MB (sensor sin persist, backend)
- Offline queue buffer: máx 5 MB
- Logs rotados: 1 MB
- Temporary files: 0.5 MB
- Upgrade copy: 1.3 MB backup
- Safety margin 50%

Total mínimo requerido: ~12 MB libre en misc_rw

Si misc_rw <12 MB → insuficiente. Alternativa: usar misc_rw_bak o reducir queue.

## 12A.2 DETECTIC ARTIFACT AUDIT

Binary audit:
- Arquitectura: ARM64, static musl
- Dependencias dinámicas: ninguna
- Librerías requeridas: ninguna
- Directorios requeridos: ninguno
- Variables de entorno: opcionales
- Capabilities: ninguna especial
- Sockets: TCP a backend HTTPS + HTTP a router local
- Writable paths: /var/run/misc/misc_rw/detectic/state
- Persistent state: queue archivo en misc_rw

Cumple requisitos router.

## 12A.3 MINIMAL DEPLOYMENT PACKAGE

Contenido paquete:
- detectic binary 1.3 MB
- manifest.json {version, arch, sha256}
- SHA-256 checksum
- launcher script bash 200 bytes
- health check script

Total ~1.5 MB

## 12A.4 LAUNCHER STATE MACHINE

Estados:
DISCOVER → AUTHENTICATE → VERIFY → DEPLOY → START → HEALTHCHECK → MONITOR → RESTART → BACKOFF → REPROVISION → ROLLBACK

Transiciones por eventos: router online, Telnet disponible, proceso muerto, checksum mismatch, reboot detectado.

## 12A.5 TELNET REQUIREMENTS

Data-model parámetro:
- `Device.X_TP_AppCfg.TelnetCfg.Enable` = true
- `Device.X_TP_AppCfg.TelnetCfg.Port` = 23
- `Device.X_TP_AppCfg.TelnetCfg.Access` = LAN

Persistencia: config guardada en misc_rw, sobrevive reboot
Exposición: LAN only, no WAN
Credenciales: user/password de admin router
Rollback: restore backup pristine

## 12A.6 BACKUPCFG FORMAT VALIDATION

Confirmado:
- DES-ECB
- zlib comprimido
- XML estructura
- MD5 integrity
- Key derivation: constante XOR DeviceInfo 0x51c
- Padding a múltiplo 8
- Restore valida MD5

Estado: formato conocido, modificar backup es seguro si se respeta cifrado.

## 12A.7 SAFE BACKUP WORKFLOW

1. pristine backup → immutable copy
2. Decodificar con clave conocida → XML
3. Modificar solo Telnet flags
4. Re-encrypt con misma clave
5. Guardar rollback artifact
6. Nunca modificar backup original

## 12A.8 LIVE TEST PLAN

Test #1 misc_rw: df, mount, libre
Test #2 marker persistence: touch → reboot → verify
Test #3 Telnet persistence: habilitar → reboot → telnet
Test #4 ARM64 probe: ejecutar busybox? test binario pequeño
Test #5 Detectic: transferir binary 1.3 MB → ejecutar
Test #6 reboot recovery: binary persiste y relanzar

## 12A.9 STOP

Esperando acceso live EX520 para ejecutar tests 12A.8.

Estado: inventario completo, modelo de capacidad listo, paquete mínimo definido.
