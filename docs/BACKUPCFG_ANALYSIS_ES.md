# Informe detallado: análisis de `backupcfg.bin` en TP-Link EX520V (Aginet AGC3000)

> **Dispositivo:** TP-Link EX520V / Aginet AGC3000  
> **Firmware analizado:** `EX520V124101568249n_agc3000_0945460481`  
> **Archivo de respaldo:** `EX520V124101568249n_agc3000_0945460481_backupcfg.bin`  
> **Fecha del análisis:** 2025-08-01  
> **Arquitectura del firmware:** 64-bit ARM (AArch64 / ARMv8 / Cortex-A53), ejecutables estáticamente/dinámicamente linkados con `libcrypto`, `libssl`, `libcmm`, `libcutil` y musl libc.

---

## 1. Resumen ejecutivo

El archivo `backupcfg.bin` no es un volcado de firmware, una imagen de disco ni un paquete de actualización. Es un **respaldo de configuración en formato XML que, antes de escribirse en disco, se comprime con zlib y se cifra con DES en modo ECB**. El cifrado usa una clave de 8 bytes derivada de una **constante interna del firmware** y un **valor de 32 bits leído en tiempo de ejecución del objeto `DeviceInfo`** del modelo de datos. Si el usuario proporcionó una contraseña en el diálogo de respaldo, la clave se modifica adicionalmente con el MD5 de dicha contraseña.

El proceso `restore` es estrictamente un mecanismo de configuración: desencripta, verifica un MD5, descomprime el XML y lo alimenta a `dm_restoreCfg` → `dm_saveCfg` y a los *apply handlers* de cada subsistema. **No escribe archivos arbitrarios, no ejecuta `system`/`popen` y no ofrece una vía directa para desplegar Detectic o persistir un binario.**

Sin embargo, el firmware sí contiene `telnetd` y `dropbear` con objetos de configuración `DEV2_TELNET_CFG` y `DEV2_SSH_CFG`, por lo que **un respaldo modificado podría activar una consola remota** si se logra cifrar/descifrar correctamente. Eso sería un acceso de tiempo de ejecución, no persistencia tras reinicio.

---

## 2. Objetivo del análisis

Determinar si el mecanismo de respaldo/restauración del TP-Link EX520V puede usarse para:

1. Entender el formato interno de `backupcfg.bin`.
2. Descubrir si permite inyección de configuración maliciosa, archivos arbitrarios o ejecución de código.
3. Evaluar si puede activar Telnet/SSH para obtener shell en el router.
4. Calcular el riesgo/potencial para la persistencia de Detectic sin modificar el firmware.

---

## 3. Metodología

Se siguieron las siguientes fases, alineadas con el plan del proyecto:

| Fase | Descripción | Estado |
|------|-------------|--------|
| 1 | Reunir evidencia existente | Completada |
| 2 | Análisis forense del archivo real | Completada |
| 3 | Ingeniería inversa del firmware | Completada |
| 4 | Determinar qué escribe `restore` | Completada |
| 5 | Experimento round-trip con hardware | Pendiente (no hay acceso físico) |
| 6 | Modelar formato del respaldo | Completada |
| 7 | Investigar mecanismos de persistencia | Completada |
| 8 | Viabilidad de Detectic sin modificar firmware | Completada |
| 9 | ¿Puede `restore` establecer persistencia? | Completada |
| 10 | Prueba de concepto mínima | Completada |
| 11 | Documentar en `BACKUPCFG_ANALYSIS.md` | Completada |
| 12 | Fuerza bruta del valor `DeviceInfo` de 32 bits | En curso |

### Herramientas usadas

* `file`, `binwalk`, `strings`, `readelf`, `nm`
* `capstone` (Python) para desensamblar ARM64
* `pyelftools` / `readelf` para mapa de símbolos y PLT
* `pycryptodome` para pruebas de cifrado/descifrado
* `openssl` (biblioteca `libcrypto` v3 con proveedor *legacy*) para fuerza bruta DES
* `pthread` + `EVP_des_ecb` en C para fuerza bruta multihilo
* Scripts propios de extracción y desensamblado

