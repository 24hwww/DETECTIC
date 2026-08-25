# M4.2 — Real Router Execution Validation

## Objective

Perform the first real-hardware execution test of Detectic on the stock
TP-Link EX520V. This is a read-only, non-destructive runtime validation.

---

## M4.2-A — Shell access verification

### Connection attempts

The router is reachable at `192.168.0.1` from the build host (ICMP echo reply,
0.638 ms RTT). Port scan results:

| Port | Service | Status |
|---|---|---|
| 22 | SSH (dropbear) | **OPEN** |
| 23 | Telnet | CLOSED (connection refused) |
| 80 | HTTP (web UI) | OPEN |
| 443 | HTTPS (web UI) | OPEN |

### SSH authentication attempts

SSH server identifies as dropbear with password + publickey authentication.
The server reports `lockedMinute:10,failedAttempts:0,remainAttempts:5` — a
lockout policy of 10 minutes after 5 failed attempts.

| Username | Password auth | Shell access | Exec | SCP/SFTP |
|---|---|---|---|---|
| `root` | **denied** | N/A | N/A | N/A |
| `admin` | **denied** | N/A | N/A | N/A |
| `user` | **authenticated** | **denied** (PTY allocation failed) | **denied** (exec request failed) | **denied** (subsystem request failed) |

### Analysis

The `user` account authenticates with the provided web UI password, confirming
the credential is valid. However, this account has a **completely restricted
shell**:

- `ssh user@router 'uname -a'` → `exec request failed on channel 0`
- `ssh -tt user@router` → `PTY allocation request failed on channel 0`
- `scp file user@router:/tmp/` → `subsystem request failed on channel 0`
- `sftp user@router` → `subsystem request failed on channel 0`

This is consistent with TP-Link's restricted CLI account design: the `user`
account can authenticate to SSH but has no shell, no command execution, and no
file transfer capability. It is likely intended for a CLI menu interface that
is only accessible via specific client implementations, not a standard SSH
client.

The `root` account (which would have a real shell) rejects the provided
password. No root credentials are available.

### Conclusion

**M4.2 BLOCKED — no usable shell access available.**

SSH is open and the `user` account authenticates, but the account has no shell,
no exec, no PTY, and no file transfer. The `root` account requires a different
password that is not available. Telnet is closed.

Per the M4.2 hard constraints:

> If no legitimate shell access exists, DO NOT attempt exploitation or firmware
> modification. In that case, document the exact blocker and stop the runtime
> phase.

No exploitation, privilege escalation, or firmware modification was attempted.

---

## M4.2-B — Binary verification (local)

The ARM64 binary was verified locally before the shell access blocker was
reached.

### Build

```bash
cargo build --release --no-default-features --target aarch64-unknown-linux-musl
```

Result: **success** (0 warnings, 0 errors).

### ELF verification

```text
$ file target/aarch64-unknown-linux-musl/release/detectic
ELF 64-bit LSB executable, ARM aarch64, version 1 (SYSV),
  statically linked, stripped

$ readelf -h
  Class:      ELF64
  Data:       2's complement, little endian
  Machine:    AArch64
  Type:       EXEC
  OS/ABI:     UNIX - System V

$ readelf -d
  No hay sección dinámica en este fichero.
  (No dynamic section = no shared library dependencies)

$ readelf -l
  9 program headers, 4 LOAD segments, entry point 0x268740
  No INTERP segment (static binary)
```

### Binary properties

| Property | Value |
|---|---|
| Format | ELF 64-bit LSB executable |
| Architecture | ARM aarch64 |
| Linking | statically linked |
| Stripped | yes |
| Dynamic dependencies | none |
| Size | 1,083,376 bytes (1.1 MB) |
| SHA256 | `a72535c4e5f8d44fc0609e00be65550a86a033331f45269bf9bc1256f85fdc53` |
| Entry point | 0x268740 |
| Build target | `aarch64-unknown-linux-musl` |
| CPU tuning | `cortex-a53` |

### Build verification

```text
cargo fmt --check                                                    OK
cargo test                                                           69 passed (64 lib + 5 bin)
cargo build --release --no-default-features --target aarch64-unknown-linux-musl  OK
```

---

## M4.2-C through M4.2-H — Not performed

All subsequent phases require shell access to the router:

| Phase | Description | Status |
|---|---|---|
| M4.2-C | Transfer binary to router | **BLOCKED** — no file transfer capability (SCP/SFTP denied) |
| M4.2-D | Execute minimal test on router | **BLOCKED** — no exec capability |
| M4.2-E | Run `detectic map` on router | **BLOCKED** — no shell |
| M4.2-F | Test local GDPR access from inside router | **BLOCKED** — no shell |
| M4.2-G | Measure CPU/RAM usage | **BLOCKED** — no shell |
| M4.2-H | Remove binary and verify clean state | **BLOCKED** — no binary was transferred |

---

## Security note

The temporary password file (`/tmp/detectic_router_pw`) was deleted immediately
after the SSH authentication test. No passwords, secrets, or credentials appear
in this report.

---

## Summary

| Item | Result |
|---|---|
| Router reachable | YES (192.168.0.1, ICMP + SSH + HTTP) |
| SSH port open | YES (port 22, dropbear) |
| `user` account authenticates | YES |
| `user` account has shell | **NO** (restricted, no exec/PTY/SCP/SFTP) |
| `root` account accessible | **NO** (password denied) |
| Telnet available | **NO** (port 23 closed) |
| ARM64 binary verified | YES (1.1 MB, static, AArch64, musl) |
| Binary transferred to router | **NO** (blocked) |
| Binary executed on router | **NO** (blocked) |
| `detectic map` run on router | **NO** (blocked) |
| Local GDPR tested from inside | **NO** (blocked) |
| CPU/RAM measured | **NO** (blocked) |
| Router state after test | **UNCHANGED** (no modifications made) |

## Blocker

The sole blocker for M4.2 is the lack of a usable shell. The `user` SSH account
authenticates but has no command execution capability. The `root` account
requires credentials that are not available.

To unblock M4.2, one of the following is needed:

1. The root SSH password for the router.
2. An alternative legitimate shell access mechanism (e.g., ISP-enabled Telnet).
3. A vendor-approved method to enable shell access for the `user` account.

None of these are available within the M4.2 hard constraints.

## What was proven

Despite the blocker, M4.2 confirmed:

1. **The router is live and reachable** at 192.168.0.1.
2. **SSH (dropbear) is running** on port 22.
3. **The `user` account authenticates** with the web UI password (confirming
   the credential database is shared between web UI and SSH).
4. **The ARM64 binary is verified and ready** — 1.1 MB, static, AArch64, musl,
   SHA256 `a72535c4...`.
5. **No router modifications were made** — the router is in its original state.

## What remains unverified

1. Whether the binary actually executes on the router's kernel/musl runtime.
2. Whether `detectic map` works from inside the router (local GDPR).
3. Whether `127.0.0.1` is a valid GDPR endpoint from inside the router.
4. CPU/RAM consumption on the router.
5. HAL ioctl runtime behavior on actual hardware.

All of these require a usable shell, which is the single remaining blocker.
