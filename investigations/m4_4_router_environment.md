# M4.4 Phase A — Router Shell Environment

## Date
2026-08-23

## Method
Legitimate administrative shell via documented mechanisms (see `admin_shell_access.md`).
Shell obtained via Lifemote Agent → `telnetd -p 8888 -l /bin/sh`.

## System Information

### Kernel
```
Linux EX520 5.4.211 #1 SMP Tue Oct 15 18:31:04 CST 2024 aarch64 GNU/Linux
```
```
Linux version 5.4.211 (root@7086da4efda9) (gcc version 8.4.0 (OpenWrt GCC 8.4.0 r16649-bcaabe6d05)) #1 SMP Tue Oct 15 18:31:04 CST 2024
```

### Architecture
- **CPU**: ARMv8 Processor rev 4 (v8l), Cortex-A53 (part 0xd03)
- **Cores**: 2 (SMP)
- **Features**: fp asimd evtstrm aes pmull sha1 sha2 crc32 cpuid
- **BogoMIPS**: 26.00 per core

### libc
- **Implementation**: musl
- **Dynamic linker**: `/lib/ld-musl-aarch64.so.1` → `/lib/libc.so` (518 KB)
- **Note**: Detectic binary is statically linked with musl — no dynamic dependency on router libc.

### Memory
```
MemTotal:         230216 kB  (225 MB)
MemFree:           35144 kB  (34 MB)
MemAvailable:      53868 kB  (53 MB)
SwapTotal:             0 kB
```
- **Slab**: 48684 kB (significant kernel allocation, mostly SUnreclaim 43344 kB)
- **AnonPages**: 19828 kB

### Filesystems
| Mount | Type | Size | Mode | Notes |
|-------|------|------|------|-------|
| `/` | squashfs | 16 MB | **ro** | Read-only rootfs |
| `/dev` | devtmpfs | 115 MB | rw | Device nodes |
| `/var` | ramfs | — | rw | Volatile (RAM-backed) |
| `/var/tmp` | (via /var) | — | rw | **Writable** — target for Detectic binary |
| `/var/run/misc/misc_ro` | ubifs | 1.1 MB | ro | Config (read-only copy) |
| `/var/run/misc/misc_rw` | ubifs | 1.1 MB | rw | Config (writable) |
| `/var/run/misc/misc_rw_bak` | ubifs | 1.1 MB | rw | Config backup |
| `/var/run/misc/misc_isp` | ubifs | 1.1 MB | ro | ISP config |
| `/sys` | sysfs | — | rw | |
| `/sys/kernel/debug` | debugfs | — | rw | |

### Writable Directories
- `/var/tmp` (ramfs, volatile — cleared on reboot)
- `/var` (ramfs, volatile)
- `/dev` (devtmpfs)

### Storage
- Rootfs: 16 MB squashfs, 100% used (read-only)
- UBIFS volumes: ~1 MB each
- No USB mount detected
- `/var/tmp` is RAM-backed (no persistent storage for temporary files)

### Device Nodes
- No `/dev/wifi*`, `/dev/rai*`, `/dev/rax*` device nodes
- Standard: mem, null, zero, random, urandom, tty, console, ptmx
- Serial: ttyS0, ttyS1, ttyS2
- MTD: mtd0, mtd0ro, mtdblock0
- Network: net (directory)

## Process List (Networking/Wi-Fi Relevant)

