# PHASE12F_TRANSFER

## 12F.6 REAL ARM64 EXECUTION PROBE — TRANSFER MECHANISM

### Current situation:

The Detectic binary (1.22 MB) needs to be transferred to the router's misc_rw partition. No transfer mechanism has been validated on live hardware.

### Options analysis:

#### A. SSH/SCP

- **Status**: UNKNOWN
- `INCLUDE_SSH_ACCESS` not set in firmware config → SSH likely NOT enabled by default
- `dropbear` binary exists in firmware → could be enabled via config
- Enabling requires: backupcfg modification with DES key (key derivation partially understood, 32-bit DeviceInfo value unknown)
- If enabled: SCP would be the most reliable transfer method
- **Classification**: UNKNOWN — depends on SSH enablement

#### B. SFTP

- Same dependencies as SSH
- **Classification**: UNKNOWN

#### C. Telnet + base64 encoding

- **Status**: UNKNOWN
- If Telnet is available: could transfer via `echo <base64> | base64 -d > file`
- Limitations: BusyBox base64 support uncertain, line length limits, slow for 1.22 MB
- Estimated transfer time: >5 minutes for 1.22 MB at Telnet speeds
- **Classification**: UNKNOWN — depends on Telnet availability + BusyBox base64

#### D. HTTP upload via web CGI

- **Status**: UNKNOWN
- Web UI has backup/restore functionality
- No evidence of arbitrary file upload endpoint
- CGI handlers are for config, not file deployment
- **Classification**: UNKNOWN — unlikely but possible

#### E. Detectic's own GTPR transport

- **Status**: PROVEN-FROM-SOURCE
- Detectic uses HTTP to talk to router's GTPR API
- Could theoretically be extended to receive files
- But this requires Detectic to already be running on the router
- **Classification**: NOT APPLICABLE for initial deployment

#### F. Physical media (USB/SD)

- **Status**: UNKNOWN
- EX520V may have USB port (not confirmed)
- If available: most reliable transfer method
- **Classification**: UNKNOWN

#### G. Controller-side transfer

- The external controller (Python) needs a way to push files
- Depends on management transport (SSH, Telnet, or other)
- **Classification**: UNKNOWN — depends on management transport

### Recommended investigation order (when live):

1. Check if SSH/Telnet is already available (port scan)
2. If SSH: use SCP (fastest, most reliable)
3. If Telnet only: test BusyBox base64 support, consider chunked transfer
4. If neither: investigate web UI file upload or physical media

### Classification:

| Item | Status |
|------|--------|
| SSH available | UNKNOWN |
| SCP available | UNKNOWN |
| Telnet available | UNKNOWN |
| Telnet base64 support | UNKNOWN |
| HTTP file upload | UNKNOWN |
| USB/SD available | UNKNOWN |
| Best transfer method | UNKNOWN |
| Transfer throughput | UNKNOWN |
| Transfer integrity | UNKNOWN |

### TRANSFER: UNKNOWN

Cannot determine viable transfer mechanism without live hardware access.
