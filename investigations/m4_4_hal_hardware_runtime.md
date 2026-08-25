# M4.4 Phase E — HAL Hardware Runtime Validation

## Date
2026-08-23

## Objective
Validate whether Detectic can access the MediaTek HAL (Hardware Abstraction Layer)
on the real EX520V hardware through device nodes, ioctls, and sysfs/procfs.

## Method
Read-only inspection of device nodes, sysfs entries, procfs entries, and
MediaTek private ioctls via `iwpriv`.

## Device Nodes

### `/dev` Inventory
```
gpiochip0, mem, null, port, zero, full, random, urandom, kmsg, tty, console,
fuse, ptmx, ttyS0, ttyS1, ttyS2, hwrng, loop-control, loop0-7, mtd0, mtd0ro,
mtdblock0, net
```

**No Wi-Fi-specific device nodes found:**
- No `/dev/wifi*`
- No `/dev/rai*` or `/dev/rax*`
- No `/dev/mtk*`
- No `/dev/rfkill`

The MediaTek MT7981 Wi-Fi driver does not expose character device nodes. Wi-Fi
is accessed exclusively through network interfaces and private ioctls.

## Sysfs

### Network Interfaces
```
/sys/class/net/rai0 through rai6  (2.4 GHz AP virtual interfaces)
/sys/class/net/rax0 through rax6  (5 GHz AP virtual interfaces)
/sys/class/net/apclii0            (2.4 GHz AP client)
/sys/class/net/apclix0            (5 GHz AP client)
```

### Interface Attributes
| Attribute | rai0 | rax0 |
|-----------|------|------|
| address | 3c:6a:d2:5f:ab:c1 | 3c:6a:d2:5f:ab:c3 |
| operstate | unknown | unknown |
| wireless | present | present |

**No `/sys/class/ieee80211/`** — The MediaTek driver does not register with
cfg80211/nl80211. This means standard Linux wireless tools (`iw`, `nl80211`)
cannot be used.

## Procfs

### `/proc/net/wireless`
```
Inter-| sta-|   Quality        |   Discarded packets               | Missed | WE
 face | tus | link level noise |  nwid  crypt   frag  retry   misc | beacon | 22
  rai0: 0000   10.  -256   -51        0      0      0      0      0        0
  rax0: 0000   10.  -256   -51        0      0      0      0      0        0
```

Available data:
- Link quality: 10 (static, not per-station)
- Signal level: -256 (invalid, no active station on this interface)
- Noise floor: -51 dBm (useful for radio stats)

**No per-station data in procfs.**

## MediaTek Private Ioctls (iwpriv)

### Available Ioctls (per interface)
| Ioctl | Code | Set | Get | Status |
|-------|------|-----|-----|--------|
| `set` | 8BEB | 1536 | 2047 | Works (driver configuration) |
| `show` | 8BF1 | 1024 | 0 | Works (empty output without args) |
| `phystate` | 8C01 | 1024 | 1024 | Not tested |
| `get_site_survey` | 8BED | 1024 | 1024 | **Works** — returns AP scan |
| `set_wsc_oob` | 8BF9 | 1024 | 1024 | Not tested |
| `get_mac_table` | 8BEF | 1024 | 1024 | **CRASHES** (segfault) |
| `get_driverinfo` | 8BFD | 1024 | 1024 | **Works** — driver version |
| `e2p` | 8BE7 | 1024 | 1024 | Not tested (EEPROM access) |
| `bbp` | 8BE3 | 1024 | 1024 | Not tested (BBP register access) |
| `mac` | 8BE5 | 1024 | 1024 | Not tested (MAC register access) |
| `rf` | 8BF3 | 1024 | 1024 | Not tested (RF register access) |
| `get_wsc_profile` | 8BF2 | 1024 | 1024 | Not tested |
| `get_ba_table` | 8BF6 | 1024 | 1024 | Not tested |
| `stat` | 8BE9 | 1024 | 1024 | **Works** — radio statistics |
| `rd` | 8BF7 | 1024 | 1024 | Not tested |
| `rx` | 8BFB | 1024 | 1024 | Not tested |

### `get_driverinfo` Result
```
Driver version: 7.6.6.1
FW ver: 0x8a00, HW ver: 0x2080000, CHIP ID: 0x7981
```
**Chip: MediaTek MT7981 (MT7981B)**, Driver 7.6.6.1, Firmware 0x8a00.

