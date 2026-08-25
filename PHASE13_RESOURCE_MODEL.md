# PHASE13_RESOURCE_MODEL.md

## Corrected architecture

INSTALLATION persistence required:
- Detectic executable ~1.3 MB
- Config ~10 KB
- Rollback copy ~1.3 MB
Total ~2.6 MB

RUNTIME:
- RAM 32 MB target
- CPU 10% target
- Storage transient <5 MB

OPTIONAL:
- Queue/state ~1 MB
- Logs minimal

Minimum storage: ~3 MB
Recommended: ~10 MB

Classification:
SIMULATED based on binary size
PROVEN-OFFLINE binary size 1.3 MB

No persistent RF database required.
