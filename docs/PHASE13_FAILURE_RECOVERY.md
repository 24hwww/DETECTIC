# PHASE13_FAILURE_RECOVERY.md

## Scenarios

Detectic crash → external launcher restarts
Detectic hang → external launcher restarts
Router reboot → binary persists in misc_rw, external launcher re-provisions
Power loss → binary persists
Network loss → sensor continues, buffering optional
Backend loss → sensor continues
Config reload → binary persists
Firmware upgrade → binary lost
Factory reset → binary lost
Storage exhaustion → risk

Classification:
SIMULATED based on controller spec
PROVEN-OFFLINE binary persistence in misc_rw

Sensor recovery decoupled from data recovery.
