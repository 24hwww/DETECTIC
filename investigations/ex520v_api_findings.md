# Hallazgos de la API GTPR/GDPR en vivo — TP-Link EX520V

> Obtenido mediante Chrome DevTools MCP contra `http://192.168.0.1/`.
> **Advertencia de privacidad:** las capturas brutas contienen MACs, IPs, nombres de host y contraseñas en texto plano. No deben commitearse.

---

## 1. Contexto del firmware observado

- **Modelo:** EX520
- **Hardware:** `EX520 v1.0`
- **Firmware detectado en la UI:** `0.1.0 3.0.0 v60b4.0 Build 241015 Rel.68249n`
- **Idioma de la UI en el router:** `pt_BR`
- **Método de captura:** Chrome DevTools MCP configurado en `.devin/mcp_config.local.json`, login a través del formulario web, extracción de llamadas `$.dm.get`/`$.dm.getList` y del panel Network.
- **Archivos brutos (temporales, fuera del repo):**
  - `/tmp/ex520_api_capture.json` — listas de OIDs (`getList`)
  - `/tmp/ex520_api_scalar.json` — objetos individuales (`get`)
  - `/tmp/ex520_api_capture.pretty.json` — versión formateada

---

## 2. Flujo de autenticación observado

### 2.1 `POST /cgi/getGDPRParm`

No requiere autenticación. Devuelve **JavaScript ejecutable**, no JSON:

```javascript
var adminSetting=1;
var userSetting=1;
var logoUrl="";
var ee="010001";
var nn="C394...0EBB";   // 128 hex = RSA 512-bit
var seq="47294808";      // string decimal
$.ret=0;
```

- `nn`: módulo RSA en hexadecimal, **128 caracteres = 512 bits** (no 1024).
- `ee`: exponente público `0x010001`.
- `seq`: número de secuencia devuelto como string.
- `adminSetting`/`userSetting`: varían según el estado de sesión del router.

### 2.2 Login: `POST /cgi_gdpr?9`

- **Headers:** `Content-Type: text/plain`, `Referer: http://192.168.0.1/`, `X-Requested-With: XMLHttpRequest`, `TokenID: <token>`.
- **Cookie:** en la captura, la petición de login ya llevaba `Cookie: JSESSIONID=...`.
- **Body:**
  ```text
  sign=<256 hex>
  data=<AES-128-CBC base64>
  ```
- **Longitud del `sign` de login:** 256 caracteres hex, es decir, **dos bloques de 64 bytes** de RSA. Ello indica chunking para mensajes > 64 bytes.

### 2.3 Respuesta de login

Cifrada con AES. Al descifrarla con la clave de sesión generada por el cliente se obtuvo algo similar a:

```json
{
  "data": { "language": "pt_BR", "stack": "0,0,0,0,0,0" },
  "operation": "go",
  "oid": "DEV2_LOCAL",
  "success": true
}
```

La clave/IV de sesión usada para descifrar provenían del `$.Iencryptor` del navegador, no de la respuesta.

### 2.4 TokenID y JSESSIONID

- Todas las peticiones `POST /cgi_gdpr?9` llevan el header `TokenID`.
- El valor varía entre sesiones/páginas; posiblemente es generado por el cliente o recibido del router en una etapa previa.
- `JSESSIONID` parece gestionarse por cookies/sesión del router.

---

## 3. Cifrado y firma

### 3.1 AES

- **Algoritmo:** AES-128-CBC con padding PKCS#7 (equivalente al PKCS#5 de CryptoJS).
- **Clave e IV:** cadenas ASCII de 16 bytes (timestamps + random). Se usan tanto para login como para las operaciones `go`/`gl`.
- **Codificación:** cifrado → bytes → base64 para el campo `data`.

### 3.2 RSA

- **Módulo:** 512 bits (`nn` de 128 hex).
- **Exponente:** `010001`.
- **Esquema de firma:**
  - Para `gl`/`go` el mensaje es corto (`h=<md5(user+pass)>&s=<seq+len>`) y cabe en un solo bloque de 64 bytes.
  - Para **login** el mensaje incluye la clave/IV: `key=<16>&iv=<16>&h=<md5>&s=<seq+len>`, con una longitud total de ~87 bytes. Esto requiere **dos bloques de 64 bytes** (chunking).
