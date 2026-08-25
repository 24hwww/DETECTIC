# PHASE12F_TELNET

## 12F.7 REAL TELNET VALIDATION — OFFLINE ASSESSMENT

### What we know (PROVEN-OFFLINE):

1. `telnetd` binary exists in firmware image
2. Data model objects exist: `DEV2_TELNET_CFG`
3. Apply handler: `telnetd -p %d &` (starts telnetd on configured port)
4. Build flags: `INCLUDE_WEB_TELNET=y`, `INCLUDE_REMOTE_TELNET=y`
5. `INCLUDE_TELNET_LOGIN_WAIT=y`
6. Backup format: DES-ECB + zlib + MD5 XML
7. Key derivation: hardcoded constant XOR DeviceInfo value
8. No signature verification: `INCLUDE_DIGITAL_SIGNATURE` not set

### What we DON'T know (UNKNOWN):

1. Whether Telnet is currently enabled on the live router
2. What port Telnet would be on (design suggests 23)
3. What credentials are required (admin password?)
4. Whether Telnet persists across reboot (design predicts yes)
5. Whether the 32-bit DeviceInfo value can be obtained
6. Whether a backup password is set (previous brute-force found 0 matches for empty password)
7. Whether Telnet is accessible from LAN only or also WAN

### Backup modification process (PROVEN-OFFLINE, not tested live):

1. Obtain backupcfg from live router
2. Decrypt using DES key (requires 32-bit DeviceInfo value)
3. Decompress zlib
4. Parse XML
5. Modify TelnetCfg.Enable = true, TelnetCfg.Port = 23
6. Re-compress zlib
7. Calculate MD5
8. Re-encrypt DES
9. Restore via web UI

### Blockers:

| Blocker | Status |
|---------|--------|
| 32-bit DeviceInfo value | UNKNOWN |
| Backup password (if any) | UNKNOWN |
| Ability to decrypt backup | UNKNOWN (key unknown) |
| Ability to restore backup | UNKNOWN (untested) |
| Telnet enablement verified | UNKNOWN |

### Classification:

| Item | Status |
|------|--------|
| telnetd in firmware | PROVEN-OFFLINE |
| Telnet config objects | PROVEN-OFFLINE |
| Telnet apply handler | PROVEN-OFFLINE |
| Telnet enabled on live router | UNKNOWN |
| Telnet port | UNKNOWN |
| Telnet credentials | UNKNOWN |
| Telnet persists reboot | UNKNOWN |
| Telnet LAN-only | UNKNOWN |
| Backup decrypt possible | UNKNOWN (DeviceInfo value unknown) |
| Backup restore works | UNKNOWN (untested) |

### TELNET: UNKNOWN

Cannot validate Telnet without:
1. Live router access
2. Port scan to check current state
3. Backup decryption (requires DeviceInfo value)
4. Live restore test

### Alternative if Telnet fails:

If Telnet cannot be enabled, the management transport must use another mechanism (SSH, web CGI, or physical access).
