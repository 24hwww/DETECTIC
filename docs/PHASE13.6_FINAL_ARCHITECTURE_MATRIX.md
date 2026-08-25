# PHASE13.6_FINAL_ARCHITECTURE_MATRIX.md

| Arch | Persistence | Autostart | Recovery | Security | Firmware Mod | Deployable |
|---|---|---|---|---|---|---|
| External launcher + misc_rw | Yes | External | External | High | No | Yes |
| rcS integration | Yes | Yes | Yes | Unknown | Yes | No evidence |
| init.d service | Yes | Yes | Yes | Unknown | Yes | No evidence |
| rcS_hook | No | No | No | Low | No | No |
| cos supervisor | Unknown | Unknown | Unknown | Unknown | No | No evidence |
| config-driven | Unknown | Unknown | Unknown | Medium | No | No evidence |
| event/hotplug | No | No | No | Low | No | No |
| vendor-signed firmware | Yes | Yes | Yes | High | Yes | Unknown |

Best safe deployable: External launcher