- **Padding en el sign:** de acuerdo con `js/encrypt.js` y `js/tpEncrypt.js`, el router usa `nopadding` (relleno con ceros al final del último chunk), no PKCS#1 v1.5.
- **Salida del sign:** hex (`gdpr-json`) o base64 (`gdpr-text`).

### 3.3 Cuerpo de las peticiones cifradas

```text
sign=<rsa-hex>
data=<aes-base64>
```

La operación desencriptada (`raw`) para `gl` tiene la forma:

```json
{"data":{"stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"},"operation":"gl","oid":"DEV2_..."}
```

Para `login` (`gdpr-json`):

```json
{"data":{"UserName":"<base64>","Passwd":"<base64>","Action":"1","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"},"operation":"cgi","oid":"/cgi/login"}
```

---

## 4. OIDs y mapeo de campos

### 4.1 Dispositivos Wi-Fi asociados

**OID:** `DEV2_WIFI_APDEV_ASSOCDEV`  
**Método:** `getList`

Campos relevantes (valores redactados):

| Campo real | Significado |
|------------|-------------|
| `X_TP_HostName` | Nombre del dispositivo (hostname) |
| `X_TP_IPAddress` | Dirección IPv4 asignada |
| `X_TP_RadioMac` | MAC de la radio del AP (ej. `3C:6A:D2:5F:AB:C1`) |
| `X_TP_ApDeviceMac` | MAC del dispositivo AP |
| `X_TP_BssMac` | BSSID |
| `MACAddress` | MAC del cliente Wi-Fi |
| `operatingStandard` | Estándar Wi-Fi: `n`, `ac`, `ax` |
| `signalStrength` | RSSI (valor numérico, ej. 90-118) |
| `noise` | Nivel de ruido |
| `lastDataDownlinkRate` / `lastDataUplinkRate` | Tasas de enlace down/up |
| `X_TP_MaxLinkRate` | Tasa máxima de enlace |
| `associationTime` | Timestamp ISO 8601 de asociación |
| `active` | `1` activo, `0` inactivo |
| `stack` | Posición OneMesh, ej. `1,1,2,N,0,0` |

El campo `stack` parece codificar: `1,1,2,<dispositivo>,0,0` para 2.4 GHz y `1,2,2,<dispositivo>,0,0` para 5 GHz.

### 4.2 Tabla de hosts (LAN)

**OID:** `DEV2_HOST_ENTRY`  
**Método:** `getList`

| Campo real | Significado |
|------------|-------------|
| `hostName` | Nombre del host |
| `physAddress` | MAC física |
| `IPAddress` | IPv4 |
| `addressSource` | `DHCP`, `Static`, etc. |
| `leaseTimeRemaining` | Tiempo restante de lease |
| `interfaceType` | `Wi-Fi`, `Ethernet`, etc. |
| `X_TP_ClientType` | Tipo de cliente (`Android`, etc.) |
| `X_TP_Layer2Interface` | Interfaz L2 asociada |
| `X_TP_LanConnDev` | Dispositivo de conexión LAN (`br0`) |
| `associatedDevice` | Referencia a `Device.WiFi.AccessPoint.X.AssociatedDevice.Y.` |
| `active` | `1` / `0` |
| `X_TP_IPv6Address` | IPv6 global |
| `X_TP_IPv6LinkLocal` | IPv6 link-local |

### 4.3 Cliente DHCP del router (WAN, no LAN)

**OID:** `DEV2_DHCPV4_CLIENT`  
**Método:** `getList`

En este firmware no devuelve leases LAN. Es el cliente DHCP de la interfaz WAN del propio router:

- `alias`: `cpe-dhcpv4client`
- `interface`: `Device.IP.Interface.3.`
- `X_TP_ConnStatus`: `Disconnected`
- `X_TP_Hostname`: `EX520`
- `IPAddress`: vacío

Esto contradice la suposición inicial de que `DEV2_DHCPV4_CLIENT` contendría los leases de clientes LAN.

### 4.4 Configuración Telnet/SSH

**OID:** `DEV2_TELNET_CFG`  
**Método:** `get`

```json
{
  "telnetLocalEnabled": "0",
  "telnetLocalPort": "23",
  "telnetRemoteEnabled": "0",
  "telnetRemoteHost": "",
  "telnetRemoteAll": "0",
  "telnetRemotePort": "23"
}
```

