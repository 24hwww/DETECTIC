# M4.4 — Definitive Runtime Validation via Legitimate Telnet/GTPR Path

## Objective

Determine whether the already-built Detectic ARM64 static binary can execute on
the real EX520V and access both the local GDPR/GTPR API and the MediaTek HAL,
using only legitimate, manufacturer-supported mechanisms under STOCK FIRMWARE.

**Constraints**: No firmware modification, no exploitation, no brute force, no
credential bypass, no privilege escalation. Use only legitimate credentials and
manufacturer-supported interfaces. Restore Telnet to disabled when testing is
complete.

---

## Phase A — Legitimate Credential Check

### Sources checked

| Source | Method | Result |
|---|---|---|
| Environment variables | `env` inspection for `DETECTIC_*`, `admin`, `password`, `secret` | No router admin credential found. `TOKENROUTER_API_KEY` is a CommandCode API key, not a router credential. No `DETECTIC_PASSWORD` set. |
| Local credential files | `find` for `.env`, `.conf`, `*.ini`, credential files | None found. Only `router/detectic.conf.example` exists (placeholder SMTP credentials, not router credentials). |
| Project documentation | Reviewed all `investigations/` markdown files, `AGENTS.md`, `CHANGELOG_PHASE2.md`, `ex520-network-map-gdpr.md` | No documented admin credential. Multiple files confirm the admin password is unknown and redacted. |
| GTPR API query | `detectic query DEV2_USER_CFG` with `user` credentials | See results below. |
| Credentials provided by user | None in this task | None provided |

### GTPR API query of `DEV2_USER_CFG` (empirical)

Using the Rust `detectic` CLI (`target/release/detectic`) with the `user`
account credentials against `http://192.168.0.1`:

```
DETECTIC_PASSWORD=<user_password> target/release/detectic --url http://192.168.0.1 --user user query DEV2_USER_CFG
```

The login succeeded (`$.ret=0`, JSESSIONID obtained). The decrypted response for
key fields (password values masked, lengths only):

| Field | Value (masked) | Visible to `user`? |
|---|---|---|
| `adminName` | `*****` (redacted) | No |
| `adminPwd` | `************` (redaction marker, 12 chars) | **No — redacted** |
| `rootPwd` | empty string | No (root account) |
| `userPwd` | plaintext (9 chars) | Yes — own password |
| `adminEnable` | `1` | Yes |
| `userEnable` | `1` | Yes |
| `rootEnable` | `0` | Yes (disabled) |
| `adminRemoteEnable` | `1` | Yes |
| `userRemoteEnable` | `1` | Yes |

**Key finding**: The `user` account can see its own password (`userPwd`) in
plaintext, but the `adminPwd` field is redacted as `************` — exactly as
documented in M4.3. The `rootPwd` field is empty (root account disabled).

### Phase A conclusion

**No legitimate admin credential is available.**

The `user` password is known and functional for GTPR API access, but:
- It is insufficient for telnet CLI authentication (confirmed in M4.3).
- The admin password is redacted in the GTPR API when queried with `user` credentials.
- The backup config is encrypted with an unknown DeviceInfo key and unknown backup password (see `backupcfg/REPORT.md`).
- The root account is disabled (`rootEnable=0`, `rootPwd` empty).

Per the M4.4 task rules: **If no legitimate admin credential exists, STOP the
live Telnet login portion and document it as BLOCKED.**

---

## Phase B — Telnet Login via GTPR — BLOCKED

Cannot proceed because Phase A found no admin credential.

The legitimate mechanism (enable Telnet via `DEV2_TELNET_CFG` with
`telnetLocalEnabled:1`) was proven to work in M4.3. However, the telnet CLI
authenticates against `DEV2_USER_CFG` and requires the **admin** password, which
is not available. The `user` password is explicitly rejected by the telnet CLI
(M4.3 confirmed).

**Blocker**: Admin password unknown and redacted in API.

---

## Phase C — Router Runtime Verification — BLOCKED

Cannot proceed because Phase B (shell access) is blocked.

All Phase C steps require a shell on the router:

### C1. Identify runtime environment

Unable to collect:
- `uname -a`
- `cat /proc/cpuinfo`
- `cat /proc/meminfo`
- `mount`
- `df -h`
- `cat /proc/mounts`
- `ls -la / /etc /etc/init.d`
- `iw dev` / `iw dev <iface> station dump`
- `which busybox iw iwinfo tcpdump hostapd_cli ubus ip ifconfig`

### C2. ARM64 binary compatibility

**Binary properties (verified on build host, not on router):**

| Property | Value |
|---|---|
| Format | ELF 64-bit LSB executable |
| Architecture | ARM aarch64 |
| Linking | statically linked (no dynamic section) |
| Stripped | yes |
| OS/ABI | UNIX - System V |
| Size | 1,110,920 bytes (~1.1 MB) |
| Build target | `aarch64-unknown-linux-musl` |
| CPU tuning | `cortex-a53` (per `.cargo/config.toml`) |
| Features | `--no-default-features` (no SQLite, no rustls) |
| SHA256 (current) | `23be46e5bb5c2d4712c267cb6ed09b940341f8ea24f6ca39f92e7805234fe642` |
| SHA256 (documented in `detectic.sha256`) | `f89ff35f6529f9a26de4795ab3773ac5ad9c00b1cda5f91763b06a9dd6e4a3d9` |

