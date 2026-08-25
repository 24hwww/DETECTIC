# Informe de Análisis del Proyecto Detectic

> **Fecha:** 2026-08-22
> **Alcance:** Análisis estático y de ejecución de todo el repositorio
> (`src/`, `python/`, `backend/`, `investigations/backupcfg/`, firmware extraído).
> **Objetivo del proyecto (AGENTS.md):** convertir un router TP-Link EX520V en un
> nodo sensor Wi-Fi que observe dispositivos, agregue localmente, seudonimice y
> envíe eventos a un backend, respetando privacidad y recursos limitados.

---

## 1. Resumen ejecutivo

Detectic es un proyecto **multi-componente y funcional** que avanza por dos vías
que no siempre coinciden con el plan original de AGENTS.md:

1. **Vía de sensorización por API (funciona SIN shell).** El router EX520 expone
   la API GTPR/GDPR (`/cgi/getGDPRParm`, `/cgi_gdpr?9`, `/cgi_gdpr`). Con las
   credenciales web se obtiene el mapa de red completo (dispositivos Wi-Fi con
   RSSI, IP, MAC, hostname, estándar, OneMesh) sin necesidad de SSH ni de
   modificar el firmware. Esto **cumple los hitos M2–M6** del proyecto.
2. **Vía de despliegue por backup/restore (bloqueada).** La ingeniería inversa
   de `backupcfg.bin` está muy avanzada, pero el backup de muestra está
   **protegido con contraseña no vacía**, y el mecanismo de restore es solo de
   configuración (no permite ejecutar código ni persistir). No es una vía de
   despliegue viable por sí sola.

El código Rust compila y **los 15 tests pasan** (9 de librería + 5 de `main` + 1
de store). El cliente Python de referencia y el router mock funcionan como
banco de pruebas. El backend de ingesta está implementado.

---

## 2. Estructura del repositorio

```
detectic/
├── src/                      # Sensor Rust (binario + librería)
│   ├── main.rs               # CLI: capture/map/stats/report/sensor
│   ├── gtpr.rs               # Cliente GTPR/GDPR (AES-128-CBC + RSA "sign")
│   ├── crypto.rs             # AES, RSA(pub), MD5, HMAC-SHA256, seudonimización
│   ├── model.rs              # Device / NetworkMap / MapDiff
│   ├── oids.rs               # OIDs y structs de respuesta
│   ├── store.rs              # Persistencia SQLite (feature "persist")
│   └── lib.rs
├── python/                   # Referencia Python + herramientas de recon
│   ├── detectic_client.py    # Cliente GTPR (equiv. Rust)
│   ├── mock_router.py        # Mock GDPR para test sin hardware
│   ├── router_recon.py       # Recon de puertos/debug (solo lectura)
│   ├── crack_login.py        # Recupera key/IV de login desde pcap
│   └── analyze_pcap.py       # Decodifica sesión GDPR desde pcap
├── backend/
│   └── server.py             # API de ingesta (M6)
├── investigations/backupcfg/ # Ingeniería inversa del backup del firmware
│   ├── REPORT.md             # Informe de la investigación
│   ├── poc/                  # derive_key / decrypt / encrypt
│   └── reversing/            # Scripts + disassembly + DeviceInfo_REVERSE.md
├── _rootfs/                  # Rootfs extraído del firmware (libcmm.so, httpd…)
├── _fw_extract/              # Firmware extraído (boot/bin)
├── ex520-network-map-gdpr.md # Descubrimiento de la API (funciona sin shell)
├── BACKUPCFG_ANALYSIS.md     # Análisis del backupcfg.bin
└── AGENTS.md                 # Directrices del proyecto
```

---

## 3. Componente A — Sensor Rust (`src/`)

**Estado: COMPLETO y verificado.** Compila con y sin la feature `persist`;
`cargo test` → 15/15 OK.

