# TP-Link EX520 `backupcfg.bin` Analysis

> Firmware: `EX520V124101568249n_agc3000_0945460481`
> Device: TP-Link EX520V / Aginet AGC3000
> Date: 2025-08-01

## Executive Summary

The `backupcfg.bin` file is not a raw firmware image, a file archive, or a generic OpenWrt `sysupgrade` tarball. It is a **DES-ECB encrypted, zlib-compressed, XML-based device configuration backup**. The encryption key is derived from a **hard-coded constant** and a **32-bit value read at runtime from the `DeviceInfo` data-model object**, optionally XORed with the MD5 of a user-supplied backup password.

`restore` is strictly a configuration operation: it decrypts the file, verifies an MD5 digest, decompresses the XML, and feeds it to the data-model restore path (`dm_restoreCfg` → `dm_saveCfg` → per-subsystem apply handlers). It does **not** allow writing arbitrary files or executing arbitrary commands. The only persistent writable area available to normal configuration is the `misc_rw` UBI partition, and there is **no writable init hook or overlay** that would let a modified backup launch a new process at boot.

**Bottom line for Detectic:** the backup/restore mechanism by itself cannot deploy or persist Detectic on the router without additional shell access or a firmware modification. It can only be used to *change configuration values* (e.g., enable Telnet/SSH if the daemon is present in the image), which is only a first step toward runtime access.

## 1. Backup File Format

A working copy of the original backup was used for all analysis:

```bash
cp EX520V124101568249n_agc3000_0945460481_backupcfg.bin \
   detectic-router-backup.bin
```

The two files are byte-identical (SHA256 already verified in earlier work). No original research artifact was modified.

The on-disk format, derived from reverse engineering `libcmm.so` and `libcutil.so`, is:

| Offset | Length | Content |
|--------|--------|---------|
| 0      | 16     | MD5 digest of the following payload |
| 16     | 4      | Original (uncompressed) XML size, little-endian |
| 20     | N      | zlib-compressed XML |
| 20+N   | 0..7   | Optional zero padding so the whole blob is a multiple of 8 bytes (exact router behaviour not fully verified; see caveats) |

The entire blob is then DES-ECB encrypted.

### 1.1 Evidence from disassembly

* `rsl_sys_backupCfg` (`libcmm.so`, around `0x6be10`):
  * calls `dm_backupCfg` to produce the XML
  * calls `util_en_compressBuff` (`libcutil.so`, `0x1a34c`) to compress it
  * calls `util_en_md5MakeDigest` on the 4-byte size prefix + compressed data
  * calls `util_en_desMinDo` (`libcutil.so`, `0x19c5c`) to DES-ECB encrypt the result

* `rsl_sys_restoreCfg` (`libcmm.so`, around `0x6c87c`):
  * calls `util_en_desMinDo` to decrypt
  * calls `util_en_md5VerifyDigest` on bytes 16..end
  * calls `util_en_uncompressBuff` (`libcutil.so`, `0x1a408`)
  * calls `dm_restoreCfg` and finally `dm_saveCfg`

* `util_en_compressBuff` prefixes the zlib stream with a 4-byte little-endian uncompressed size (`util_en_uncompressBuff` reads it as the expected output size). This was confirmed by disassembling `libcutil.so`.

## 2. Key Derivation

### 2.1 No-password key

The function `getBackNRestoreK` (found in `libcmm.so` around `0x6a66c`) does:

1. Start with a hard-coded 8-byte constant:

   ```
   74 8d a5 0b f9 3e 2d cf
   ```

2. Read a 32-bit unsigned value from `DeviceInfo` object 0, instance 2, at offset `0x51c` via `dm_getObj(0, 2, ..., 0x6e8, ...)`.

3. Format that value as a lowercase 8-hex-char string with `snprintf(buf, 16, "%08x", value)`.

4. XOR each byte of the constant with the corresponding ASCII hex character to produce the 8-byte DES key.

A Python reproduction is in `investigations/backupcfg/poc/derive_key.py`.