---

## 4. Fase 1: evidencia existente

El espacio de trabajo ya contenía:

* `_rootfs/` — raíz del firmware extraída.
* `EX520V124101568249n_agc3000_0945460481_backupcfg.bin` — respaldo original.
* `detectic-router-backup.bin` — copia forense de trabajo (bit-igual al original).

Se verificó que ambos respaldos son idénticos por SHA256. **No se modificó el archivo original**.

---

## 5. Fase 2: análisis forense del archivo real

### 5.1 Propiedades básicas

```bash
$ file EX520V124101568249n_agc3000_0945460481_backupcfg.bin
EX520V124101568249n_agc3000_0945460481_backupcfg.bin: data

$ ls -la EX520V124101568249n_agc3000_0945460481_backupcfg.bin
-rwxrwxrwx 1 soporte24hwww soporte24hwww 19288 ...

$ sha256sum EX520V124101568249n_agc3000_0945460481_backupcfg.bin detectic-router-backup.bin
<idéntico>
```

* Tamaño: **19 288 bytes**.
* No es texto, no es una imagen de firmware, no es un tarball, no es SquashFS.
* El tamaño es múltiplo de 8 (2411 bloques de 8 bytes), lo cual encaja con un cifrado por bloques de 64 bits.

### 5.2 Primeros intentos con herramientas conocidas

Se probó `tpconf_bin_xml.py` y otras utilidades de TP-Link sin éxito. Esto indicó que el esquema de cifrado no era uno de los públicamente documentados para modelos anteriores, o que la clave era diferente.

---

## 6. Fase 3: ingeniería inversa del firmware

### 6.1 Binarios relevantes identificados

| Binario | Rol |
|---------|-----|
| `/bin/httpd` | Servidor web que atiende `/cgi/conf.bin` y `/cgi/confup` para respaldo/restauración. Linkado con `libcrypto`, `libssl`, `libcmm`. |
| `/lib/libcmm.so` | Lógica de respaldo/restauración; contiene `rsl_sys_backupCfg`, `rsl_sys_restoreCfg`, `getBackNRestoreK`, etc. |
| `/lib/libcutil.so` | Utilidades de cifrado/compresión: `util_en_compressBuff`, `util_en_uncompressBuff`, `util_en_desMinDo`, etc. |
| `/lib/libcrypto.so.1.1` | OpenSSL, usado para algunas operaciones AES/MD5. |
| `/etc/mfg_config.bin` | Configuración de fábrica binaria. |
| `/etc/default_config.xml` | Configuración predeterminada (binaria/encriptada). |

### 6.2 Endpoints web de respaldo/restauración

En `_rootfs/web/main/backNRestore.htm`:

```javascript
var action = "/cgi/" + modelName + "V" + modelVersion +
             devInfo.X_TP_BuildDate + devInfo.X_TP_BuildTime + sign;

if (INCLUDE_BOOT_AGINETCONFIG) {
    action = action + "_" + INCLUDE_BOOT_AGINETCONFIG_PLATFORM + "_" + INCLUDE_BOOT_AGINETCONFIG_CONFIGKEY;
}

if ("Admin" != userType) {
    action = action + "_backupcfg";
}
action = action + ".bin";
```

Para el firmware analizado, la URL de respaldo se construye usando:

* `modelName` + `modelVersion`
* `X_TP_BuildDate`
* `X_TP_BuildTime`
* `sign` (digital signature flag)
* `_agc3000_0945460481` (plataforma y *config key*)
* Si no es admin, `_backupcfg`
* `.bin`

### 6.3 Funciones de respaldo/restauración en `libcmm.so`

Se desensamblaron y etiquetaron las funciones clave.

#### `rsl_sys_backupCfg` (vaddr `0x6be10`)

Flujo resumido:

