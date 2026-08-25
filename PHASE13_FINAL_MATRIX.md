# PHASE13_FINAL_MATRIX.md

| Method | Status | Persistence | Autostart | Recovery | Security | Brick Risk | Deployable |
|---|---|---|---|---|---|---|---|
| misc_rw + external launcher | PROVEN-OFFLINE | Yes | No internal | External | Good | None | Yes |
| rcS modification | UNKNOWN | Yes | Yes | Yes | Unknown | High | No evidence |
| init.d service | UNKNOWN | Yes | Yes | Yes | Unknown | High | No evidence |
| cos supervisor | UNKNOWN | ? | ? | ? | Unknown | Medium | No evidence |
| config-driven exec | UNKNOWN | ? | ? | ? | Unknown | Low | No evidence |
| firmware-integrated signed | UNKNOWN | Yes | Yes | Yes | Unknown | High | No evidence |

Best technically feasible: External launcher
Best safely deployable: External launcher
Firmware integration: TECHNICALLY POSSIBLE OFFLINE, NOT PROVEN DEPLOYABLE

Remaining blockers:
- Firmware binary for signature analysis
- Live misc_rw capacity measurement
- cos binary disassembly
