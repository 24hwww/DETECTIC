# PHASE12E_SIMULATOR

Simulator EX520 built in controller/simulator.py

Features:
- ARM64 identity
- misc_rw filesystem with capacity model
- Persistent vs non-persistent files
- Reboot semantics clearing processes and tmp files
- Process table with PID reuse simulation
- Network availability toggle
- Telnet availability toggle
- File upload / activation / rollback
- Process hung simulation
- Storage capacity enforcement

Classification: SIMULATED

Limitations:
- No BusyBox command emulation details
- No actual Telnet transport
- Process exe verification simplified

Ready for offline validation.