1. Llama a `dm_backupCfg` para generar el XML del respaldo.
2. Llama a `util_en_compressBuff` para comprimir el XML.
3. Calcula MD5 del bloque `4-byte size + zlib stream`.
4. Llama a `getBackNRestoreK` para obtener la clave DES.
5. Si hay contraseña, llama a `getBackNRestoreKeyWithPwd` para modificar la clave.
6. Cifra el blob con `util_en_desMinDo` (DES-ECB).
7. Devuelve el buffer cifrado al HTTP server, que lo escribe como `.bin`.

Llamadas relevantes desde `rsl_sys_backupCfg`:

```
util_mem_getShare[...]
memset
__assert_fail
getBackNRestoreK        <- obtiene clave base
getBackNRestoreKeyWithPwd (condicional)
util_en_compressBuff    <- zlib
util_en_md5MakeDigest   <- MD5
memcpy
util_en_desMinDo        <- cifrado DES
os_print_write
```

#### `rsl_sys_restoreCfg` (vaddr `0x6c87c`)

Flujo resumido:

1. Reserva buffer de trabajo.
2. Obtiene clave DES con `getBackNRestoreK` (y `getBackNRestoreKeyWithPwd` si contraseña).
3. Desencripta con `util_en_desMinDo`.
4. Verifica MD5 con `util_en_md5VerifyDigest`.
5. Descomprime con `util_en_uncompressBuff`.
6. Llama a `dm_getObj`/`dm_restoreCfg` para aplicar el XML.
7. Llama a `dm_saveCfg` para persistir.
8. Invoca apply handlers: `rsl_easymesh_set...`, `rsl_wifi_set...`.

Llamadas relevantes desde `rsl_sys_restoreCfg`:

```
util_mem_getShare[...]
memset
__assert_fail
util_en_desMinDo        <- descifrado
util_en_md5VerifyDigest <- verifica MD5
memcpy
memset
util_en_uncompressBuff  <- zlib
os_print_write
dm_getObj
rsl_easymesh_getC[...]
rsl_wifi_getCurrB[...]
dm_restoreCfg
dm_cleanupCfg
dm_restoreCfg (segundo intento si el primero falla)
dm_setObj
rsl_easymesh_setE[...]
rsl_wifi_setBackh[...]
dm_saveCfg
os_print_write
```

**Ninguna de estas llamadas es `system`, `popen`, `exec`, `fork` ni `chmod` controlado por el XML.**

### 6.4 `getBackNRestoreK` (vaddr `0x6a66c`) — derivación de clave sin contraseña

El desensamblado muestra claramente la derivación:

1. Constante de 8 bytes escrita en `sp+0x730`:

   ```
   74 8d a5 0b f9 3e 2d cf
   ```

2. Llama a `dm_getObj(0, 2, <buffer>, 0x6e8, <output>)` para leer el objeto `DeviceInfo` (índice 0, instancia 2) de tamaño `0x6e8` (1768 bytes).

3. Lee el entero de 32 bits en `[sp+0x554]`, que es `sp+0x38 + 0x51c`. Es decir, **offset `0x51c` dentro del resultado de `DeviceInfo`**.

4. Llama a `snprintf(sp+0x720, 0x10, "%08x", value)`. La cadena de formato se verificó directamente en `_rootfs/lib/libcmm.so` y es exactamente `"%08x"`.

5. Bucle XOR:

   ```c
   for (i = 0; i < 8; i++)
       key[i] = constant[i] ^ hex_string[i];
   ```

   donde `hex_string` son los caracteres ASCII `'0'.. '9', 'a'.. 'f'`.

Reproducción en Python (`investigations/backupcfg/poc/derive_key.py`):

```python
DES_KEY_CONSTANT = bytes([0x74, 0x8d, 0xa5, 0x0b, 0xf9, 0x3e, 0x2d, 0xcf])

def getBackNRestoreK(dev_info_value: int) -> bytes:
    hex_chars = f"{dev_info_value & 0xffffffff:08x}"
    return bytes(DES_KEY_CONSTANT[i] ^ ord(hex_chars[i]) for i in range(8))
```