### 3.1 `crypto.rs`
- `aes128_cbc_encrypt/decrypt`: AES-128-CBC con PKCS#7 (usa `cbc` + `aes`).
- `rsa_sign_public`: firma TP-Link con la **clave pública** (`m^e mod n` sobre
  bloque PKCS#1 v1.5 *signature*). Comentado que no es firma privada real; requiere
  verificación en vivo (coincide con `mock_router.py` que verifica con `d`).
- `gen_login_aes_pair`: key/iv de 16 bytes ASCII = `<unix_ms(13)><3 dígitos
  aleatorios>` (réplica de `@hertzg/tplink-api`).
- `pseudonymize` / `hmac_sha256_hex`: HMAC-SHA256 (privacidad + auth de upload).
- Tests: vectores MD5 y HMAC-SHA256 (RFC 4231) correctos.

### 3.2 `gtpr.rs`
- Implementa el flujo completo: `getGDPRParm` → login (`cgi_gdpr?9`) →
  `TokenID` desde `/` → `gl`/`go` cifrados.
- `Dialect::{GdprJson, GdprText}`: json usa `sign` hex; text usa `sign` base64.
- `network_map()`: fusiona **3 OIDs** (`WIFI_APDEV_ASSOCDEV`, `DHCPV4_CLIENT`,
  `HOST_ENTRY`) en una lista unificada; el Wi-Fi (con RSSI) es fuente primaria y
  DHCP/host rellenan IP/hostname (`parse_network_map`, bien testeado).
- `canon_mac`: normaliza variantes de MAC para el merge.

### 3.3 `model.rs` / `oids.rs`
- `Device` con campos `Option`, `identity()` = MAC→IP→hostname.
- `NetworkMap::diff` para detección de cambios.
- Structs de respuesta deserializan los nombres del firmware
  (`signalStrength`, `MACAddress`, `opStandard`, `radioMAC`, `stack`…).

### 3.4 `store.rs` (feature `persist`)
- SQLite con tablas `snapshots` / `devices` e índices.
- `device_aggregates` (M4): first/last seen,观测数, avg/min/max RSSI por
  seudónimo. Test `aggregates_two_snapshots` OK.

### 3.5 `main.rs` (CLI + daemon)
- Subcomandos: `capture`, `map`, `stats`, `report`, `sensor`.
- `sensor`: bucle de polling con **buffer offline JSONL acotado**
  (`append_bounded`, nunca llena el fs), **reintentos con backoff exponencial**
  (`backoff_delay` 1→2→4→8 s, cap 8 s) y **auth HMAC** por upload.
- `UploadPayload`: solo envía `pseudonym`, `rssi`, `standard`, `source` —
  **nunca MAC/IP/hostname/raw** (cumple AGENTS.md §21/§39).
- **Sin defaults inseguros**: password y secret son obligatorios (bueno).

---

## 4. Componente B — Python (`python/`)

**Estado: funcional como referencia/herramientas; tiene defaults inseguros.**

- `detectic_client.py`: reimplementa el protocolo (_requests + cryptography).
  - ✅ Útil para experimentar sin compilar Rust.
  - ⚠️ **Defaults inseguros**: `--password default="<REDACTED>"` y
    `--secret default="64656474656374696300"` (= `"detectic\0"`). Contradice
    AGENTS.md §11/§40 (el Rust sí los exige).
  - ⚠️ `network_map()` solo consulta `DEV2_WIFI_APDEV_ASSOCDEV` (1 OID), **sin**
    merge DHCP/host → más débil que el Rust (3 OIDs).
- `mock_router.py`: mock GDPR funcional (RSA-1024, verifica sign con `d`,
  cifra respuestas con key/iv de sesión). Permite testear el cliente Rust sin
  hardware. Compatible con el flujo de `gtpr.rs`.
- `router_recon.py`: recon **solo lectura** (TCP scan, banner telnet, TDDP
  GET, sondas de debug/backup). Cumple "no modificar el router".
- `crack_login.py` / `analyze_pcap.py`: recuperan key/iv de un login capturado
  vía known-plaintext (prefijo `8\r\n[/cgi/login#0` o JSON). Requieren
  `epoch` y (para json) user/password. Herramientas de diagnóstico, no core.

---

## 5. Componente C — Backend (`backend/server.py`)

**Estado: implementado (M6).**

- `POST /api/v1/events`: autentica con `X-Detectic-Sensor` +
  `X-Detectic-Signature` = HMAC-SHA256(secret, body). Persiste en SQLite.
- `GET /api/v1/devices`: agregados por seudónimo. `GET /api/v1/healthz`.
- Acepta el seudónimo del sensor; si falta, deriva uno (solo compat).
- ⚠️ **Defaults de dev**: master `"dev-master-secret"`, sensor
  `"dev-secret-change-me"` en `sensors.json`. Aceptable en dev, debe cambiarse
  en prod.
- ⚠️ **Privacidad**: almacena `hostname`/`ip` en claro además del seudónimo. El
  diseño (AGENTS.md §21/§39) prefiere no guardar identificadores crudos; el
  sensor no los envía, pero el backend los persiste si el cliente los incluye.
  Recomendado: no almacenar hostname/ip en el backend.

---

## 6. Componente D — Ingeniería inversa del firmware (`investigations/backupcfg/`)

**Estado: muy avanzada; bloqueada por contraseña desconocida.**

### 6.1 Formato `backupcfg.bin` (PROBADO por análisis binario)
`DES-ECB( MD5(16) + orig_size(4 LE) + zlib(XML) )`.
Evidencia: `rsl_sys_backupCfg` / `rsl_sys_restoreCfg` en `libcmm.so`,
`util_en_desMinDo`, `util_en_compressBuff`, `util_en_md5MakeDigest` en
`libcutil.so`.

### 6.2 Derivación de clave (PROBADO)
`getBackNRestoreK(value)`:
```
constante = 74 8d a5 0b f9 3e 2d cf
key[i] = constante[i] ^ ord("%08x" % (value & 0xffffffff))[i]
```
`value` = u32 leída de `DeviceInfo` objeto 0, instancia 2, **offset 0x51c**
(`dm_getObj(0,2,…,0x6e8,…)`; `sp+0x554 − sp+0x38 = 0x51c`). Confirmado en
`getBackNRestoreK.txt` y `DeviceInfo_REVERSE.md`.

Con contraseña: `getBackNRestoreKeyWithPwd` XOR-cíclico del MD5(pass) sobre la
clave de 8 bytes.

### 6.3 Hallazgo reciente (esta sesión)
Se corrigió un **bug en `analyze_devinfo.py`**: el escaneo de argumentos de
`dm_getObj` trataba `w3` como registro 11 (`wN`/`xN` son el mismo registro).
Tras la corrección, la tabla de call-sites se puebla correctamente y
`getBackNRestoreK` aparece como `Object=0, Instance=2, Size=0x6e8`, lo que
**corrobora que `0x54cd0` es el wrapper de `dm_getObj`** y que el offset 0x51c
es correcto.

### 6.4 Conclusiones de la RE
- Restore es **solo configuración**: no ejecuta comandos ni escribe archivos
  arbitrarios (`dm_restoreCfg` → `dm_saveCfg`).
- Rootfs es **solo lectura**; `misc_rw` (UBI) es la única zona escribible y no
  tiene hook de init → **backup/restore NO da persistencia de boot**.
- El firmware contiene `telnetd`/`dropbear` con handlers `DEV2_TELNET_CFG` /
  `DEV2_SSH_CFG` → un backup modificado **podría habilitar shell en runtime**
  (hipótesis no probada en hardware).
- `INCLUDE_DIGITAL_SIGNATURE=0` → restore no exige firma.

### 6.5 Bloqueo de la RE
- Búsqueda brute-force **no-password** y **empty-password** del espacio 2³²:
  **0 aciertos** → el backup de muestra usa **contraseña no vacía**.
- Sin la contraseña ni un volcado vivo de `DeviceInfo[0x51c]`, no se puede
  descifrar la muestra ni mapear el campo a un nombre.

---

## 7. Estado por hitos (AGENTS.md)

| Hito | Estado | Evidencia |
|------|--------|-----------|
| M0 Hardware Discovery | Parcial | Firmware extraído (`_fw_extract`, `_rootfs`); SoC MT7981; `libcmm.so` presente |
| M1 Shell Access | **No alcanzado** | Sin credenciales SSH/telnet ni UART; `router_recon.py` listo para recon solo-lectura |
| M2 Wi-Fi Observation | **Alcanzado (vía API)** | `ex520-network-map-gdpr.md`: mapa completo con RSSI sin shell |
| M3 Detectic Sensor | **Alcanzado** | `src/gtpr.rs` + `src/main.rs`; compila y testea |
| M4 Local Aggregation | **Alcanzado** | `store.rs::device_aggregates`, comando `report` |
| M5 Secure Transport | **Alcanzado** | HMAC auth, retry/backoff, buffer offline acotado en `main.rs` |
| M6 Backend | **Alcanzado** | `backend/server.py` ingesta + SQLite |
| M7 Presence Analytics | Parcial | Agregados por dispositivo; falta patrones hora-día/recurrencia |
| M8 Multi-sensor | Parcial | Backend soporta `sensor_id`; no probado con varios sensores |
| M9 Intelligence | No iniciado | — |

**Nota clave:** AGENTS.md asume que la observación Wi-Fi exige shell en el
router. El descubrimiento de la API GDPR (**M2 sin shell**) cambia el modelo de
despliegue: el sensor puede operar con solo las credenciales web, sin
modificar firmware.

---

## 8. Descubrimientos clave

1. **La API GDPR da el mapa de red completo sin shell** (RSSI, MAC, IP,
   hostname, estándar, OneMesh). Esto satisface el objetivo central de
   "Detectic como sensor" por la vía ligera prevista en AGENTS.md §2.
2. **El protocolo GTPR es AES-128-CBC + RSA-"sign"** (`h=md5(user+pass)&s=seq+len`,
   key/iv = `<ms><3 rand>`). El cliente Rust lo implementa fielmente y pasa tests.
3. **`backupcfg.bin` es DES-ECB/zlib/MD5** con clave = constante XOR
   `"%08x"(DeviceInfo[0x51c])` ± MD5(pass). Derivación probada por disassembly.
4. **El backup de muestra está cifrado con contraseña no vacía** (brute-force
   no/empty-password = 0 aciertos).
5. **Restore no es vector de despliegue/persistencia** de código; solo config.
6. **`0x54cd0` es el wrapper de `dm_getObj`** (corroborado esta sesión).
7. El sensor Rust **nunca transmite identificadores crudos** (solo seudónimo).

---

## 9. Bugs e inconsistencias encontradas

| # | Archivo | Problema | Gravedad | Acción |
|---|---------|----------|----------|--------|
| 1 | `investigations/backupcfg/reversing/analyze_devinfo.py` | Registro `wN`/`xN` contabilizado como distinto → call-sites salían `?` | Alta (datos falsos) | **Corregido esta sesión** |
| 2 | `python/detectic_client.py` | Defaults inseguros `--password "<REDACTED>"` y `--secret` hardcoded | Media | Eliminar defaults; exigir como el Rust |
| 3 | `python/detectic_client.py` | `network_map()` solo 1 OID, sin merge DHCP/host | Baja | Replicar el merge de 3 OIDs del Rust |
| 4 | `backend/server.py` | Persiste `hostname`/`ip` en claro | Media (privacidad) | No almacenar identificadores crudos en backend |
| 5 | `backend/server.py` / `sensors.json` | Secrets de dev por defecto | Baja (dev) | Documentar cambio obligatorio en prod |
| 6 | `src/store.rs` | Guarda MAC/hostname/ip crudos localmente | Baja (local) | Aceptable para agregación local; revisar si el store vive en backend |

---

## 10. Bloqueos actuales

1. **Backup cfg:** contraseña de backup desconocida + valor `DeviceInfo[0x51c]`
   desconocido → no se puede descifrar la muestra ni habilitar telnet/ssh por
   esta vía.
2. **Shell/hardware:** sin acceso al router (UART/SSH) no se puede (a) confirmar
   que restore activa `dropbear`/`telnetd`, ni (b) volcar el valor 0x51c en vivo.
3. **M7/M8:** falta analytics de presencia (hora-día, recurrencia) y prueba
   multi-sensor.

---

## 11. Recomendaciones / próximos pasos

1. **Priorizar la vía API GDPR** como despliegue real del sensor (ya funciona
   sin shell). Cross-compilar el binario Rust a `aarch64-unknown-linux-musl` y
   ejecutarlo desde `misc_rw` vía la shell que la API misma podría habilitar.
2. **Limpiar `python/detectic_client.py`**: quitar defaults inseguros y
   alinear `network_map` con el merge de 3 OIDs del Rust.
3. **Endurecer privacidad del backend**: no persistir hostname/IP; confiar
   siempre en el seudónimo del sensor.
4. **Cerrar la RE de backupcfg**: obtener la contraseña de backup o un volcado
   vivo de `DeviceInfo[0x51c]` (vía la shell habilitada por la vía API) para
   descifrar la muestra y validar la hipótesis telnet/ssh.
5. **M7:** añadir detección de patrones (hora-día, días, recurrencia) sobre los
   agregados ya existentes en `store.rs`/`server.py`.
6. **M8:** probar con 2+ sensores y añadir correlación/cruce en el backend.

---

## 12. Verificación realizada en esta sesión

- `python -m py_compile` de todos los `.py`: OK.
- `cargo test --no-default-features`: 14/14 OK.
- `cargo build` (feature `persist`, SQLite bundled): compila.
- `cargo test` (completo): 15/15 OK (incluye `store::aggregates_two_snapshots`).
- Corrección de `analyze_devinfo.py` y regeneración de `DeviceInfo_REVERSE.md`
  con la tabla de call-sites poblada y corroboración del wrapper `dm_getObj`.
