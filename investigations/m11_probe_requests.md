# M11 — Probe-Request & Unassociated Device Visibility

**Status:** OFF-ROUTER ONLY — no experiments on the EX520.
**Date:** 2026-08-23
**Scope:** Determine whether Detectic can observe Wi-Fi devices that are **not** associated with the EX520V AP, using existing local evidence only.

---

## 1. Summary

| Capability | Evidence level | Source |
|---|---|---|
| Associated stations (MAC, RSSI, rates) | ✅ Confirmed | GTPR API `DEV2_WIFI_APDEV_ASSOCDEV` |
| Nearby APs (site survey) | ✅ Confirmed | `iwpriv get_site_survey` |
| Unassociated stations (probe requests) | ⛔ Confirmed NOT available | All local evidence |
| Passive RF / monitor mode | ⚠️ Theoretically possible, not validated | Driver-level investigation |

---

## 2. What Evidence Exists

### 2.1 GTPR API coverage (`investigations/ex520v_api_findings.md`)

The GTPR API was exhaustively mapped via Chrome DevTools against the live router. Relevant OIDs:

| OID | Method | Result on EX520V |
|---|---|---|
| `DEV2_WIFI_APDEV_ASSOCDEV` | `getList` | ✅ Returns associated stations (MAC, RSSI, rates, hostname, IP, assoc time) |
| `DEV2_HOST_ENTRY` | `getList` | ✅ Returns LAN host table (Wi-Fi and Ethernet) |
| `DEV2_WIFI_DE_UNASSOCSTA` | `getList` | ⛔ Returns error 9003 — **not implemented** on stock firmware |
| `DEV2_DHCPV4_CLIENT` | `getList` | ⚠️ Returns only the WAN-side DHCP client (the router itself), not LAN leases |

### 2.2 Wi-Fi capability inventory (`investigations/m4_4_wifi_capability.md`)

The Wi-Fi hardware is a MediaTek MT7981B with dual-band 802.11ax. Available tools and data sources:

| Tool / Source | Path | Provides unassociated visibility? |
|---|---|---|
| GTPR API | HTTP | Only associated stations |
| `iwpriv stat` | `/usr/sbin/iwpriv` | Radio stats (temperature, TX/RX counts, per-antenna RSSI) — no per-station data |
| `iwpriv get_site_survey` | `/usr/sbin/iwpriv` | Nearby APs (AP-to-AP scan), not probe requests |
| `/proc/net/wireless` | procfs | Per-interface signal/noise — no station enumeration |
| `iwconfig` | `/usr/sbin/iwconfig` | Interface configuration only |
| `iwlist` | `/usr/sbin/iwlist` | Scan not supported by driver |
| `wlNetlinkTool` | `/bin/wlNetlinkTool` | Receives events but **does not expose them via API** |

**Tools NOT available on the stock firmware:**
- `iw` (cfg80211/nl80211) — not installed
- `hostapd_cli` — not installed
- `tcpdump` — not installed
- `wpa_cli` — not installed

### 2.3 HAL binary analysis (`investigations/libplatform_api/ANALYSIS.txt`)

The MediaTek HAL library (`libplatform_api.so`) was statically analyzed. Key findings:

- `getAssociateStaList` (OID 0x0a01) — returns associated stations ✅
- `getScanResult` (OID 0x0b04) — returns scan results (APs) ✅
- `getUnassocStaLinkMetrics` (OID 0x0a03) — present in HAL but **not wired** to GTPR DataElement `DEV2_WIFI_DE_UNASSOCSTA`
- `getRssi` (OID 0x0b05) — per-station RSSI conversion (RCPI → dBm)

The HAL function `getUnassocStaLinkMetrics` exists in the binary, but:

> `rsl_getDev2WifiRadioObj` is real (537 instructions) and can return radio-level statistics, but does not enumerate stations. No RSSI conversion formula for unassociated devices is found.

### 2.4 Kernel driver and nl80211/cfg80211

The MediaTek MT7981 driver in the stock TP-Link firmware is a **proprietary driver**, not the upstream `mt76` driver. Consequences:

- **cfg80211/nl80211**: The kernel module may or may not register with cfg80211. Without `iw` (nl80211 tool) installed, we cannot test this off-router.
- **Monitor mode**: The driver exposes `rai0` (2.4 GHz) and `rax0` (5 GHz) as AP-mode interfaces. Monitor mode (`type monitor`) may be supported by the MT7981 driver, but requires:
  1. Root shell access (not currently available via GTPR)
  2. An `iw` binary or direct nl80211 socket programming
  3. A driver that supports monitor mode (not guaranteed on consumer TP-Link firmware)
- **Management frame capture**: Requires monitor mode or a driver that supports `RX_FRAME` filtering via nl80211. This cannot be validated without shell access.

### 2.5 `get_mac_table` crash investigation

`iwpriv rai0 get_mac_table` segfaults due to wireless-tools incompatibility with MediaTek's binary response format. This affects **associated station** enumeration, not probe requests. The GTPR API provides the same associated-station data without crashing.

---

## 3. What Is Confirmed

1. **Associated stations are fully observable** via GTPR — MAC, RSSI (RCPI 100–110), TX/RX rates, signal level, hostname, IP, association time, operating standard.

2. **Nearby APs are observable** via `iwpriv get_site_survey` — SSID, BSSID, channel, signal %, security, W-Mode. These are **access points**, not client devices.

3. **Probe requests are NOT observable** on the stock EX520V firmware:
   - The GTPR OID `DEV2_WIFI_DE_UNASSOCSTA` returns error 9003 (not implemented).
   - No probe-request log exists in the firmware filesystem image.
   - No `hostapd_cli` or `wpa_supplicant` control interface is available.
   - `iwlist scan` (which could probe for nearby networks) is not supported by the driver.

4. **The HAL function `getUnassocStaLinkMetrics` exists** but is not exposed through the GTPR API. Even if it were, it is a link-metrics function (likely returning airtime/ESSID data), not a probe-request sniffer.

---

## 4. What Is Only Theoretically Possible

### 4.1 nl80211 monitor mode

If the MediaTek kernel driver registers with cfg80211 and supports monitor mode:

```bash
iw dev rai0 interface add mon0 type monitor
iw dev mon0 set channel 6
```

This would allow capturing:
- Probe request frames (`mgmt:0x40`)
- Probe response frames (`mgmt:0x50`)
- Authentication/association frames
- Beacon frames from non-associated APs

**Requirements:**
- `iw` binary (not installed; would need to be added to `/tmp` or a writable partition)
- Kernel cfg80211 support in the MediaTek driver
- Root privileges (requires shell access — not yet obtained)
- The driver must allow creating a monitor interface on an AP interface (not all drivers do)

### 4.2 Raw socket / packet socket sniffing

Using `AF_PACKET` sockets on the AP interface:
- Can capture received frames at the data link layer
- Would see probe requests if the driver passes them to the stack
- **Likely not possible** — the AP interface only sees frames destined for associated stations or broadcast; probe requests from non-associated devices may be handled entirely in hardware/firmware

### 4.3 HAL-level access

If the root shell is obtained, the HAL function `getUnassocStaLinkMetrics` (OID 0x0a03) might return some data via the ioctl interface. However:
- This is a "link metrics" function, not a "station list" function
- It may require a specific device to already be in range
- The exact data structure returned is undocumented

### 4.4 Custom firmware (OpenWrt)

Installing OpenWrt would provide:
- `iw` (nl80211) tool
- `hostapd` with probe-request logging
- Full cfg80211 access
- Monitor mode support

**This violates the project constraint: "Avoid replacing firmware with OpenWrt" and "Keep the original TP-Link firmware untouched."**

---

## 5. What Would Require Router-Side Validation

Any validation of the theoretical possibilities in §4 requires:

1. **Shell access** to the EX520V — not yet obtained (SSH disabled, Telnet disabled, no known debug interface)
2. **Root privileges** — required to create monitor interfaces, open raw sockets, or call HAL functions
3. **Testing `iw dev rai0 interface add mon0 type monitor`** — would fail if the driver doesn't support it
4. **Testing nl80211 socket connection** — `iw` is not installed; could use Python with `pyroute2` if Python is available (it is not on the stock firmware)
5. **Checking driver capabilities** — would need `/sys/kernel/debug/ieee80211/phy*/` or `iw dev` output

All of these are **router-side operations** and are explicitly excluded by the production constraint.

---

## 6. What Would Violate the "Untouched Firmware" Requirement