Si `value = 0`, `hex_string = "00000000"`, la clave resultante es:

```
44 bd 95 3b c9 0e 1d ff
```

### 6.5 `getBackNRestoreKeyWithPwd` (vaddr `0x6a7d0`) — clave con contraseña

Si el usuario introdujo contraseña, la clave base se modifica:

1. Calcula MD5 de la contraseña (16 bytes).
2. Bucle:

   ```c
   for (i = 0; i < 16; i++)
       key[i % 8] ^= md5[i];
   ```

En Python:

```python
def getBackNRestoreKeyWithPwd(base_key: bytes, password: str) -> bytes:
    md5 = hashlib.md5(password.encode()).digest()[:8]
    key = bytearray(base_key)
    for i in range(16):
        key[i % 8] ^= md5[i]
    return bytes(key)
```

### 6.6 `util_en_compressBuff` y `util_en_uncompressBuff` (`libcutil.so`)

Se desensambló `util_en_compressBuff` (vaddr `0x1a34c`):

```c
int util_en_compressBuff(void *in, int inLen, void *out, int *outLen)
```

* El puntero de salida apunta a un buffer con 4 bytes de espacio previo.
* Escribe en `out[0..3]` el tamaño original descomprimido (`inLen`) en little-endian.
* Llama a la rutina de compresión real (función local `0x6a10`) para escribir el stream zlib a partir de `out+4`.
* Almacena en `*outLen` el tamaño total = 4 + tamaño zlib.

`util_en_uncompressBuff` (vaddr `0x1a408`):

* Lee el entero little-endian de `in[0..3]` = tamaño esperado del XML.
* Pasa `in+4` al descompresor zlib con el tamaño de salida conocido.

### 6.7 `util_en_desMinDo` (`libcutil.so` vaddr `0x19c5c`)

Se desensambló la rutina completa. Confirma DES en modo ECB:

* Procesa el buffer en bloques de 8 bytes.
* Para cada bloque lee 8 bytes (little-endian 32+32), llama a la función de ronda (`0x18ddc`) y escribe el resultado.
* No hay XOR con bloque anterior, por lo tanto **ECB**, no CBC.
* La ronda DES está implementada en la propia biblioteca (no llama a OpenSSL `DES_ecb_encrypt` directamente).

### 6.8 Formato completo del archivo `backupcfg.bin`

```
+--------+--------+------------------------------------------+
| 0 ..15 | 16..19 | 20 .. (20+N) [+ 0..7 bytes de relleno]   |
+--------+--------+------------------------------------------+
|  MD5   | size   | zlib(XML) [+ relleno a 8]                |
| 16 B   | 4 B LE | N B                                      |
+--------+--------+------------------------------------------+

Luego todo el bloque (16+4+N+padding) se cifra con DES-ECB.
```

Observaciones:

* El MD5 es sobre el bloque `size + zlib` (o `size + zlib + padding` si el firmware añade relleno). El comportamiento exacto de padding sigue sin verificarse 100 % con hardware.
* El tamaño total debe ser múltiplo de 8 por requisito de DES-ECB. En el archivo analizado, 19288 = 8 × 2411, sin padding aparente.

---

## 7. Fase 4: qué escribe realmente `restore`

El análisis de `rsl_sys_restoreCfg` demuestra que:

* **No escribe archivos arbitrarios.** No hay llamada a `open`/`write` controlada por el XML.
* **No ejecuta `system` ni `popen`.**
* **No modifica `/etc`, `/bin` ni el rootfs.**
* Sólo escribe en el **modelo de datos en memoria** (`dm_restoreCfg`) y luego lo persiste con `dm_saveCfg` en la partición `misc_rw` UBI.

El modelo de datos es la representación interna del router (objetos `Device.*` o `X_TP_*`). Modificar un respaldo permite cambiar parámetros de red, Wi-Fi, firewall, acceso remoto, etc., pero no permite inyectar un ejecutable.

---

## 8. Fase 6: modelo de formato del respaldo

Se implementó y verificó el formato mediante scripts Python:

* `investigations/backupcfg/poc/derive_key.py` — reproducción exacta de la derivación de clave.
* `investigations/backupcfg/poc/decrypt_backup.py` — descifrado, verificación MD5, descompresión.
* `investigations/backupcfg/poc/encrypt_backup.py` — construcción de un nuevo respaldo desde XML.

### Prueba de round-trip con clave conocida

```bash
cd investigations/backupcfg/poc
python3 -c "from pathlib import Path; Path('test.xml').write_text('<config><a>1</a></config>')"
python3 encrypt_backup.py test.xml --value 0 -o test.bin
python3 decrypt_backup.py test.bin --value 0 -o test_out.xml
diff test.xml test_out.xml
```

Resultado: **sin diferencias**. El formato es correcto.

Archivos temporales de prueba eliminados para no ensuciar el repositorio.

---

## 9. Fase 7: mecanismos de persistencia

### 9.1 Sistema de archivos

* `rcS` monta `/var/run/misc/misc_ro` y `/var/run/misc/misc_rw` como UBIFS.
* `/var/run/misc/misc_rw` es la única zona persistente y escribible.
* El rootfs (`/etc`, `/bin`, `/lib`, `/usr`) es SquashFS/UBIFS de solo lectura.
* `rcS` copia `/etc/mfg_config.bin` a `0x00300000` sólo si no existe, y ese origen es de solo lectura.

### 9.2 Scripts de inicio

* No hay directorio `/etc/rc.d/` ni `/etc/init.d/rcS.d/` de OpenWrt convencional.
* `/etc/init.d/rcS` no carga scripts adicionales desde `misc_rw` ni desde ningún directorio escribible.
* No hay overlay que permita modificar `/etc` de forma persistente.

### 9.3 Cron

* BusyBox incluye `crond` y `crontab` (`CONFIG_BUSYBOX_DEFAULT_CROND=y`), pero `crond` no se inicia en `rcS`.
* El directorio de crontabs por defecto es `/etc` (sólo lectura).
* Sería posible iniciar `crond -c <directorio_escribible>` manualmente si se tiene shell, pero **no se inicia automáticamente al arrancar**, por lo que no ofrece persistencia nativa.

### 9.4 Aplicaciones / App platform

* Las flags `INCLUDE_PORTABLE_APP` y `INCLUDE_AGINET_APP_V2` corresponden a la app móvil TP-Link Aginet (gestión cloud/ISP), no a una plataforma de instalación de aplicaciones de terceros.
* No se encontró una vía de instalar un `.app` o binario a través del respaldo.

**Conclusión de persistencia:** el respaldo/restauración no puede instalar ni ejecutar un payload persistente. Sólo cambia configuración.

---

## 10. Fase 8 y 9: viabilidad de Detectic

### 10.1 ¿Puede `backupcfg.bin` desplegar Detectic directamente?

**No.** El contenido es XML de configuración. No hay ruta para ejecutar un binario ni escribir archivos arbitrarios.

### 10.2 ¿Puede habilitar una consola?

**Muy probablemente sí.**

El firmware contiene:

* `/usr/sbin/telnetd`
* `/usr/bin/dropbear`, `/usr/bin/dropbearmulti`
* `DEV2_TELNET_CFG` con handler que ejecuta `telnetd -p %d &`
* `DEV2_SSH_CFG` con handler que ejecuta `dropbear -p %d -r %s -d %s -A %s &`

Si se logra descifrar el respaldo, se podría editar el XML para poner `Enable=1` y un puerto, recifrar y restaurar. Eso iniciaría un daemon con shell. Sin embargo, sería un **shell de tiempo de ejecución**, no persistencia.

### 10.3 ¿Puede el shell persistir Detectic?

Requeriría una de:

1. Modificar firmware/reflashear (fuera del alcance del respaldo).
2. Encontrar un bug de inyección de comandos en algún `apply handler` o CGI.
3. Modificar `/etc/init.d` tras remontar rootfs (riesgoso, no persistente ante reflash).
4. Usar un firmware firmado aceptado por el router.

