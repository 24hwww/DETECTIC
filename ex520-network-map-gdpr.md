# TP-Link EX520 Network Map Access via GTPR API (Without SSH)

## Summary
Successfully retrieved the complete network map from a TP-Link EX520 router (model EX520V124101568249) using only the GTPR (GDPR) HTTP API — no SSH access required.

## Key Finding: Operations Must Be Encrypted
The critical breakthrough: **ALL `go` and `gl` operations must be sent encrypted** via AES-128-CBC with RSA signing. Sending the JSON body in clear text produces `[error]71111` or empty responses.

### Encryption Format (matching `tpAjax` / `$.tpAjax`):
```
raw = JSON.stringify({data:{stack:"0,0,0,0,0,0",pstack:"0,0,0,0,0,0",...}, operation:"go"|"gl", oid:"DEV2_..."}) + "\r\n"

data = AES-128-CBC-encrypt(raw, session_key, session_iv) → base64 string
sign = RSA-signature over "h=<md5(user+pass)>&s=<seq+data.length>"

body = "sign=" + sign + "\r\ndata=" + data + "\r\n"
```

- **Session key/iv**: Generated during login (`POST /cgi_gdpr?9`)
- **seq**: The `seq` value from `GET /cgi/getGDPRParm` — does NOT increment per request
- **md5**: `MD5("user" + "<REDACTED>")` = `CBBCE1B61B0896CE5B2B51CAE878D9F763F763B8530BEAB701C8424EED439D7FA89D52515F3F472E7C69A2AF3C02DDC7B845D9FAC32A55E7A6DA277B43C22745`
- **Headers required**: `TokenID: <token from GET />`, `Cookie: JSESSIONID=...`, `Referer: http://192.168.0.1/`, `Content-Type: text/plain`

### Responses
- Response body is **base64 text split across HTTP chunks**
- Must concatenate all chunk texts as base64 string, then AES-decrypt the combined base64
- Do NOT concatenate binary chunks — causes "bad decrypt" errors

## Successfully Read OIDs

| OID | Method | Result |
|-----|--------|--------|
| `DEV2_WIFI_APDEV_ASSOCDEV` | `gl` | **Full map**: All connected devices with hostname, IP, MAC (radio + AP), RSSI (signalStrength 0-128), noise floor, operating standard (ax/n/ac), link rates, association time, and **OneMesh stack** (`1,1,2,N,0,0` = radio→AP→slot) |
| `DEV2_DHCPV4_CLIENT` | `gl` | DHCP leases: IP, MAC, hostname, lease time |
| `DEV2_HOST_ENTRY` | `gl` | Host table: MAC, IP, hostname |
| `DEV2_HOSTS` | `go` | Count only: `hostNumberOfEntries: "6"` |

### Sample: DEV2_WIFI_APDEV_ASSOCDEV (6 devices)
- **A35-de-Daniel**: IP 192.168.0.20, MAC 16:A2:EC:D2:15:69, RSSI 118, ax, rates 229/145 Mbps
- **moto-g42**: IP 192.168.0.20, MAC A6:9D:50:62:05:E6, RSSI 118, n, rates 96/86 Mbps
- **Coopera-Telecom-MotoG**: IP 192.168.0.26, MAC 32:D0:61:45:D9:A0, RSSI 74, n, rates 1/1 Mbps
- **moto-g54-5G**: IP 192.168.0.21, MAC D6:8A:2B:93:62:7A, RSSI 100, n, rates 86/39 Mbps
- 2 additional devices (one on 2.4GHz RadioMac ...:C1, one on 5GHz ...:C3)
- **OneMesh stack examples**: `1,1,2,1,0,0`, `1,1,2,2,0,0`, `1,1,2,3,0,0`, `1,1,2,4,0,0` — maps radio index → AP index → slot

### Failed/Unsupported OIDs
- `DEV2_X_TP_ONEMESH_TOPO` → errorcode 9804 ("Case not found")
- `DEV2_X_TP_ONEMESH_DEVICE` → errorcode 9804
- `DEV2_HOSTS` via `gl` → errorcode 9003
- `DEV2_WIFI_APDEV_ASSOCDEV` via `go` (unencrypted) → `[error]71111`

## Authentication Flow (Working)
1. `POST /cgi/getGDPRParm` → Returns `nn` (RSA modulus), `ee` (public exponent `010001`), `seq`
2. `POST /cgi_gdpr?9` with login body: `sign=RSA(h=md5&s=seq+len)\r\ndata=AES-base64(username+\npassword)\r\n` → Gets `JSESSIONID` + session key/iv
3. `GET /` (with cookie) → HTML containing `var token="<32hex>"` (TokenID)
4. All subsequent `go`/`gl` operations: encrypt body as above + `TokenID` + `Cookie` headers

## Files Referenced (from investigation)
- `/tmp/ex520/ex520-map.mjs` — complete working script (login + encrypted `gl` calls + JSON parsing)
- `/tmp/ex520/probe-enc.mjs` — confirmed encryption is mandatory
- `/tmp/ex520_gdprProxy.js` — `tpAjax` (lines 108-168) enforces encryption for ALL `/cgi_gdpr` POSTs
- `/tmp/ex520_tpEncrypt.js` — `AESEncrypt`, `getSignature` (isLogin=0 → `h=md5&s=seq+len`)
- `/tmp/ex520_oid_str.js` — OIDs: `DEV2_HOST_ENTRY`, `DEV2_DHCPV4_CLIENT`, `DEV2_WIFI_APDEV_ASSOCDEV`, `DEV2_HOSTS`
- `waffelheld/TP-Link-gdpr-api` (Python) — reference implementation of the same protocol

## Conclusion
The EX520 firmware's GTPR API requires encrypted bodies for network operations. Once the encryption pattern is understood (AES-CBC + RSA `h=md5&s=seq+len`), the complete network map is accessible: all WiFi clients with signal strength, IPs, MACs, device names, and OneMesh topology. No SSH or admin password needed beyond the web login credentials (`user`/`<REDACTED>` in this case).


Resumen: la clave era cifrar los bodies go/gl con AES-128-CBC + RSA h=md5&s=seq+len (igual que el login). Sin cifrado → [error]71111. Con cifrado → mapa completo: clientes WiFi con RSSI, IPs, MACs, sistema OneMesh (stack jerárquico), DHCP leases, y hasta 6 dispositivos visibles. Ya no se necesita SSH.