### `stat` Result (rai0 — 2.4 GHz)
```
CurrentTemperature              = 33
Tx success                      = 57924
Tx fail count                   = 3270, PER=5.3%
Rx success                      = 93775
Rx with CRC                     = 139368, PER=59.7%
Rssi: -53 -53 -109 -109
Last TX Rate                    = MCS7, BW20, 0.8us GI, HT_MM, LDPC
Last RX Rate                    = NSS1_MCS8, BW20, LGI, VHT, BCC
```

### `stat` Result (rax0 — 5 GHz)
```
CurrentTemperature              = 35
Tx success                      = 0
Rx success                      = 18580
Rx with CRC                     = 104004, PER=84.8%
Rssi: 0 0 0 0
```

### `get_site_survey` Result
Returns a list of nearby APs (111 found):
```
No  Ch  SSID                             BSSID               Security               Siganl(%)  W-Mode      ExtCH
0   1   Juliana                          64:61:40:41:e0:e0   WPA2PSK/AES            13         11b/g/n     NONE
1   1   Rafaella                         18:81:ed:6a:da:8c   WPA2PSKWPA3PSK/AES     0          11b/g/n/ax  NONE
...
```

### `get_mac_table` Crash Analysis

**Status**: Segfaults (exit code 139) on all AP interfaces (rai0, rax0).

**Root cause**: The `iwpriv` binary (wireless-tools) calls the MediaTek private
ioctl `get_mac_table` (0x8BEF). The driver returns a binary structure in the
response buffer. The `iwpriv` binary attempts to print this as a string,
encounters non-null-terminated data or invalid memory access, and segfaults.

This is a known incompatibility between the wireless-tools `iwpriv` and
MediaTek's proprietary driver response format. The MediaTek SDK typically
includes a custom `iwpriv` or uses direct ioctl calls with proper buffer
handling.

**Router stability**: The router remains fully operational after the crash.
The crash is in the `iwpriv` userspace process, not in the kernel driver. A
core dump is generated at `/var/core-iwpriv` (2832 bytes).

**Recommendation**: Do NOT use `iwpriv get_mac_table`. Use the GTPR API
(`DEV2_WIFI_APDEV_ASSOCDEV` OID via `gl` operation) instead, which provides
the same station data in a structured JSON format.

## HAL Smoke Test Summary

| Test | Result | Notes |
|------|--------|-------|
| Device nodes | NOT AVAILABLE | No `/dev/wifi*` nodes |
| cfg80211/nl80211 | NOT AVAILABLE | No `/sys/class/ieee80211/` |
| `/proc/net/wireless` | PARTIALLY PROVEN | Noise floor only, no per-station |
| `iwpriv get_driverinfo` | PROVEN | Driver 7.6.6.1, MT7981 |
| `iwpriv stat` | PROVEN | Radio stats, RSSI, TX/RX rates, temperature |
| `iwpriv get_site_survey` | PROVEN | Nearby AP scan (111 APs) |
| `iwpriv get_mac_table` | NOT AVAILABLE | Segfaults (iwpriv/driver incompatibility) |
| GTPR `DEV2_WIFI_APDEV_ASSOCDEV` | PROVEN | Full station data via API |

## Conclusion

The MediaTek MT7981 Wi-Fi driver does not expose standard Linux wireless
interfaces (cfg80211/nl80211, `/sys/class/ieee80211/`). Wi-Fi data is
accessible through:

1. **MediaTek private ioctls** (`iwpriv`) — works for `stat`, `get_driverinfo`,
   `get_site_survey`, but `get_mac_table` crashes due to wireless-tools
   incompatibility.
2. **GTPR API** (`DEV2_WIFI_APDEV_ASSOCDEV` OID) — provides complete station
   data (MAC, RSSI, rates, noise, standard, hostname, IP) in structured JSON.

The GTPR API is the recommended mechanism for Detectic Wi-Fi observations, as
it is stable, structured, and does not require direct ioctl interaction with
the proprietary driver.

**HAL Classification: PARTIALLY PROVEN** (radio stats via iwpriv, station data
via GTPR API; no direct HAL/device-node access)