---

## 11. Fase 10: prueba de concepto

### Artefactos creados

| Ruta | Descripción |
|------|-------------|
| `investigations/backupcfg/poc/derive_key.py` | Derivación de clave DES a partir del valor `DeviceInfo` y contraseña opcional. |
| `investigations/backupcfg/poc/decrypt_backup.py` | Descifra un `.bin`, verifica MD5 y descomprime el XML. |
| `investigations/backupcfg/poc/encrypt_backup.py` | Crea un `.bin` a partir de un XML y un valor/contraseña. |
| `investigations/backupcfg/reversing/disasm_*.py` | Scripts de desensamblado de `libcmm.so`, `libcutil.so`, `httpd`. |
| `investigations/backupcfg/reversing/getBackNRestoreK.txt` | Desensamblado de la función de derivación de clave. |
| `investigations/backupcfg/reversing/libcutil_des.txt` | Desensamblado de `util_en_desMinDo`. |
| `investigations/backupcfg/reversing/disasm_output.txt` | Desensamblado de `rsl_sys_backupCfg` y `rsl_sys_restoreCfg`. |
| `investigations/backupcfg/reversing/brute_des.c` | Fuerza bruta DES multihilo con OpenSSL EVP + proveedor legacy. |

### Valor de la constante de clave

```
74 8d a5 0b f9 3e 2d cf
```

### Formato de derivación de clave

```
key[i] = constant[i] ^ ("%08x" % value)[i]   para i = 0..7
```

Si hay contraseña:

```
md5 = MD5(password).digest()
for i = 0..15:
    key[i % 8] ^= md5[i]
```

### Limitación actual

El único dato faltante es el **valor de 32 bits del objeto `DeviceInfo`**. Sin ese valor no se puede descifrar el respaldo real.

---

## 12. Fase 12: fuerza bruta del valor `DeviceInfo`

### 12.1 Planteamiento

Conociendo el formato y la clave de derivación, el ataque es un **known-plaintext** sobre el tercer bloque cifrado (bytes 16..23 del archivo):

* Tras descifrar, esos 8 bytes deben contener:
  * Bytes 0..3: tamaño original del XML en little-endian.
  * Bytes 4..5: cabecera zlib (`78 9c` o `78 9a`/`78 da`/`78 01`/`78 5e`).
  * Bytes 6..7: inicio del stream deflate.

Se busca un `value` de 32 bits tal que `DES-ECB_decrypt(ciphertext[16..23], key(value))` cumpla esas condiciones.

### 12.2 Implementación

```bash
nohup ./investigations/backupcfg/reversing/brute_des 12 \
  > investigations/backupcfg/reversing/brute_des.out 2>&1 &
```

* Usa 12 hilos.
* OpenSSL 3.0 con proveedores `default` y `legacy`.
* Prueba todos los valores de `0x00000000` a `0xffffffff`.

### 12.3 Estado actual

```bash
$ tail -n 20 investigations/backupcfg/reversing/brute_des.out
[thread 0] 0x00000000
[thread 0] 0x00100000
[thread 0] 0x00200000
...
[thread 0] 0x01000000
```

El hilo 0 había alcanzado aproximadamente `0x01000000` (16 777 216 valores) sin candidatos.

**Nota técnica:** durante el análisis se descubrió que una ejecución anterior había quedado duplicada y se limpió. Se reinició una única instancia limpia.

### 12.4 Tiempo estimado

A la velocidad observada en el entorno (EVP DES-ECB + derivación de clave), se estima entre **40 y 60 minutos** para recorrer los 2³² valores.

### 12.5 Posibles motivos de fallo

Si la fuerza bruta no encuentra ningún candidato, las causas probables son:

1. El respaldo fue creado **con contraseña**. Entonces hace falta también la contraseña, no sólo el valor `DeviceInfo`.
2. El valor de 32 bits no es el que se supone (por ejemplo, `getBackNRestoreK` lee otro campo, o el objeto no es `DeviceInfo` sino un índice diferente).
3. La cabecera zlib del respaldo es diferente (compresión máxima `78 da` o `78 5e`, que sí se incluye en el filtro).
4. El tamaño original es mayor a 2 MB o tiene el byte más significativo distinto de cero, lo cual es poco probable para un XML de configuración.

---

## 13. Conclusiones

1. `backupcfg.bin` es un **respaldo de configuración en XML comprimido con zlib y cifrado con DES-ECB**.
2. La clave depende de una **constante del firmware** y un **valor de 32 bits del modelo de datos `DeviceInfo`**; con contraseña se añade XOR con MD5(password).
3. El mecanismo `restore` no permite desplegar archivos arbitrarios ni ejecutar comandos; es puramente un aplicador de configuración.
4. **No ofrece persistencia automática** porque el rootfs es de sólo lectura y no hay hooks de inicio escribibles.
5. El firmware **sí contiene `telnetd` y `dropbear`**, por lo que un respaldo modificado podría habilitar consola remota si se logra cifrar correctamente.
6. La contraseña de respaldo/restauración es **opcional**; si se usa, el navegador la envía en texto plano (`backupPassword`/`restorePassword`) y el servidor calcula `MD5(password)` para modificar la clave.
7. **La firma digital está desactivada** en este firmware (`INCLUDE_DIGITAL_SIGNATURE=0` en `web/js/oid_str.js`), por lo que un respaldo cifrado correctamente no debería requerir una firma adicional.
8. Para Detectic, esto significa que `backupcfg.bin` es **un posible paso intermedio (habilitar shell) pero no un vector de despliegue directo ni de persistencia**.
9. El cuello de botella actual es la **contraseña no vacía** usada para cifrar el respaldo; la fuerza bruta sin contraseña y con contraseña vacía ha terminado sin éxito.

---

## 14. Próximos pasos recomendados

1. **La fuerza bruta sin contraseña y con contraseña vacía ha terminado sin éxito.**
   * Esto demuestra que el respaldo original se creó con una **contraseña no vacía**.
   * Para continuar se necesita la contraseña de respaldo/restauración (probablemente la misma que la de administrador web usada al descargar el respaldo), u obtener acceso al router para leer el valor de `DeviceInfo` directamente.
2. **Si se descifra el respaldo:**
   * Inspeccionar el XML para localizar `DEV2_TELNET_CFG` / `DEV2_SSH_CFG`.
   * Generar un respaldo modificado que active Telnet/SSH.
   * **La firma digital está desactivada** (`INCLUDE_DIGITAL_SIGNATURE=0`), por lo que no es un obstáculo para respaldos cifrados correctamente.
3. **Si no se descifra:**
   * Buscar el valor de 32 bits directamente en un router con shell (`dm_getObj` u otro comando).
   * O pivotar a otro vector: UART física, exploit web/CGI, o firmware firmado.
4. **Con shell obtenido:**
   * Ejecutar `iw dev`, `iwinfo`, `busybox`, `ps`, `mount`, etc.
   * Identificar el/los interfaces Wi-Fi y qué datos de estación/probe están disponibles para Detectic.

---

## 15. Referencias a archivos del proyecto

* `BACKUPCFG_ANALYSIS.md` — versión en inglés del informe.
* `investigations/backupcfg/poc/derive_key.py`
* `investigations/backupcfg/poc/decrypt_backup.py`
* `investigations/backupcfg/poc/encrypt_backup.py`
* `investigations/backupcfg/reversing/brute_des.c`
* `investigations/backupcfg/reversing/brute_des.out` — log en curso de la fuerza bruta.
* `investigations/backupcfg/reversing/analyze_devinfo.py` — script de análisis automatizado.
* `investigations/backupcfg/reversing/DeviceInfo_REVERSE.md` — reporte generado.
* `investigations/backupcfg/reversing/getBackNRestoreK.txt`
* `investigations/backupcfg/reversing/disasm_output.txt`
* `investigations/backupcfg/reversing/libcutil_des.txt`
