# PHASE12F_DETECTIC_RUNTIME

## 12F.10 DETECTIC ARTIFACT VALIDATION — OFFLINE

### Binary properties (PROVEN-OFFLINE):

```
File: target/aarch64-unknown-linux-musl/release/detectic
Size: 1,278,728 bytes (1.22 MB)
SHA-256: 89abf70c17c4cab3703f6ed52f946f989413f10a7d0a5002a5b06d519cb797cd
Arch: ELF 64-bit LSB executable, ARM aarch64, version 1 (SYSV)
Linking: statically linked
Stripped: yes
Dynamic deps: none
Target CPU: Cortex-A53 (ARMv8-A baseline)
Build: --no-default-features (no persist/TLS/rusqlite)
```

### Runtime behavior (PROVEN-FROM-SOURCE):

#### CLI arguments
- No `--daemon` flag
- No `--log` flag
- No `--verbose` flag
- Uses subcommands: `sensor`, `map`, `status`, `version`, `health`, `config`, etc.

#### Sensor loop
- Runs in foreground (no daemonization)
- Polls router GTPR API via HTTP
- Sleeps `interval` seconds between polls
- Exits on SIGTERM/SIGINT (graceful shutdown)
- Exits after `MAX_RESTART_ATTEMPTS` (3) consecutive poll failures

#### Signal handling (PROVEN-FROM-SOURCE):
```rust
// src/runtime.rs
extern "C" fn handle_sig(_sig: i32) {
    request_shutdown();
}
// Registers SIGTERM (15) and SIGINT (2) handlers
```

#### Health model (PROVEN-FROM-SOURCE):
- Reads `/proc/self/status` for VmRSS
- Reads `/proc/self/stat` + `/proc/uptime` for process uptime
- No heartbeat file
- No heartbeat endpoint
- No structured health output
- Health derived from process existence + logs

#### Process inspection (PROVEN-FROM-SOURCE):
- Does NOT shell out to `ps`, `pidof`, `pgrep`
- Does NOT read `/proc/<pid>/exe`
- Does NOT create PID file
- Process identified by: external tool checks `ps` output

#### Writable paths (PROVEN-FROM-SOURCE):
- Spool: `/tmp/detectic_buffer.jsonl` (VOLATILE, lost on reboot)
- State: `/var/run/misc/misc_rw/detectic/state/sensor_id` (PERSISTENT)
- Scripts: `/var/run/misc/misc_rw/detectic/current/detectic-*.sh` (PERSISTENT)

#### Network behavior (PROVEN-FROM-SOURCE):
- HTTP to router GTPR API (default: http://192.168.0.1)
- HTTP to backend URL (configurable, optional)
- No TLS in this build (--no-default-features)
- Uses ureq HTTP client (pure Rust, no C deps)

#### Environment variables (PROVEN-FROM-SOURCE):
All configuration via environment variables or CLI args.
No config file loading in the on-router build (persist feature disabled).

### What the controller should NOT assume:

| Assumption | Reality |
|------------|---------|
| `--daemon` flag works | DOES NOT EXIST |
| `--log` flag works | DOES NOT EXIST |
| stdout is a heartbeat | NOT AUTHORITATIVE |
| pidof finds detectic | CODE DOES NOT USE IT |
| /proc/<pid>/exe works | CODE DOES NOT USE IT |
| SCP can transfer files | NOT USED BY CODE |
| Spool persists reboot | DEFAULT IS /tmp (VOLATILE) |
| Heartbeat file exists | DOES NOT EXIST |
| Backend always reachable | MAY BE UNREACHABLE |

### Classification:

| Item | Status |
|------|--------|
| Binary architecture | PROVEN-OFFLINE |
| Binary static linking | PROVEN-OFFLINE |
| CLI arguments | PROVEN-FROM-SOURCE |
| Signal handling | PROVEN-FROM-SOURCE |
| Health model | PROVEN-FROM-SOURCE |
| Writable paths | PROVEN-FROM-SOURCE |
| Network behavior | PROVEN-FROM-SOURCE |
| Execution on real hardware | UNKNOWN |
| Actual RSS/memory | UNKNOWN |
| Actual CPU usage | UNKNOWN |
| Actual startup time | UNKNOWN |
| Graceful shutdown behavior | PROVEN-FROM-SOURCE (SIGTERM handler exists) |
| Spool persistence | PROVEN-FROM-SOURCE (default /tmp = volatile) |
