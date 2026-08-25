# PHASE12F_RESOURCE_BASELINE

## 12F.14 REAL RESOURCE BASELINE — OFFLINE

### What we know about the binary (PROVEN-OFFLINE):

- Size on disk: 1,278,728 bytes (1.22 MB)
- Statically linked (no shared libraries loaded)
- Single-threaded (confirmed from source: `thread_count: 1`)
- No TLS in this build (no rustls overhead)
- No SQLite in this build (no rusqlite overhead)
- Uses ureq (pure Rust HTTP client)
- Uses aes/cbc/hmac/sha2 for crypto (all pure Rust)

### Estimated resource usage (NOT MEASURED LIVE):

| Resource | Estimate | Basis |
|----------|----------|-------|
| RSS | 2-8 MB | Static musl binary, minimal deps, single thread |
| VSZ | Unknown | Static binary with no dynamic allocation patterns observed |
| CPU idle | ~0% | Sleep-based polling loop |
| CPU active | <5% | HTTP request + JSON parse + HMAC + file write |
| Threads | 1 | Single-threaded sensor loop |
| File descriptors | 3-5 | stdin/stdout/stderr + HTTP connection |
| Network | Intermittent | HTTP requests at configurable interval |

### Policy targets (NOT PROVEN REQUIREMENTS):

| Resource | Target | Status |
|----------|--------|--------|
| CPU | <10% avg | POLICY TARGET |
| RAM | <32 MB RSS | POLICY TARGET |
| Storage | <10 MB total | POLICY TARGET |
| Bandwidth | <1 KB/s upstream | POLICY TARGET |

### What the router has (PROVEN-OFFLINE):

- CPU: MediaTek MT7981, 4x Cortex-A53 @ up to 1.3 GHz
- RAM: Not measured live (EX520V typically 256 MB or 512 MB)
- Flash: SPI NAND 128 MB

### Classification:

| Item | Status |
|------|--------|
| Binary size | PROVEN-OFFLINE |
| Thread count | PROVEN-FROM-SOURCE |
| RSS estimate | SIMULATED (not measured) |
| CPU estimate | SIMULATED (not measured) |
| Actual RSS | UNKNOWN |
| Actual CPU | UNKNOWN |
| Actual memory available on router | UNKNOWN |
| Resource limits safe | SIMULATED |

### Resource baseline: UNKNOWN

Actual measurements required on live hardware.
