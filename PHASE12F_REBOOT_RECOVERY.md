# PHASE12F_REBOOT_RECOVERY

## 12F.16 REAL REBOOT RECOVERY — OFFLINE

### Expected behavior (design, not tested):

```
Detectic HEALTHY
    ↓
router reboot
    ↓
controller detects loss (connection timeout)
    ↓
controller enters REBOOT_RECOVERY state
    ↓
router returns (boot time: UNKNOWN)
    ↓
management transport reconnects (delay: UNKNOWN)
    ↓
persistent binary verified in misc_rw (assumed: yes)
    ↓
Detectic restarted by controller
    ↓
health verified
    ↓
HEALTHY
```

### What we know (PROVEN-OFFLINE):

1. Binary in misc_rw should persist reboot (design prediction)
2. Spool in /tmp is VOLATILE — lost on reboot
3. State in misc_rw should persist reboot
4. Detectic has no autostart — external launcher must restart it
5. Detectic has SIGTERM handler for graceful shutdown
6. Controller has state persistence for crash recovery

### What we DON'T know (UNKNOWN):

| Item | Status |
|------|--------|
| Router boot time | UNKNOWN |
| Management availability after boot | UNKNOWN |
| Time for Telnet/SSH to start after boot | UNKNOWN |
| Whether binary survives real reboot | UNKNOWN |
| Whether controller reconnect works | UNKNOWN |
| Whether spool loss causes data loss | DESIGN: yes, bounded by spool size |

### Risks:

1. **Spool data loss**: Default spool is `/tmp/detectic_buffer.jsonl` (volatile). On reboot, any unsent buffered data is lost.
   - Mitigation: drain spool before reboot (if controller detects reboot gracefully)
   - Mitigation: move spool to misc_rw (requires code change)

2. **Controller state loss**: Controller state is in `/tmp/controller_state.json` (volatile). On controller restart, state must be reconstructed.
   - Mitigation: controller re-probes router state on startup

3. **Management transport delay**: After router reboot, Telnet/SSH may take time to start. Controller must wait and retry.

### Classification:

| Item | Status |
|------|--------|
| Binary survives reboot (design) | SIMULATED |
| Spool lost on reboot | PROVEN-FROM-SOURCE (default /tmp) |
| State survives reboot | PROVEN-FROM-SOURCE (misc_rw path) |
| Controller state survives reboot | UNKNOWN (depends on controller persistence) |
| Boot time | UNKNOWN |
| Recovery time | UNKNOWN |
| Full reboot recovery | SIMULATED (not tested live) |
