# PHASE12E_TEST_MATRIX

Tests executed:
1. happy_path - deploy artifact, verify healthy - PASS
2. rollback_on_bad_checksum - deploy corrupt artifact triggers rollback - PASS
3. reboot_recovery - binary persists after reboot, process restarts - PASS
4. idempotency - repeated deploy safe - PASS
5. storage_insufficient - deploy rejected when storage low - PASS
6. process_hung - supervisor detects HUNG state - PASS

Failure injections covered:
- checksum mismatch
- storage insufficient
- reboot
- hung process

Security tests:
- command allowlist enforced in controller design
- path traversal prevented
- No raw shell exposure

Classification:
PROVEN: deployment transaction, rollback, persistence, process supervision
SIMULATED: simulator behavior
UNKNOWN: real Telnet, real BusyBox command output
LIVE_REQUIRED: capacity measurement, Telnet persistence
