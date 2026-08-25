# Detectic Research — Non-associated Wi-Fi device detection on TP-Link EX520V

> **Question:** Can the stock TP-Link EX520V detect a nearby Wi-Fi device that is **not associated** to it, obtain its RSSI, and estimate distance, using only the existing firmware and the GTPR/GDPR web API?
>
> **Scope:** Read-only investigation. No shell access, no UART, no firmware modification, no disruptive Wi-Fi operations. Sources: extracted rootfs `_rootfs/`, firmware binary, web UI assets, GTPR client code, public MediaTek/OpenWrt documentation.

---

## 1. Executive summary

| Capability | Verdict | Evidence |
|---|---|---|
| Detect non-associated clients via GTPR/GDPR API | **NOT supported** in this build | Relevant `rsl_get*` handlers in `libcmm.so` are stubs returning `0x3e8` (1000) or empty `0`; build flags disable wireless survey and advanced scan. |
| Detect non-associated clients via EasyMesh / 1905.1 multi-AP protocol | **PARTIALLY implemented at library level, not usable standalone** | `libtp1905.so` and `libplatform_api.so` contain unassociated-STA link metrics, channel scan, and neighbor BSS code, but it is designed for controller/agent traffic **between** EasyMesh nodes. A standalone router has no peer to query or to be queried by. |
| Detect non-associated clients via monitor mode / `tcpdump` | **Theoretically possible if shell access were available, not exposed via API** | MT7981/MT76 family supports monitor mode, but the stock image lacks `iw`/`iwinfo`/`hostapd_cli`; the only wireless tool is `iwconfig`. Enabling monitor mode would require shell and would likely disrupt the AP. |
| Obtain RSSI for non-associated clients | **Not accessible without shell or EasyMesh controller** | RSSI of probe requests is used internally by `nrd` for band steering (`Delay24GProbeRSSIThreshold`), but the value is not surfaced through any readable API. |
| Estimate distance from RSSI | **Not applicable** | Distance estimation is a backend math problem, but it requires an RSSI input the router cannot provide for non-associated clients via the API. |

**Bottom line:** The stock EX520V firmware *does not* expose non-associated Wi-Fi devices through the same GTPR/GDPR API that already provides the associated-device network map. The only ways to get this data require capabilities the stock image does not expose (shell + monitor mode / private `iwpriv` / EasyMesh controller with other APs).

---

## 2. Methods

1. **Static analysis of the extracted rootfs** (`_rootfs/`):
   - Enumerated binaries, libraries, and web UI assets.
   - Searched for OIDs, build flags, and wireless tools.
2. **Disassembly of `libcmm.so`**, the data-model handler library:
   - Used `nm -D` to list exported `rsl_get*` handlers.
   - Used `llvm-objdump -d` to distinguish real handlers from `return 0x3e8` / `return 0` stubs.
3. **String and symbol analysis** of `libtp1905.so`, `libplatform_api.so`, `libmapShared.so`, `mapController`, `mapAgent`, `meshMonitor`, `nrd`, and `wlNetlinkTool`.
4. **Web UI review**: inspected `web/js/oid_str.js`, `gdprProxy.js`, `easyMesh.htm`, `wlScan_*.htm`, and `wirelessHost.htm`.
5. **Public documentation** for the MT7981 / MT76 / nl80211 wireless stack.
6. **Prototype**: wrote a read-only Python probe that exercises the candidate OIDs against a live router.

---

## 3. GTPR/GDPR data model — what exists and what is alive

### 3.1 Candidate OIDs for unassociated / neighbor / scan data

The following OIDs are defined in the web UI registry <ref_file file="/home/soporte24hwww/Documentos/Repositorios/detectic/_rootfs/web/js/oid_str.js" />:

| OID | Meaning | `MAX_INST` |
|---|---|---|
| `DEV2_WIFI_DE_UNASSOCSTA` | DataElements `UnassociatedSTA` | 256 |
| `DEV2_WIFI_NEIGHBORWIFI` | Neighboring Wi-Fi diagnostic (site survey) | — |
| `DEV2_WIFI_DE_SCAN_RESULT` | EasyMesh channel-scan container | 256 |
| `DEV2_WIFI_DE_OPCLASS_SCAN` | EasyMesh operating-class scan | 256 |
| `DEV2_WIFI_DE_CHANNEL_SCAN` | EasyMesh per-channel scan | 256 |
| `DEV2_WIFI_DE_NEIGHBORBSS` | Neighboring BSSs found during scan | 256 |
| `DEV2_WIFI_APDEV_NEIGHBORSIG` | Neighbor signal strength (CustomTopo) | — |
| `DEV2_X_TP_ONBOARDBYSCANNING` | EasyMesh onboarding-by-scanning state | — |
| `DEV2_WIFI_RADIO` / `WIFI_RADIO_STATS` | Radio objects and statistics | 2 |
| `DEV2_WIFI_APDEV_RADIO` | AP-Device radio object | 32 |
| `DEV2_WIFI_DIAGNOSTICRESULT` | Wi-Fi diagnostic result | — |

