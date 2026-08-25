# PHASE13_EVIDENCE_INDEX

## Existing Evidence Files
- CAPTURA_BASE.md — firmware base capture, MT7981, ARM64, rootfs ro, misc_rw rw
- SUPERFICIES_DESCUBIERTAS.md — writable surfaces, hotplug, rcS_hook
- CLASIFICACION_MECANISMOS.md — not read yet
- PRUEBA_MINIMA.md — not read yet
- FAILURE_ANALYSIS.md — not read yet
- SEGUNDA_FAMILIA.md — not read yet
- OPTIMIZACION.md — not read yet
- COMBINACIONES.md — not read yet
- FAILURE_RECOVERY.md — not read yet
- MATRIZ_FINAL.md — architecture external launcher + misc_rw
- PHASE11_VALIDATION.md — runtime_data disabled, no RW→EXEC chain
- PHASE12A_INVENTARIO.md — inventory, capacity model 12 MB
- PHASE12B_LIVE_TEST_PLAN.md — live tests, safety gate
- PHASE12C_OFFLINE_CONTROLLER.md — controller design
- PHASE12D_CONSISTENCY_AUDIT.md — consistency audit
- PHASE12E_IMPLEMENTATION.md — controller implementation spec
- PHASE12E_DETECTIC_RUNTIME_AUDIT.md — Detectic runtime audit
- PHASE12E_SIMULATOR.md — simulator doc
- PHASE12E_TEST_MATRIX.md — test matrix
- PHASE12E_FAILURE_RESULTS.md — failure results
- PHASE12E_SECURITY_RESULTS.md — security results
- PHASE12E_READINESS.md — readiness
- BACKUPCFG_ANALYSIS.md — DES-ECB backup format
- PHASE12F_READINESS.md — live readiness

## Firmware Artifacts
- _rootfs/ — extracted rootfs
- etc/config.bba — build config, INCLUDE_RUNTIME_DATA_SECTION not set
- Cargo.toml — Detectic project
- target/aarch64-unknown-linux-musl/release/detectic — 1.3 MB static binary

## Classification Summary
PROVEN OFFLINE:
- ARM64 static binary
- rootfs ro, misc_rw rw
- runtime_data disabled
- backupcfg DES-ECB
- rcS_hook exists but path is ro
- No RW→EXEC chain

UNKNOWN:
- firmware signature model
- bootloader verification
- image reconstruction feasibility
- cos supervisor internals
- exact boot chain details
- upgrade behavior

SIMULATED:
- controller, simulator

## Next Steps
13.1 Establish exact firmware model
13.2 Map complete boot chain
13.3 Signature/integrity analysis
13.4 Image forensics
...
