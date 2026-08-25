# Detectic — Binary Compatibility (EX520V)

## Target specification

The router's userland is `AArch64` + `musl`, so Detectic is configured to build
for:

```text
aarch64-unknown-linux-musl
```

The `.cargo/config.toml` already uses:

```toml
[target.aarch64-unknown-linux-musl]
linker = "rust-lld"
rustflags = [
    "-C", "target-cpu=cortex-a53",
    "-C", "link-self-contained=yes",
]
```

## Router compatibility indicators

| Check | Evidence | Verdict |
|---|---|---|
| Architecture is ARM64 | `busybox`: `ELF 64-bit LSB executable, ARM aarch64` | ✅ compatible |
| C library is musl | `readelf -l _rootfs/bin/busybox` → `/lib/ld-musl-aarch64.so.1` | ✅ compatible with a static musl binary |
| Binary must be static | `rust-lld` + `link-self-contained=yes` in `.cargo/config.toml` | intended |
| Binary must fit | release profile uses `opt-level = "z"`, `lto = true`, `codegen-units = 1`, `strip = true` | intended |

## Cross-compilation attempt

Executed:

```bash
cargo build --release --target aarch64-unknown-linux-musl
```

Result: **failure**.

The build fails at `libsqlite3-sys` (bundled SQLite, required by `rusqlite`)
because `aarch64-linux-musl-gcc` is not installed in the build environment:

```text
Compiler family detection failed: failed to find tool "aarch64-linux-musl-gcc"
error occurred in cc-rs: failed to find tool "aarch64-linux-musl-gcc"
```

A second attempt with `CC_aarch64_unknown_linux_musl=clang` and
`CFLAGS_aarch64_unknown_linux_musl="--target=aarch64-linux-musl"` also failed
because `clang` pulled the host glibc headers (`/usr/include/stdio.h`) instead
of musl aarch64 headers:

```text
/usr/include/stdio.h:28:10: fatal error: 'bits/libc-header-start.h' file not found
```

## Current host binary (reference)

```bash
cargo build --release   # x86_64 host
ls -lh target/release/detectic
```

Result: `-rwxrwxr-x 2,2M target/release/detectic`.

The x86_64 binary is 2.2 MB and statically linked with the host dependencies.
This size is within the 3 MB router budget, but the binary itself cannot run on
ARM64.

## Required dependencies with C code

The current `Cargo.lock` pulls in C libraries that need a cross toolchain:

- `ring` (for `rustls` TLS)
- `libsqlite3-sys` (for `rusqlite` / persistence)

Because the router firmware uses musl and the project intentionally targets a
fully static binary, an `aarch64` musl C cross-toolchain (`aarch64-linux-musl-gcc`)
is required to produce an ARM64 build. That toolchain is not present in the
build environment.

## Conclusion on binary compatibility

- The *runtime environment* is compatible with a static `aarch64-unknown-linux-musl` binary.
- The *current build* cannot produce an ARM64 binary in this environment without a proper cross toolchain.
- With a suitable `aarch64-linux-musl-gcc` and musl headers, the target is expected to build and run; this has not been reproduced here.