Telnet está deshabilitado.

**OID:** `DEV2_SSH_CFG`  
**Método:** `get`

Devuelve error `9804` (no soportado). No existe configuración SSH accesible por GTPR.

### 4.5 Usuarios y credenciales

**OID:** `DEV2_USER_CFG`  
**Método:** `get`

Devuelve los usuarios admin/user y las **contraseñas en texto plano** (sin ofuscar). Ejemplo de campos:

- `adminName`, `adminPwd`, `adminPwdBackup`
- `userName`, `userPwd`, `userPwdBackup`
- `adminEnable`, `userEnable`, `rootEnable`

Este hallazgo implica que cualquier sesión web administrativa puede leer la contraseña del otro usuario a través de la API.

### 4.6 Configuración HTTP/HTTPS y acceso remoto

**OID:** `DEV2_HTTP_CFG`  
**Método:** `get`

- `httpLocalEnabled`: `1`, puerto `80`
- `httpsLocalEnabled`: `1`, puerto `443`
- `httpRemoteEnabled`: `1`
- `httpsRemoteEnabled`: `1`
- `remoteHost`: IP/máscara permitida para gestión remota (redactada).

### 4.7 OIDs no soportados

| OID | Error | Nota |
|-----|-------|------|
| `DEV2_HOSTS` | 9003 | No devuelve lista por `getList` |
| `DEV2_SSH_CFG` | 9804 | No soportado |
| `DEV2_LOGIN_CFG` | 9804 | No soportado |
| `DeviceInfo` | 9804 | No soportado |
| `DEV2_DEVICE_INFO` | 9804 | No soportado |

---

## 5. Diferencias con la documentación previa

La documentación anterior (`ex520-network-map-gdpr.md`) asumía varias cosas que la captura en vivo corrige:

| Aspecto | Documentación anterior | Observación en vivo |
|---------|------------------------|---------------------|
| RSA | 1024-bit (256 hex) | **512-bit** (128 hex) |
| Sign de login | `h=md5&s=seq+len` | Incluye `key` e `iv`; requiere **chunking** |
| Padding RSA | PKCS#1 v1.5 asumido | `nopadding` con ceros finales |
| `DEV2_DHCPV4_CLIENT` | Leases LAN | Cliente WAN del router |
| Campos del mapa | `hostname`, `opStandard`, `IPAddress`, `MACAddress` | `X_TP_HostName`, `X_TP_IPAddress`, `operatingStandard`, `MACAddress`, `X_TP_RadioMac` |

---

## 6. Hallazgos de seguridad y privacidad

1. **Contraseñas en claro en la API:** `DEV2_USER_CFG` devuelve las credenciales sin ofuscar.
2. **Gestión remota activa:** HTTP/HTTPS remotos habilitados con IP restringida.
3. **Telnet/SSH deshabilitados:** no hay shell remota fácil vía configuración web.
4. **Datos personales visibles:** hostname, IP, MAC, RSSI y tipo de cliente de todos los dispositivos conectados.
5. **No requiere SSH:** el mapa de red completo es accesible con solo las credenciales del panel web.

---

## 7. Implicaciones para el cliente Detectic

Para que `detectic_client.py` y el cliente Rust funcionen contra este router real, el cliente debe:

1. Parsear el JavaScript de `getGDPRParm` (ya implementado parcialmente).
2. Soportar **RSA 512-bit** con chunking de 64 bytes y `nopadding`.
3. Incluir `key` e `iv` en el sign de login y fragmentarlo en dos bloques.
4. Enviar header `TokenID` en todas las peticiones `cgi_gdpr`.
5. Manejar cookies (`JSESSIONID`).
6. Mapear los campos reales `X_TP_*` y `operatingStandard` en el parser del mapa de red.
7. No confiar en `DEV2_DHCPV4_CLIENT` para leases LAN; usar `DEV2_HOST_ENTRY` y `DEV2_WIFI_APDEV_ASSOCDEV`.

---

## 8. Próximos pasos documentados

- Actualizar los parsers y firmas del cliente Python y Rust según el cifrado real observado.
- Decidir estrategia de pseudonimización de MACs antes de persistir datos en backend.
- Evaluar si la gestión remota habilitada representa riesgo y si se requiere hardening.
