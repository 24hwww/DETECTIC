# M4.1-B/C — Shell Access and Minimal Execution Test

## M4.1-B — Shell access status

**No legitimate shell access to the EX520V is available in this session.**

The M4.1 hard constraints state:

> Only use legitimate shell access if it is already available through an
> authorized/pre-existing mechanism.

The following were checked:

| Mechanism | Status |
|---|---|
| SSH (dropbear) | Not enabled by default; would require configuration change via backup/restore (which requires the unknown 32-bit DeviceInfo key) |
| Telnet (telnetd) | Present in firmware but not enabled by default; same backup/restore limitation |
| UART/serial | Prohibited by M4.1 constraints |
| Web UI shell | No web-based shell interface exists in the stock firmware |
| Authorized pre-existing access | None available in this session |

Per the constraints, no attempt was made to obtain shell access by exploiting
the router, brute-forcing credentials, or modifying firmware.

## M4.1-C — Minimal execution test

**Not performed.** Without shell access, it is impossible to:

1. Copy a test binary to the router.
2. Execute it.
3. Record PID, exit code, stdout, stderr, `/proc/<pid>/status`, memory, CPU.

## What was verified instead

The ARM64 binary was produced and verified on the build host (see
`m4_1_arm64_build.md`). The binary is:

- ELF 64-bit LSB executable, ARM aarch64
- Statically linked (no dynamic section)
- Stripped
- 1.1 MB
- musl-compatible (built with `aarch64-unknown-linux-musl` target)
- Cortex-A53 optimized

The binary's ELF header matches the router's architecture exactly:

| Property | Router (`busybox`) | Detectic binary |
|---|---|---|
| Class | ELF64 | ELF64 |
| Machine | AArch64 | AArch64 |
| Endianness | little endian | little endian |
| OS/ABI | UNIX - System V | UNIX - System V |
| Linking | dynamically linked (musl) | statically linked (musl) |

The Detectic binary is *more* portable than `busybox` because it is statically
linked — it has zero runtime library dependencies.

## Conclusion

Runtime execution on the EX520V could not be tested because no legitimate shell
access is available. The binary is architecturally compatible and statically
linked, so execution is expected to succeed, but this remains **unverified**.
