# PHASE13_FIRMWARE_INTEGRATION_EXPERIMENT.md

## Experiment
Conceptual offline modification of _rootfs copy to add /etc/init.d/S99detectic launching /var/run/misc/misc_rw/detectic/detectic

## Results
PROVEN-FROM-SOURCE: rootfs can be unpacked offline
UNKNOWN: Can modified rootfs be rebuilt into valid ubifs image?
UNKNOWN: Will container accept rebuilt rootfs?
UNKNOWN: Will signature remain valid?

Barrier:
Integrity metadata changes → signature invalid → bootloader rejection UNKNOWN

Classification:
TECHNICALLY POSSIBLE OFFLINE, DEPLOYMENT BARRIER UNKNOWN due to signature/verification

No flash performed.