**Note**: The SHA256 of the binary on disk (`23be46e5...`) does not match the
value documented in `detectic.sha256` (`f89ff35f...`). The binary appears to have
been rebuilt after the SHA256 file was last updated. The binary remains
architecturally compatible (AArch64, static, musl).

**Architecture compatibility**: The router's `busybox` ELF header (from M4.1)
matches the Detectic binary: ELF64, AArch64, little endian, UNIX - System V.
The Detectic binary is **more portable** than busybox because it is statically
linked with zero dynamic dependencies.

### C3. Binary transfer + execution test

**BLOCKED** — no file transfer mechanism available without shell access:
- SCP/SFTP to `user` account denied (M4.2: "subsystem request failed")
- No shell to `scp` the binary to `/var/run/misc/misc_rw/`
- No firmware update mechanism that accepts arbitrary binaries

### C4. Local GDPR/GTPR API access from inside router

**BLOCKED** — requires shell access to test:
- `detectic map` against `127.0.0.1` (loopback binding unknown)
- `detectic map` against LAN IP `192.168.0.1`
- TokenID, `gl`/`go` operations from inside the router

**Unverified question**: Does the router's HTTP daemon bind to loopback
(`127.0.0.1`)? Some embedded web servers bind only to the LAN interface.

### C5. MediaTek HAL runtime access

**BLOCKED** — requires shell access to test:
- HAL ioctl interface (ABI reconstructed in M3, runtime not tested)
- Read-only ioctl probes against `/dev/MT7981` or similar device nodes

### C6. CPU/RAM measurement

**BLOCKED** — requires shell access to read `/proc/<pid>/status`,
`/proc/meminfo`, etc.

---

## Current Router State

The router was queried with read-only GTPR operations only. No modifications
were made.

| Check | Value | Status |
|---|---|---|
| Router reachable | `192.168.0.1` (ICMP, HTTP:80, SSH:22) | UP |
| `telnetLocalEnabled` | `0` | DISABLED (original state) |
| Port 23 (Telnet) | — | CLOSED |
| Port 22 (SSH) | — | OPEN (restricted `user` only) |
| Port 80 (HTTP) | — | OPEN |
| Port 443 (HTTPS) | — | — |

**Telnet is confirmed disabled.** No router configuration was changed. No
firmware was modified. No files were transferred to the router.

---

## Summary

| Classification | Verdict | Evidence |
|---|---|---|
| **A. ARM64 binary compatibility** | **PROVEN (architectural only)** | Binary is ELF64/AArch64/static/musl/1.1MB. SHA256 mismatch between disk and `detectic.sha256` — binary was rebuilt. Runtime execution unverified. See `m4_1_arm64_build.md`, `m4_2_runtime_validation.md`. |
| **B. Manual Detectic execution on EX520V** | **BLOCKED** | No shell access. Telnet can be enabled via GTPR API (proven in M4.3) but telnet CLI requires admin password. Admin password redacted in `DEV2_USER_CFG` for `user` accounts. No legitimate admin credential available. See Phase A. |
| **C. Local GDPR access from inside router** | **BLOCKED** | Requires shell access to test `127.0.0.1` vs LAN binding. GTPR works externally. See `m4_1_detectic_runtime.md`. |
| **D. HAL runtime access** | **BLOCKED** | ABI reconstructed in M3; runtime test requires shell. See `m4_1_hal_runtime.md`. |
| **E. Writable persistent storage** | **PROVEN** | `/var/run/misc/misc_rw` is a writable UBIFS partition. See M4.1. |
| **F. Automatic execution after reboot** | **NOT AVAILABLE on stock firmware** | Exhaustive rootfs search found no boot-time execution hook from writable partitions. See `m4_1_persistence.md`. |

### Blocker chain

```
No legitimate admin credential available (Phase A)
  → Cannot authenticate to telnet CLI (Phase B)
  → No shell access on router (Phase C)
  → Cannot transfer or execute ARM64 binary (C2)
  → Cannot test local GDPR / HAL / CPU/RAM (C4-C6)
```

### What would unblock

1. **The admin password for the router** — the sole requirement for telnet CLI
   login and shell access via `doFshell`.
2. **A factory reset** to set a known admin password — would lose current
   configuration.
3. **ISP-level access** to change the admin password.
4. **A vendor-approved method** to grant the `user` account shell/exec
   capability via GTPR API.

None of these are available within the M4.4 hard constraints.

---

## Security notes

- No passwords, credentials, or secrets were printed or stored in this report.
- Password field values were checked via length/marker comparison only; actual
  password values were never displayed or logged.
- No router configuration was modified (Telnet was already disabled; confirmed
  `telnetLocalEnabled=0` before and after).
- No firmware modifications, exploitation, or privilege escalation was
  attempted.
- All GTPR queries used legitimate `user` account credentials via the
  manufacturer-supported encrypted API.
- The temporary password file (if any was created) was not needed and none was
  created.
- Router state was verified unchanged: Telnet disabled, port 23 closed.
