# PHASE12F_STORAGE

## 12F.4 REAL MISC_RW DISCOVERY — OFFLINE ASSESSMENT

### What we know (PROVEN-OFFLINE):

1. `/var/run/misc/misc_rw` is a UBIFS volume mounted by rcS
2. It contains the data model binary `0x00300000`
3. It persists across reboot (confirmed by rcS code: `cp /etc/mfg_config.bin /var/run/misc/misc_rw/0x00300000` only if file doesn't exist)
4. It does NOT survive factory reset
5. It is the ONLY persistent writable surface without firmware modification

### What we DON'T know (UNKNOWN):

- Total UBI volume size
- Current free space
- Whether files other than data model can coexist safely
- Whether executing binaries from misc_rw is permitted (UBIFS allows it, but kernel/filesystem permissions may differ)
- Whether misc_rw_bak is available as alternative

### Design targets (NOT PROVEN):

From Phase 12A inventory:
- Minimum required: ~12 MB free
- Operational target: ~20 MB
- Binary size: ~1.5 MB (current build: 1.22 MB)
- Previous version backup: ~1.3 MB
- Temporary staging: ~1.3 MB
- Queue: 0 (spool is in /tmp, not misc_rw)
- State: <100 KB
- Safety margin: ~2 MB

**These are PLANNING TARGETS, not measured facts.**

### Revised storage budget (based on actual code analysis):

| Component | Size | Location | Persistent |
|-----------|------|----------|------------|
| detectic binary | 1,278,728 B (1.22 MB) | misc_rw/detectic/ | YES |
| Previous version | ~1.3 MB | misc_rw/detectic/ | YES |
| New staging | ~1.3 MB | misc_rw/detectic/detectic.new | TEMPORARY |
| State (sensor_id) | <1 KB | misc_rw/detectic/state/ | YES |
| Shell scripts | <5 KB | misc_rw/detectic/current/ | YES |
| Spool file | Variable | /tmp/ | NO (reboot lost) |
| detectic.db | Variable (persist build only) | CWD | NO |
| **Total in misc_rw** | **~4 MB** | | |

The spool file (`/tmp/detectic_buffer.jsonl`) is NOT in misc_rw. It's in /tmp which is volatile. This means:
- Offline buffering does NOT survive reboot (by default)
- misc_rw only needs the binary + state + scripts
- Estimated total: ~4 MB, well below any reasonable misc_rw capacity

### Classification:

| Item | Status |
|------|--------|
| misc_rw exists | PROVEN-OFFLINE |
| misc_rw persists reboot | PROVEN-OFFLINE |
| misc_rw total capacity | UNKNOWN |
| misc_rw current free space | UNKNOWN |
| misc_rw can store binary | UNKNOWN |
| misc_rw can run binary | UNKNOWN |
| Spool in /tmp not persistent | PROVEN-FROM-SOURCE |
| Storage budget ~4MB | DESIGN TARGET |

### STORAGE_CAPACITY: UNKNOWN

Cannot determine PASS/MARGINAL/FAIL without live measurement.

### Live test required:

```bash
df -h /var/run/misc/misc_rw
mount | grep misc_rw
du -sh /var/run/misc/misc_rw
ls -la /var/run/misc/misc_rw
```

### Risk assessment:

If misc_rw has ≥5 MB free: likely sufficient for binary + state
If misc_rw has <5 MB free: may need optimization or alternative surface
If misc_rw has <2 MB free: BLOCKED for current architecture

### Recommendation:

When live access is obtained, immediately measure misc_rw capacity before any deployment attempt.