```python
DES_KEY_CONSTANT = bytes([0x74, 0x8d, 0xa5, 0x0b, 0xf9, 0x3e, 0x2d, 0xcf])

def getBackNRestoreK(dev_info_value: int) -> bytes:
    hex_chars = f"{dev_info_value & 0xffffffff:08x}"
    return bytes(DES_KEY_CONSTANT[i] ^ ord(hex_chars[i]) for i in range(8))
```

### 2.2 Password-protected key

If a backup password is supplied, `getBackNRestoreKeyWithPwd` computes the MD5 of the password and XORs the first 16 MD5 bytes into the 8-byte key, cycling over the key bytes:

```python
for i in range(16):
    key[i % 8] ^= md5[i]
```

### 2.3 Unknown: the 32-bit DeviceInfo value

The 32-bit value is **not** present in the static firmware. It is read from the runtime data-model (`/var/run/misc/misc_rw/0x00300000` or equivalent). It is located at **offset `0x51c`** inside `DeviceInfo` object 0, instance 2, as proven by the `getBackNRestoreK` disassembly in `libcmm.so`: `dm_getObj` writes the object to an output buffer at `sp+0x38` and the value is loaded from `sp+0x554` (`0x554 - 0x38 = 0x51c`).

It is likely a small numeric field inside `DeviceInfo` such as `X_TP_Country`, `X_TP_ModelID`, or a build-time identifier, but the exact parameter name is not yet identified because the data-model files are encrypted with the same DES key.

A multi-threaded EVP-based brute forcer that tests all 2³² values against the known plaintext (4-byte size + zlib header `78 9c`/`78 da`/`78 01`/`78 5e` at the start of the third DES block) is running and is available at `investigations/backupcfg/reversing/brute_des.c`. An automated analysis report is generated by `investigations/backupcfg/reversing/analyze_devinfo.py`.

## 3. What `restore` Actually Writes

`rsl_sys_restoreCfg` does not call `system`, `popen`, `exec`, or any file-write function that could place an attacker-controlled payload on disk. Its key calls are:

* `util_en_desMinDo` — decryption
* `util_en_md5VerifyDigest` — integrity check
* `util_en_uncompressBuff` — decompression
* `dm_restoreCfg` — apply the XML to the in-memory data model
* `dm_setObj` — modify individual data-model objects
* `rsl_easymesh_set...`, `rsl_wifi_set...` — subsystem apply hooks
* `dm_saveCfg` — persist the updated data model to flash/UBI

`dm_saveCfg` writes to the running-config UBI partition (the same area mounted at `/var/run/misc/misc_rw/0x00300000`). This is a **configuration database**, not a general-purpose filesystem. It cannot be used to store an executable or to register a new startup service.

## 4. Persistence Mechanisms

* The root filesystem (`_rootfs`) is a read-only SquashFS/UBIFS image. Init scripts under `/etc/init.d/` and `/etc/init.d/rcS` cannot be modified at runtime and they do not source any user-writable startup hook directory.
* `rcS` does copy `/etc/mfg_config.bin` to `0x00300000` on first boot, but that source file is also on the read-only rootfs.
* The only persistent writable area is the `misc_rw` UBI partition. It holds the active data-model binary and a few other runtime files, but no startup script is loaded from there.
* BusyBox has `crond` and `crontab` compiled in, but no `crond` is started from `rcS` and the default crontab directory is `/etc` (read-only). A shell could start `crond -c <writable dir>` manually, but that is not initiated by a configuration restore and would not survive reboot unless a startup hook is created by other means.
* The `INCLUDE_PORTABLE_APP` / `INCLUDE_AGINET_APP_V2` build flags refer to the TP-Link Aginet mobile-application integration, not a third-party app platform; there is no evidence of an app-installation path through the backup file.

Therefore, **backup/restore alone cannot establish boot-time persistence for Detectic**.

## 5. Detectic Deployability Without Firmware Modification

### 5.1 Can backup/restore deploy Detectic directly?

No. The mechanism is configuration-only. Even with full knowledge of the DES key, the restore path does not accept arbitrary files or shell commands.

### 5.2 Can backup/restore enable a shell?

Very likely. The firmware image contains both `telnetd` and `dropbear`, and `libcmm.so` contains data-model objects and apply handlers for them:

* `DEV2_SSH_CFG` / `Device.X_TP_AppCfg.SSHCfg.` — starts `dropbear -p %d -r %s -d %s -A %s &`
* `DEV2_TELNET_CFG` / `Device.X_TP_AppCfg.TelnetCfg.` — starts `telnetd -p %d &`
* `rcS` creates `/var/tmp/dropbear` for Dropbear's host-key and runtime files.

This means a decrypted and modified backup that sets the appropriate `Enable`, `Port`, and `Access` parameters could cause the router to start a remote shell daemon when the configuration is applied. This would provide a **runtime shell**, but still not boot-time persistence unless the same shell is used to install a startup hook (which appears difficult on this read-only rootfs).

### 5.3 Can a shell then persist Detectic?

A shell can write to `/var/run/misc/misc_rw`, but there is **no standard writable init hook** to start Detectic on the next boot. Persistence would require one of:

1. Modifying the firmware image and reflashing (not backup/restore).
2. Finding an unpatched command-injection or file-write bug in an apply-handler or web CGI.
3. Physically modifying `/etc/init.d` after remounting the rootfs read-write (not persistent across reflash, and the bootloader may verify signatures).
4. Using a signed firmware update that the router will accept (out of scope for backup/restore).

### 5.4 Recommended path forward

1. **Continue the DES key brute-force** to decrypt the sample backup and inspect the XML schema. This proves the format and allows crafting custom backups if the 32-bit `DeviceInfo` value becomes known.
2. Once a shell is obtained (via Telnet/SSH or serial/UART), enumerate actual interfaces with `iw dev`, `busybox`, and the MT7986 Wi-Fi tools.
3. Build a small, statically linked aarch64 sensor binary and run it from `misc_rw`. For boot persistence, investigate whether a signed firmware with an added init script is acceptable, or find a writable startup hook not visible in the extracted rootfs.

## 6. Proof-of-Concept Scripts

All scripts are in `investigations/backupcfg/poc/`:

* `derive_key.py` — reproduce `getBackNRestoreK` and `getBackNRestoreKeyWithPwd`.
* `decrypt_backup.py` — decrypt a backup, verify MD5, and decompress the XML (requires the 32-bit value and optional password).
* `encrypt_backup.py` — build a new backup from an XML payload (requires the 32-bit value and optional password).

The scripts round-trip correctly when given a known key:

```bash
python3 encrypt_backup.py test.xml --value 0 -o test.bin
python3 decrypt_backup.py test.bin --value 0 -o test_out.xml
```

They cannot yet be used on the real backup until the 32-bit `DeviceInfo` value is recovered.

## 7. Remaining Unknowns

1. The exact 32-bit `DeviceInfo` value used as the no-password key input. The location is proven to be offset `0x51c`; the numeric value cannot be recovered from the provided backup without the backup password.
2. The full XML schema produced by `dm_backupCfg` (waiting for key recovery).
3. The backup/restore password used for the original backup. Full no-password and empty-password brute-force searches of the 2^32 `DeviceInfo` value space both completed with 0 matches, so the backup was created with a **non-empty password**. The same derivation logic (`getBackNRestoreKeyWithPwd`) must be brute-forced with the correct password once it is known.
4. Whether the router would actually start `dropbear`/`telnetd` when the corresponding data-model objects are set after a restore.

## 8. Conclusion

The `backupcfg.bin` mechanism is well understood. It is a **DES-ECB encrypted, zlib-compressed XML configuration** whose key is a hard-coded constant XORed with a runtime `DeviceInfo` value and an optional password MD5. Restoring a backup modifies the data model and writes it to the `misc_rw` UBI partition, but it does **not** provide a path to deploy, execute, or persist arbitrary code.

For Detectic, backup/restore is therefore **not a direct deployment vector**. It may be useful for enabling remote management features once the encryption key is known, but actual persistence and execution of the sensor still require either a firmware modification or a separate runtime exploit. Digital signature verification is **disabled** in this firmware build (`INCLUDE_DIGITAL_SIGNATURE=0`), so a correctly encrypted backup should be accepted without an additional signature check.