| Action | Violates constraint? | Reason |
|---|---|---|
| Install OpenWrt | ✅ Yes | Replaces firmware entirely |
| Modify kernel modules | ✅ Yes | Touches firmware partition |
| Enable `iw`/nl80211 tools | ⚠️ Borderline | Would require writing to read-only filesystem or squashfs overlay |
| Enable monitor mode via `iw` | ⚠️ Borderline | Requires installing `iw` binary; monitor mode itself does not modify firmware |
| Call HAL ioctls from userspace | ⚠️ Borderline | Only if done from the router's own shell; no firmware modification |
| Patch `/etc/shadow` to enable root | ❌ No (but unsafe) | Modifies configuration, not firmware — still violates "minimal changes" principle |

---

## 7. Recommended Safest Experiment for a Future Maintenance Window

The investigation sequence (per AGENTS.md §7) for probe-request visibility during a maintenance window:

### Step 1: Confirm shell access already exists
No new action if shell is already available.

### Step 2: Check for `iw` or `iwlist`
```bash
which iw iwlist iwconfig iwpriv
```
If `iw` is absent but `iwpriv` exists, try the proprietary MediaTek ioctl:
```bash
iwpriv rai0 get_site_survey
```
This provides AP scan data only, not probe requests.

### Step 3: Attempt monitor interface creation (non-destructive)
```bash
# Check if the driver supports monitor mode
iw dev rai0 interface add mon0 type monitor 2>&1
# If successful, check what comes through
# If it fails with "Operation not supported", monitor mode is unavailable
# If it fails with "File exists" or "Device busy", the driver may need AP interface down first
```

**Reversibility:** If this fails, simply run `iw dev mon0 del` to clean up. No filesystem changes.

### Step 4: Check `/proc/net/pf_trace` or driver debugfs
Some MediaTek drivers expose station counters via debugfs. Check:
```bash
ls /sys/kernel/debug/ieee80211/phy*/  # may not exist with proprietary driver
cat /proc/net/wireless
```

### Step 5: Check for management frame logs
```bash
# Check if hostapd or equivalent logs probe requests
find / -name "hostapd*" -o -name "wpa_supplicant*" 2>/dev/null
# Check syslog
dmesg | grep -i "probe\|mgmt\|frame"
```

### Step 6: HAL ioctl probe (if shell access and root are confirmed)
If the HAL library is loadable and the ioctl interface is accessible:
```bash
# This would require a custom binary or script
# Test OID 0x0a03 (getUnassocStaLinkMetrics)
```

**Critical safety note:** All experiments in Steps 2–6 should run on a **backup unit**, not the production EX520V. The production unit should only be used once findings are validated on identical hardware.

---

## 8. Impact on Detectic Architecture

The `DriverProvider` trait already accounts for this limitation:

```rust
DriverCapability::UnassociatedStations => ProviderValue::Unavailable,
```

The `MediaTekHalProvider::capability_matrix()` explicitly reports unassociated stations as `Unavailable`. The `ProbeObservation` struct exists in the model but is always empty on the stock EX520V.

**Design implication:** Detectic must gracefully degrade. The pipeline should:

1. Report `probe` source events only when probe-request capture is confirmed available.
2. The `RealtimeEventKind::DeviceNearby` variant exists for this purpose.
3. The `DriverCapability` enum should remain extensible for future hardware platforms that *do* support probe capture.

The current architecture handles this correctly — probe observations are processed through the same `ingest()` pipeline and are simply empty on the EX520V. No code changes are needed to support future probe-request-capable platforms; the existing `ProbeObservation` type and `RealtimeEventKind::DeviceNearby` with `source: "probe"` already cover the path.

---

## 9. Conclusion

On the stock TP-Link EX520V firmware:

- **Associated devices**: Fully observable via GTPR API ✅
- **Nearby APs (site survey)**: Observable via `iwpriv get_site_survey` ✅
- **Probe requests from unassociated devices**: **Not observable** — no API, no tool, no log source exposes them ⛔

Probe-request visibility would require either:
1. A monitor-mode capable driver with nl80211/cfg80211 support (unvalidated, requires shell + `iw`)
2. Custom firmware (violates project constraints)

The Detectic sensor should ship with probe-request support **conditionally enabled** — when the driver/provider reports `UnassociatedStations` as `Available`. On the EX520V, this will remain `Unavailable`, and Detectic will rely on associated-station RSSI for proximity estimation.
