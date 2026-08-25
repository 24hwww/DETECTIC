# PHASE13_IMAGE_RECONSTRUCTION.md

## A. Firmware container format
UNKNOWN
No firmware binary extracted. Container format, headers, offsets, compression, signatures UNKNOWN.
Missing artifact: firmware.bin

## B. Root filesystem reconstruction
PROVEN-FROM-SOURCE: _rootfs exists as extracted ubifs
UNKNOWN: Can rootfs be rebuilt byte-for-byte? Image boundaries unknown.
SIMULATED: In theory ubifs can be unpacked/repacked, size/alignment may change.

## C. Repacking feasibility
POSSIBLE: unpack/repack offline
UNKNOWN: integrity/checksum update
UNKNOWN: signature validity
IMPOSSIBLE to prove deployable without firmware binary and signature chain

## D. Signature boundary
UNKNOWN
No evidence of signed components in extracted rootfs.
config.bba shows INCLUDE_DIGITAL_SIGNATURE not set.
Bootloader verification UNKNOWN.

## E. Legitimate deployment
UNKNOWN
No evidence of TP-Link legitimate signing path for custom images.
Technically modifiable offline ≠ deployable.

Classification: Image reconstruction UNKNOWN due to missing firmware binary.
