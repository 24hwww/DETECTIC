# PHASE14.1_NEXT_TEST.md

Current unknowns:
- misc_rw existence/writable/executable
- filesystem topology
- process/service status
- autostart mechanism

Lowest-risk read-only next test:
Query DEV2_DEV_INFO via GTPR to obtain firmware version, hardware identity, serial, MAC.

This is read-only, safe, and provides baseline identity.

Command:
python3 detectic_client.py with URL fe80::3e6a:d2ff:fe5f:abc1%enp2s0, user=user, password=***, OID DEV2_DEV_INFO via gl.

Expected result: device info JSON.

If successful, proceed to query DEV2_TELNET_CFG and DEV2_SSH_CFG read-only to confirm current service state.

Risk: LOW
No modification, no reboot, no network change.