| PID | VSZ | Process | Notes |
|-----|-----|---------|-------|
| 1 | 948 | init | System init |
| 937 | 22180 | cos | Core OS management |
| 939 | 5456 | cmmsyslogd | Syslog daemon |
| 957 | 5624 | igmpd | IGMP proxy |
| 1224 | 18336 | cwmp | TR-069 CWMP |
| 1240 | 17196 | xmpp | XMPP client |
| 1306 | 1016 | dnsmasq | DNS/DHCP |
| 1308 | 6072 | dhcpd | DHCP server |
| 1314 | 5464 | dhcpc | DHCP client |
| 1776 | 5536 | wlNetlinkTool | **Wi-Fi netlink tool** |
| 2049 | 16516 | tmpd | Local management (port 20002) |
| 2073 | 14172 | tdpd | TP-Link discovery protocol |
| 2113 | 16780 | httpd | **Web server (ports 80/443)** |
| 2134 | 3448 | upnpd | UPnP (port 1900) |
| 2142 | 13648 | snmpd | SNMP |
| 2625 | 17480 | cloud-brd | Cloud broadcast |
| 2627 | 17052 | cloud_client | Cloud client |
| 2629 | 16920 | cloud_https | Cloud HTTPS |
| 2691 | 13100 | meshMonitor | Mesh monitoring |
| 2740 | 5840 | mapController | EasyMesh controller |
| 2746 | 5916 | mapAgent | EasyMesh agent |
| 2749 | 4148 | nrd | Network routing daemon |
| 2882 | 22348 | obuspa | **TR-369 USP agent** (connects to 52.54.34.102:8883 MQTT) |
| 2886 | 14804 | tr143d | TR-143 diagnostics |
| 2901 | 13880 | wanconnd2 | WAN connection |
| 8545 | 1316 | **dropbear** | **SSH server (port 22)** |
| 9871 | 948 | telnetd | Telnet (port 23, our enabled) |
| 9945 | 956 | telnetd | Our shell (port 8888) |

### Wi-Fi Kernel Threads
| PID | Process | Notes |
|-----|---------|-------|
| 1464 | sub_wifi_thrd | Wi-Fi subsystem thread |
| 1516 | warp | Wi-Fi accelerator |
| 1517 | wed_task0 | Wireless Ethernet Dispatch |
| 1520 | RtmpCmdQTask | Ralink command queue |
| 1521 | RtmpWscTask | WPS task |
| 1522 | HwCtrlTask | Hardware control |
| 1523 | ser_task | Station event reporting? |
| 1534 | RtmpMlmeTask | MLME task (MAC layer management) |

## Network Sockets (Listening)

| Proto | Local Address | PID/Program | Notes |
|-------|--------------|-------------|-------|
| tcp | 127.0.0.1:20002 | 2049/tmpd | Local management |
| tcp | 0.0.0.0:1900 | 2134/upnpd | UPnP |
| tcp | 0.0.0.0:53 | 1306/dnsmasq | DNS |
| tcp | 0.0.0.0:22 | 8545/dropbear | **SSH** |
| tcp | :::80 | 2113/httpd | **HTTP (GTPR/GDPR)** |
| tcp | :::53 | 1306/dnsmasq | DNS |
| tcp | :::22 | 8545/dropbear | SSH |
| tcp | :::23 | 9871/telnetd | Telnet (temporary) |
| tcp | :::8888 | 9945/telnetd | Our shell (temporary) |
| tcp | :::443 | 2113/httpd | **HTTPS (GTPR/GDPR)** |

## Key Observations

1. **SSH (dropbear) is running on port 22** — This was not visible in the firmware config
   (`INCLUDE_SSH_ACCESS` not set), but dropbear is actively running. This may provide
   an alternative shell access mechanism (not investigated per scope constraints).

2. **httpd serves both HTTP (80) and HTTPS (443)** — The GTPR/GDPR API is accessible
   from inside the router via `localhost:80` or `localhost:443`.

3. **tmpd on 127.0.0.1:20002** — Local management service, potentially useful for
   internal API access.

4. **`/var/tmp` is the writable target** — RAM-backed, cleared on reboot. Safe for
   temporary binary deployment.

5. **No Wi-Fi device nodes in `/dev`** — Wi-Fi is accessed via netlink/ioctl on
   network interfaces, not character devices.

6. **`wlNetlinkTool` (PID 1776)** — A TP-Link Wi-Fi netlink tool that may provide
   station/event information via netlink.

7. **`obuspa` (TR-369)** — Connected to external MQTT broker (52.54.34.102:8883),
   indicating TR-369 USP is active.

8. **Rootfs is squashfs (read-only)** — Cannot modify firmware files at runtime.

9. **53 MB available RAM** — Sufficient for Detectic (1.1 MB binary, expected
   <10 MB RSS).

## Security Note
- No credentials, secrets, tokens, or password databases were dumped.
- All information collected is standard system metadata.
