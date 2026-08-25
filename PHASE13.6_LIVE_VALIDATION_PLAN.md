# PHASE13.6_LIVE_VALIDATION_PLAN.md

## Tests for live EX520

1. Pristine backup
PRECONDITION: Router powered
COMMAND: backupcfg export
PASS: backup obtained
FAIL: no backup

2. Firmware identity
COMMAND: cat /proc/version, uname -a
PASS: values recorded

3. Storage
COMMAND: df -h, mount
PASS: misc_rw size known

4. Binary execution probe
COMMAND: upload probe to misc_rw, execute
PASS: runs

5. Process model
COMMAND: ps, /proc/pid/exe
PASS: verifiable

6. Telnet persistence
COMMAND: enable via backup, reboot, verify
PASS: telnet up

7. File transfer
COMMAND: scp-like via telnet
PASS: transfer works

8. Reboot persistence
COMMAND: reboot, check binary survives
PASS: survives

9. Autostart candidates
COMMAND: inspect rcS, rcsHook
PASS: documented

10. Resource baseline
COMMAND: free, top
PASS: baseline recorded

All tests offline-safe, no network config change.
