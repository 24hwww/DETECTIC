# Detectic — Runtime Execution (EX520V)

## Status

No runtime execution was performed on the EX520V.

## Why

The M4 milestone explicitly permits only legitimate, stock-firmware mechanisms.
No pre-existing shell access (SSH, Telnet, serial console, UART, etc.) was
available in this session, and the following actions are prohibited:

- enabling services via a decrypted backup (would require the unknown 32-bit `DeviceInfo` value and possibly a backup password)
- UART / serial access
- brute-force of credentials
- exploitation of any vulnerability
- any firmware modification

Without a shell, the steps in Phase 4 cannot be carried out:

1. Copy Detectic to a writable directory.
2. Execute it manually.
3. Run `detectic map`.
4. Verify process start, exit, memory, CPU, GDPR access.

## What was attempted on the build host

- `cargo build --release` on `x86_64-unknown-linux-gnu` → succeeded, 2.2 MB binary.
- `cargo test` on `x86_64` → 69 tests passed.
- `cargo build --release --target aarch64-unknown-linux-musl` → failed (missing cross toolchain).

## What could not be validated

| Check | Evidence |
|---|---|
| Process starts on EX520 | unverified |
| Exits cleanly | unverified |
| Memory usage | unverified |
| CPU usage | unverified |
| GTPR/GDPR access from the router | unverified |
| No kernel errors | unverified |

## Conclusion

Runtime execution on real hardware is a required validation step and is not
possible without first obtaining a legitimate shell. This is a blocker for the
overall `SUPPORTED` classification.