These look like exactly the objects that could report non-associated transmitters. However, the *handler* code in `libcmm.so` tells a different story.

### 3.2 Handler status from `libcmm.so`

`libcmm.so` exports `rsl_getDev2*` functions for every data-model object. Using `llvm-objdump` we can see that most of the objects above are stubs.

A typical **live** handler, such as `rsl_getDev2WifiApdevAssocdevObj` (the one already used by Detectic), contains hundreds of instructions and fills a data buffer.

A typical **stub** handler, such as `rsl_getDev2WifiDeUnassocstaObj`, is only 7 instructions and simply returns `0x3e8` (decimal 1000):

```
0000000000189724 <rsl_getDev2WifiDeUnassocstaObj>:
  189724: d10083ff      sub sp, sp, #0x20
  189728: b9001fe0      str w0, [sp, #0x1c]
  18972c: f9000be1      str x1, [sp, #0x10]
  189730: f90007e2      str x2, [sp, #0x8]
  189734: 52807d00      mov w0, #0x3e8              // =1000
  189738: 910083ff      add sp, sp, #0x20
  18973c: d65f03c0      ret
```

The same 7-instruction `return 1000` stub applies to **all** of the DataElements objects: `ScanResult`, `OpClassScan`, `ChannelScan`, `NeighborBSS`, `UnassocSTA`, `BSS`, `STA`, `BackhaulSta`, etc. <ref_file file="/home/soporte24hwww/Documentos/Repositorios/detectic/_rootfs/lib/libcmm.so" />

`rsl_getDev2WifiNeighborwifiObj` is also a stub that returns 1000 (22 instructions, but the return path is identical).

`rsl_getDev2WifiApdevRadioObj` returns `0` in 7 instructions, i.e. an empty/success-with-no-data handler. `rsl_getDev2WifiRadioObj` is real (537 instructions) and can return radio-level statistics, but does not enumerate stations.

**Interpretation:** The data model *schema* contains placeholders for EasyMesh / DataElements / neighbor data, but the actual `get` handlers are not implemented in this firmware build. A `gl` call to `DEV2_WIFI_DE_UNASSOCSTA` or `DEV2_WIFI_NEIGHBORWIFI` will almost certainly return an error or an empty result.

### 3.3 Build flags that disable the relevant UI

In `web/js/oid_str.js` <ref_file file="/home/soporte24hwww/Documentos/Repositorios/detectic/_rootfs/web/js/oid_str.js" />:

```javascript
var INCLUDE_WIRELESS_SURVEY=0;
var INCLUDE_ADV_WIFI_SCAN=0;
var INCLUDE_APPS_MONITOR=0;
var INCLUDE_BETA_SIGNAL=0;
```

These flags confirm that the site-survey / advanced-scan / monitoring paths are compiled out of the web UI in this build. Only `INCLUDE_EASYMESH_TP_ONBOARDING_WEB_SCAN=1` is enabled, and that is used for adding another EasyMesh AP (not for listing arbitrary client MACs).

---

## 4. EasyMesh / 1905.1 multi-AP — what the libraries can do

### 4.1 Relevant `.so` symbols

`libtp1905.so` contains the IEEE 1905.1 / Multi-AP protocol stack:

```
tp1905_sendUnAssociatedSTALinkMetricsQueryPacket
tp1905_handleUnAssociatedSTALinkMetricsQueryTLV
tp1905_handleUnAssociatedSTALinkMetricsResponseTLV
_CHANNEL_SCAN_RESULT_
_UNASSOCIATED_STA_LINK_METRICS_QUERY_
_UNASSOCIATED_STA_LINK_METRICS_RESPONSE_
```

`libplatform_api.so` contains the vendor HAL that talks to the MediaTek driver:

```
hal_multiap_mtk_doScan
hal_multiap_mtk_getScanResult
hal_multiap_mtk_getUnassocStaLinkMetrics
hal_multiap_mtk_setUnassocLinkMetrics
hal_multiap_mtk_getRssi
```

`mapController` and `mapAgent` link against these libraries and expose strings such as:

```
mapController_applySettingUnassocStalinkMetricsRsp
multiap_get_unassoc_sta_link_metrics
multiap_get_scan_result
```

### 4.2 What this means in practice

`nrd` (the neighbor/steering daemon) contains probe-threshold strings such as:

```
Delay24GProbeRSSIThreshold
Delay24GProbeTimeWindow
Delay24GProbeMinReqCount
%s: %02X:... isn't associated, no need to monitor.
```

This proves the firmware **does** see probe requests from non-associated devices and can measure their RSSI, but it uses that information internally for **band steering / client steering**, not for external reporting.

The EasyMesh unassociated-STA link-metrics flow is designed as a *request/response between a controller and an agent*:

1. Controller sends `UnassociatedSTALinkMetricsQuery` (specifying STA MAC and channels).
2. Agent receives the query, asks the radio to measure, and replies with `UnassociatedSTALinkMetricsResponse` containing RSSI.

In a standalone EX520V there is no external controller to send the query and no external agent to ask. The router can act as controller (`mapController`) and as agent (`mapAgent`), but the 1905.1 protocol would need another EasyMesh device on the network before any query/response traffic is generated. Therefore these capabilities are **not usable for Detectic on a single stock router**.

---

## 5. Available wireless tools in the stock image

The rootfs contains:

| Tool / binary | Path | Notes |
|---|---|---|
| `busybox` | `/bin/busybox` | Includes many applets, but not `iw`/`iwpriv`. |
| `tcpdump` | `/usr/sbin/tcpdump` | Can capture frames, but only useful with shell + monitor-capable interface. |
| `iwconfig` | `/usr/sbin/iwconfig` | Legacy wireless-tools; shows SSID, mode, etc. Not `station dump` or scan. |
| `dropbearmulti` | `/usr/bin/dropbearmulti` | SSH binary present; no evidence it is reachable via the API in this build. |
| `wlNetlinkTool` | `/bin/wlNetlinkTool` | Listens to wireless netlink events for WPS, not for scan data. |
| `nrd` | `/bin/nrd` | Neighbor / steering daemon; sees probes internally. |
| `mapController` / `mapAgent` / `meshMonitor` | `/bin/` | EasyMesh daemons; require other EasyMesh nodes. |
| `tp1905cliC` | `/bin/tp1905cliC` | 1905.1 CLI; requires shell. |

Notably **missing** from the SquashFS image:

- `iw`
- `iwinfo`
- `hostapd` / `hostapd_cli`
- `iwpriv`
- `wpa_supplicant`

Without `iw` or `iwpriv` there is no standard userspace path to request a scan, dump station info, or create a monitor interface. The router instead uses a proprietary MediaTek abstraction (`mtkwifi` Lua module / `libplatform_api.so`) for Wi-Fi control.

---

## 6. MT7981 / MT76 wireless capabilities

The EX520V is based on the MediaTek MT7981 SoC (Filogic 630) with an MT7976 DBDC radio frontend. The Linux driver is the in-kernel `mt76` family (`mt7915e` / `mt798x`).

- `nl80211` scan and station dump are supported by the driver in general.
- Monitor mode is supported, but with caveats (deep-sleep / runtime-pm need to be disabled on some chipsets, and capturing beacons/probes reliably may require a `sniffer` firmware configuration).
- The **stock firmware does not expose these capabilities** to the administrator through the web API or installed tools.

In other words: the *silicon* can do it, the *shipping firmware image* does not provide a read-only, non-disruptive way to access it.

---

## 7. Smallest possible prototype

To give the project a concrete, repeatable test without modifying the router, a read-only Python probe was added:

<ref_file file="/home/soporte24hwww/Documentos/Repositorios/detectic/python/probe_unassociated.py" />

It reuses the existing `GtprClient` from `python/detectic_client.py` to:

1. Authenticate via `getGDPRParm` / `cgi_gdpr?9`.
2. Send `operation: "gl"` (getList) for all of the candidate OIDs listed in section 3.1.
3. Record each raw, decrypted response, parse it, and print a one-line summary.
4. Write a JSON file with the complete results.

