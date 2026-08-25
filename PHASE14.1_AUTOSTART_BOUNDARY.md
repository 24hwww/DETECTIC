# PHASE14.1_AUTOSTART_BOUNDARY.md

## Writable → EXEC autostart via GTPR

Known config OIDs:
DEV2_TELNET_CFG.telnetLocalEnabled → fixed binary telnetd launch
DEV2_SSH_CFG → fixed binary dropbear launch
DEV2_LIFEMOTE_AGENT.enable/URL → fixed agent binary

Evidence from offline analysis:
Config handlers launch fixed vendor binaries, not user-controlled executable paths.

No OID proven to accept arbitrary executable path from persistent storage.

Classification:
WRITABLE → EXEC AUTOSTART = UNKNOWN via GTPR
ARBITRARY EXECUTION = UNKNOWN

No evidence of configuration-driven arbitrary executable launch.
Fixed executable launch proven for telnet/ssh.

Cannot prove autostart of Detectic via GTPR alone.
