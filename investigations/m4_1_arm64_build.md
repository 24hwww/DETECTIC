# M4.1-A — ARM64 Static Detectic Binary

## Objective

Produce a real, static, AArch64/musl Detectic binary that can execute on the
TP-Link EX520V (MediaTek MT7981, Cortex-A53, musl libc).

## Root cause of previous build failure

The M4 report failed to cross-compile because two dependencies require a C
cross-toolchain (`aarch64-linux-musl-gcc`):

| Dependency | Pulled in by | C code | Used by |
|---|---|---|---|
| `libsqlite3-sys` (bundled SQLite) | `rusqlite` | yes (sqlite3.c) | `store`, `notifier/queue` |
| `ring` (crypto/assembly) | `rustls` | yes (C + asm) | `notifier/smtp` |

Both modules are already gated behind `#[cfg(feature = "persist")]` in
`src/lib.rs`. The on-router sensor does not need SQLite persistence or SMTP
TLS — those are backend/M5 features.

## Solution

Made `rustls` and `webpki-roots` optional, gated behind the existing `persist`
feature. The on-router build uses `--no-default-features` to exclude all C
dependencies.

### Cargo.toml change

```toml
# Before (always compiled):
rustls = { version = "0.23.43", default-features = false, features = ["ring", "std", "tls12"] }
webpki-roots = "1.0.9"

[features]
default = ["persist"]
persist = ["dep:rusqlite"]

# After (optional, persist-gated):
rustls = { version = "0.23.43", default-features = false, features = ["ring", "std", "tls12"], optional = true }
webpki-roots = { version = "1.0.9", optional = true }

[features]
default = ["persist"]
persist = ["dep:rusqlite", "dep:rustls", "dep:webpki-roots"]
```

No functionality was removed. The `persist` build (default) still includes
SQLite and TLS. The on-router build (`--no-default-features`) excludes them.

## Toolchain

| Item | Value |
|---|---|
| Rust target | `aarch64-unknown-linux-musl` |
| Linker | `rust-lld` (bundled with Rust) |
| C compiler | **none required** (zero C dependencies) |
| Target CPU | `cortex-a53` (via `.cargo/config.toml`) |
| Self-contained link | `link-self-contained=yes` |

### `.cargo/config.toml`

```toml
[target.aarch64-unknown-linux-musl]
linker = "rust-lld"
rustflags = [
    "-C", "target-cpu=cortex-a53",
    "-C", "link-self-contained=yes",
]
```

## Build command

```bash
# On-router sensor (no C dependencies, pure Rust, static musl):
cargo build --release --no-default-features --target aarch64-unknown-linux-musl

# Backend/host build (with SQLite + TLS, default features):
cargo build --release
```

## Reproducibility

The build is fully reproducible with only the Rust toolchain (no external C
cross-compiler, no Docker, no special environment):

```bash
rustup target add aarch64-unknown-linux-musl
cargo build --release --no-default-features --target aarch64-unknown-linux-musl
```

## Binary verification

```text
$ file target/aarch64-unknown-linux-musl/release/detectic
target/aarch64-unknown-linux-musl/release/detectic: ELF 64-bit LSB executable,
  ARM aarch64, version 1 (SYSV), statically linked, stripped
```

```text
$ readelf -h target/aarch64-unknown-linux-musl/release/detectic
  Clase:      ELF64
  Datos:      complemento a 2, little endian
  Máquina:    AArch64
  Tipo:       EXEC (Fichero ejecutable)
```

```text
$ readelf -d target/aarch64-unknown-linux-musl/release/detectic
No hay sección dinámica en este fichero.
```

No dynamic section = no shared library dependencies = fully static.

| Property | Value |
|---|---|
| Format | ELF 64-bit LSB executable |
| Architecture | ARM aarch64 |
| Linking | statically linked |
| Stripped | yes |
| Dynamic dependencies | none |
| Size | 1,083,376 bytes (1.1 MB) |
| SHA256 | `a72535c4e5f8d44fc0609e00be65550a86a033331f45269bf9bc1256f85fdc53` |

## Verification commands

```bash
cargo fmt --check          # OK
cargo test                 # 64 lib + 5 bin = 69 passed, 0 failed
cargo build --release      # OK (default features, x86_64 host)
cargo build --release --no-default-features --target aarch64-unknown-linux-musl  # OK
```

## What the on-router binary includes

The `--no-default-features` build includes all sensor functionality:

- GTPR/GDPR client (`gtpr`, `transport`)
- Network map collector (`collector`)
- Device pseudonymization (`crypto`)
- Event extraction (`events`)
- Analytics (`analytics`)
- Publisher/upload (`publisher`)
- CLI (`main`)

It excludes only:

- SQLite persistence (`store`) — backend feature
- SMTP notification (`notifier`) — M5 backend feature

## Conclusion

A real ARM64 static Detectic binary has been produced. It is 1.1 MB, fully
static, musl-compatible, Cortex-A53 optimized, and has zero dynamic
dependencies. The build is reproducible with only the Rust toolchain.
