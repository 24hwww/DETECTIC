# PHASE13_FULL_EVIDENCE_INDEX.md

## 13.0 Evidence Inventory

### Phase 14.1 additions
- PHASE14.1_ACCESS_CONFIRMATION.md
- PHASE14.1_API_READ_SURFACE.md
- PHASE14.1_RUNTIME_RECON.md
- PHASE14.1_PERSISTENCE_BOUNDARY.md
- PHASE14.1_AUTOSTART_BOUNDARY.md
- PHASE14.1_NEXT_TEST.md
- PHASE14.1_MIMO_EXECUTION_PATH_AUDIT.md (comprehensive execution/persistence/autostart audit)

### Phase 13.6 additions
- PHASE13.6_BOOT_EXECUTION_GRAPH.md
- PHASE13.6_AUTOSTART_EXHAUSTION.md
- PHASE13.6_COS_DEEP_ANALYSIS.md
- PHASE13.6_CONFIG_EXECUTION_AUDIT.md
- PHASE13.6_EVENT_EXECUTION_AUDIT.md
- PHASE13.6_IMAGE_RECONSTRUCTION.md
- PHASE13.6_TRUST_CHAIN.md
- PHASE13.6_INTEGRATION_OPTIONS.md
- PHASE13.6_DETECTIC_MIN_RUNTIME.md
- PHASE13.6_FINAL_ARCHITECTURE_MATRIX.md
- PHASE13.6_LIVE_VALIDATION_PLAN.md
- PHASE13.6_READINESS.md

### Pre-existing evidence files
- CAPTURA_BASE.md
- SUPERFICIES_DESCUBIERTAS.md
- CLASIFICACION_MECANISMOS.md
- PRUEBA_MINIMA.md
- FAILURE_ANALYSIS.md
- SEGUNDA_FAMILIA.md
- OPTIMIZACION.md
- COMBINACIONES.md
- FAILURE_RECOVERY.md
- MATRIZ_FINAL.md
- PHASE11_VALIDATION.md
- PHASE12A_INVENTARIO.md
- PHASE12B_LIVE_TEST_PLAN.md
- PHASE12C_OFFLINE_CONTROLLER.md
- PHASE12D_CONSISTENCY_AUDIT.md
- PHASE12E_IMPLEMENTATION.md
- PHASE12E_DETECTIC_RUNTIME_AUDIT.md
- PHASE12E_SIMULATOR.md
- PHASE12E_TEST_MATRIX.md
- PHASE12E_FAILURE_RESULTS.md
- PHASE12E_SECURITY_RESULTS.md
- PHASE12E_READINESS.md
- PHASE12F_LIVE_BASELINE.md
- PHASE12F_STORAGE.md
- PHASE12F_PERSISTENCE.md
- PHASE12F_TRANSFER.md
- PHASE12F_TELNET.md
- PHASE12F_DETECTIC_RUNTIME.md
- PHASE12F_PROCESS_MODEL.md
- PHASE12F_RESOURCE_BASELINE.md
- PHASE12F_REBOOT_RECOVERY.md
- PHASE12F_LIVE_RESULTS.md
- PHASE12F_READINESS.md
- BACKUPCFG_ANALYSIS.md

### Evidence source mapping

| Source | Classification | Confidence | Key Conclusion |
|---|---|---|---|
| etc/config.bba | PROVEN-OFFLINE | High | runtime_data disabled, RUNTIME_DATA_SECTION_SIZE=0 |
| _rootfs/etc/init.d/rcS | PROVEN-FROM-SOURCE | High | mounts misc_rw, misc_rw_bak, misc_isp; rootfs /etc/init.d/rcS ro |
| /bin/rcsHook | PROVEN-FROM-SOURCE | High | hardcodes /etc/rcS_hook path, ro |
| /etc/rcS_hook | PROVEN-FROM-SOURCE | High | contains .gitkeep only, ro |
| BACKUPCFG_ANALYSIS | PROVEN-OFFLINE | Medium | DES-ECB + zlib XML, backup contains config |
| PHASE11_VALIDATION | PROVEN-OFFLINE | High | No RW→EXEC chain without firmware mod |
| CLASIFICACION_MECANISMOS | PROVEN-OFFLINE | Medium | misc_rw writable persistent, no executable chain |
| PHASE12D | PROVEN-OFFLINE | High | Command allowlist, deployment transaction spec |
| PHASE12E | SIMULATED | High | Controller spec, simulator, tests passing |
| PHASE12F_* | UNKNOWN | N/A | Awaiting live hardware |

## 13.1 Firmware Identity Evidence

- Hardware: EX520V124101568249n_agc3000_0945460481
- SoC: MediaTek MT7981, aarch64
- RootFS: SquashFS/UBI, read-only
- Init: BusyBox, rcS
- Daemons: cos, cmmsyslogd, httpd, cwmp, dnsmasq, awnd, ated_tp, apsd, mapAgent, meshMonitor, ntpc, snmpd
- misc_rw: UBI ubifs mounted at /var/run/misc/misc_rw
- runtime_data: disabled
- Classification: PROVEN-FROM-SOURCE for rootfs, UNKOWN for bootloader signature

## 13.2 Unresolved Questions

1. Does cos re-apply data-model config at boot? (CRITICAL — determines autostart)
2. misc_rw capacity: 1144 KB total (PROVEN-LIVE) — binary is 1.26 MB (BLOCKS deployment)
3. Is misc_rw_bak available and writable? (alternative storage)
4. Exact bootloader signature mechanism?
5. Firmware image container format?
6. Partition layout details?
7. Can rootfs reconstruction preserve size/alignment?
8. Is web upgrade signed?
9. Can legitimate firmware be signed offline?
10. Can the binary be stored elsewhere if misc_rw is too small?

## 13.3 Confidence Summary

PROVEN-LIVE:
- Shell access via GTPR → Telnet → first-login → Lifemote Agent
- Telnet enablement via GTPR
- pwdSign bypass (first-login password reset)
- Lifemote Agent downloads and executes scripts
- phoenix.sh supervisor keeps scripts alive
- misc_rw total capacity: 1144 KB (TOO SMALL for binary)

PROVEN-OFFLINE:
- misc_rw writable, persistent, executable
- runtime_data disabled
- rcS path ro
- rcsHook path ro
- No RW→EXEC (fixed binaries only)
- Backupcfg format
- cos does NOT execute scripts from writable paths

PROVEN-FROM-SOURCE:
- phoenix.sh supervisor behavior
- Telnet CLI doFshell
- cos started at boot by rcS

SIMULATED:
- Controller deployment transaction
- Simulator behavior
- Telnet persistence hypothesis

UNKNOWN:
- cos re-apply config at boot (CRITICAL)
- misc_rw_bak availability
- Bootloader secure boot
- Firmware image reconstruction feasibility
- Legitimate signing possibility

BLOCKED:
- Binary deployment (1.26 MB > 1144 KB misc_rw)
- Autostart (depends on cos re-apply)
- Boot chain beyond rootfs

## 13.4 Next Evidence Needed

1. Test: enable Lifemote, reboot, check if phoenix.sh auto-starts (autostart)
2. Check: misc_rw_bak existence and capacity
3. Check: actual misc_rw free space after data model
4. cos binary disassembly for config re-apply behavior
5. firmware.bin full image for forensic analysis
6. bootloader binary for signature analysis

## Classification Notes

All conclusions marked per classification scheme:
PROVEN-LIVE, PROVEN-OFFLINE, PROVEN-FROM-SOURCE, SIMULATED, UNKNOWN, BLOCKED, FAILED

No assumption promoted to fact.