Usage:

```bash
export DETECTIC_PASSWORD="your-web-password"
python3 python/probe_unassociated.py --url http://192.168.0.1 --user admin
```

The script **does not** start scans, write configuration, or reboot the device. If any of the OIDs unexpectedly returns real data on a different firmware revision, the script will capture it.

---

## 8. Classification table

| Mechanism | Status for Detectic on stock EX520V | Why |
|---|---|---|
| GTPR `DEV2_WIFI_APDEV_ASSOCDEV` | **SUPPORTED** | Already used; returns associated clients with RSSI, MAC, hostname, IP, etc. |
| GTPR `DEV2_WIFI_DE_UNASSOCSTA` | **NOT SUPPORTED** | Handler in `libcmm.so` is a `return 1000` stub. |
| GTPR `DEV2_WIFI_NEIGHBORWIFI` | **NOT SUPPORTED** | Stub in `libcmm.so`. |
| GTPR `DEV2_WIFI_DE_SCAN_RESULT` / `*_SCAN` / `*_NEIGHBORBSS` | **NOT SUPPORTED** | All DataElements get handlers are stubs. |
| GTPR `DEV2_WIFI_APDEV_NEIGHBORSIG` | **NOT SUPPORTED** | No dedicated handler; parent object returns empty. |
| GTPR `DEV2_WIFI_RADIO` / `WIFI_RADIO_STATS` | **PARTIALLY SUPPORTED** | `rsl_getDev2WifiRadioObj` is real; may return radio stats but no client list. |
| Web UI site survey (`wlScan.htm`) | **DISABLED** | `INCLUDE_WIRELESS_SURVEY=0`; legacy htm uses a `LAN_WLAN_BSSDESC_ENTRY` constant not present in the OID registry. |
| EasyMesh onboarding scan | **PARTIALLY** | Finds other EasyMesh APs, not arbitrary client MACs. |
| EasyMesh unassociated-STA link metrics | **NOT USABLE STANDALONE** | Requires an external EasyMesh controller/agent. |
| `nrd` probe-RSSI thresholds | **INTERNAL ONLY** | Firmware sees probe RSSI for steering, but does not expose it. |
| Monitor mode + `tcpdump` | **REQUIRES SHELL / NOT EXPOSED** | Possible on MT7981, but no `iw`/`iwpriv` and would disrupt the AP. |

---

## 9. Direct answer to the research question

> **Can the EX520 detect a non-associated Wi-Fi device via existing, read-only, non-disruptive mechanisms?**

**No, not on the stock firmware through the GTPR/GDPR API.**

The router can see associated clients (`DEV2_WIFI_APDEV_ASSOCDEV`), and the silicon/firmware can see non-associated probe requests, but the latter information is kept inside the EasyMesh/steering daemons and is not exported through any object the web API can read. All of the relevant data-model handlers are unimplemented stubs in this build.

---

## 10. Recommendations

1. **Run `python/probe_unassociated.py` against the live EX520V** to confirm the static findings empirically. This is the fastest, safest way to be sure no OIDs return unexpected data on the current firmware.

2. **If the project must detect non-associated devices, the path requires shell access** (which is currently out of scope for the stock, read-only investigation). With shell, the options would be, in order of preference:
   - Check whether `iwpriv` or `mtkwifi` Lua calls can query scan/probe data without monitor mode.
   - Use the MediaTek `ated_tp` or `mapController`/`mapAgent` tooling to read scan results / unassociated-STA metrics.
   - Create a monitor VIF (if the driver/firmware permits it on the same phy) and run `tcpdump`.

3. **Do not rely on EasyMesh DataElements OIDs in the GTPR client.** They are not populated in this build and would only add failed requests / latency.

4. **Continue the existing associated-device sensor path** (events, persistence, upload) because it is the only observation source currently verified to work via the web API.

5. **If future firmware revisions enable the DataElements handlers**, the same `probe_unassociated.py` script and the event pipeline in `src/events.rs` can be extended to consume `DEV2_WIFI_DE_UNASSOCSTA` / `DEV2_WIFI_DE_NEIGHBORBSS` entries and emit privacy-safe `DEVICE_PROBED` / `DEVICE_NEARBY` events.

---

## 11. Artifacts produced / modified

- `python/probe_unassociated.py` — new read-only GTPR probe script.
- This report.

No router configuration was changed; no firmware was modified; no destructive operations were performed.
