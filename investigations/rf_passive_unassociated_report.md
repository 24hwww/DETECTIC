# DETECTIC — EX520 Passive RF / Unassociated Device Detection Research Report

> **Date:** 2026-08-25  
> **Scope:** TP-Link EX520V, MediaTek MT7981B + MT7976CN DBDC, stock firmware (unmodified)  
> **Question:** Can the EX520 detect, characterize and track Wi-Fi devices that are NOT associated with it, using only the existing hardware/firmware and without disrupting normal router operation?

---

## A. Executive Conclusion

**Answer to the primary research question:**

> *Can DETECTIC realistically detect Wi-Fi devices that have never connected to the EX520?*

**NO-GO for the EX520 alone. GO for an external, unmodified Wi-Fi sniffer.**

- **GTPR/web API alone: NO.** The stock TP-Link GTPR/GDPR data model does not expose unassociated stations (`DEV2_WIFI_DE_UNASSOCSTA` returns error `9003`; all DataElements handlers are unimplemented stubs).
- **Shell access + additional software on the EX520: NO for stock firmware.** The EX520V uses a **proprietary MediaTek Wi-Fi driver** (version 7.6.6.1) that does **not** register with `cfg80211`/`nl80211`. There is no `/sys/class/ieee80211/`, no `iw` tool, and no standard Linux monitor VIF path. All wireless control is through private `iwpriv` ioctls. The available `iwpriv` commands (`stat`, `get_site_survey`, `get_driverinfo`) do not expose probe requests or raw frame capture. Creating a monitor VIF via `iw` is impossible.
- **MediaTek low-level `iwpriv` ioctls (`bbp`, `mac`, `rf`, `rx`, `rd`): THEORETICAL / DANGEROUS.** These give access to baseband/MAC/RF registers, but they are undocumented, could crash the wireless subsystem, and are not a structured frame-capture interface. They are not a safe or practical path for DETECTIC.
- **EasyMesh 1905.1 unassociated-STA link metrics: NOT USABLE STANDALONE.** The libraries and daemons exist (`libtp1905.so`, `mapController`, `mapAgent`, `nrd`), but the protocol is request/response between a controller and an agent. A standalone EX520 has no peer to query and is not triggered to report unassociated-STA metrics through the web API.
- **CSI, per-chain I/Q, beamforming matrices, direction estimation: NO on stock firmware.** These are computed in the PHY/firmware but are not transferred to host memory through any supported userspace interface on this firmware.
- **Distance/proximity estimation: NO for unassociated devices on EX520 alone.** No RSSI source exists for non-associated transmitters. It is feasible only with an external monitor sensor.

**Confidence level for unassociated Probe-Request detection via EX520 + additional software:** **Low** (proprietary driver lacks cfg80211 and frame-capture interface).  
**Confidence level for unassociated Probe-Request detection via external hardware:** **High** (any dual-band monitor-capable USB Wi-Fi adapter or OpenWrt SBC will work).

---

## B. Signal Inventory

| Signal | EX520 availability | Unassociated devices | Distance usefulness | Identity usefulness | Risk |
| --- | --- | --- | --- | --- | --- |
| Probe Request | **NOT via API; NOT on stock EX520; YES with external sniffer** | YES (with external monitor) | Medium (with RSSI calibration) | Medium (randomized MAC) | LOW with external sniffer; MEDIUM if low-level register probe |
| RSSI | YES (associated); NO for unassociated on EX520; YES with external sniffer | YES (per-frame in radiotap) | High | High (for proximity) | LOW |
| SNR | NO (not exposed) | NO | High | Low | — |
| Noise floor | YES (`/proc/net/wireless`, `iwpriv stat`) | N/A (per-phy) | High (baseline) | Low | LOW |
| Channel utilization | **NO** (driver does not report CCA/survey to userspace) | N/A | Medium | Low | — |
| Capabilities (IEs) | **NO via API; YES with external sniffer** | YES (HT/VHT/HE + vendor IEs) | Low | **Very High** (fingerprint) | LOW |
| HT/VHT/HE capabilities | **NO via API; YES with external sniffer** | YES | Low | **Very High** | LOW |
| MCS | NO via API (only last rate for associated) | YES per frame (radiotap) | Low | Medium | LOW |
| NSS | NO via API | YES per frame | Low | Medium | LOW |
| Antenna RSSI | **YES** for associated via `iwpriv stat`; NO per-probe on EX520; YES with external sniffer | YES per frame with external sniffer | High | Medium | LOW |
| CSI | **NO** | NO | Very High | High | — |
| Beamforming | **NO** (not exported) | NO | High | Low | — |
| Timing / periodicity | **NO via API; YES with external sniffer** | YES | Low | **High** (behavioral fingerprint) | LOW |
| RF baseline (noise/utilization) | **Partial** (noise only) | N/A | Medium | Low | LOW |

---

## C. Capability Classification

| Capability | Classification | Evidence |
| --- | --- | --- |
| Detect unassociated Wi-Fi via GTPR API | **IMPOSSIBLE on stock** | `DEV2_WIFI_DE_UNASSOCSTA` handler is a `return 0x3e8` stub; live test returns 9003. See `investigations/ex520_unassociated_detection_report.md` and `investigations/m11_probe_requests.md`. |
| Detect unassociated via `iwpriv`/`libplatform_api` | **IMPOSSIBLE on stock** | HAL function `getUnassocStaLinkMetrics` exists but is not wired to the web API and requires EasyMesh controller/agent traffic to be triggered. |
| Probe Request capture with monitor VIF | **NOT POSSIBLE on stock EX520; requires external hardware** | The EX520V uses a proprietary MediaTek driver with no cfg80211/nl80211. `/sys/class/ieee80211/` is absent. `iw` and `tcpdump` are not installed. `wlNetlinkTool` consumes netlink events internally. |
| RSSI per associated station | **CONFIRMED** | GTPR `ASSOCDEV` returns RCPI 0–127; converted to ~-86 dBm. See `investigations/rssi_semantics.md`. |
| Per-antenna RSSI | **CONFIRMED for associated; NOT for unassociated on EX520** | `iwpriv stat` shows four per-chain values for associated traffic. No probe-frame capture path exists. |
| Noise floor | **CONFIRMED** | `/proc/net/wireless` and `iwpriv stat` report noise. |
| Channel utilization / CCA / airtime | **NOT EXPOSED** | Not available via `iwpriv`, `iw` (missing) or `/proc`. mt76 driver collects it internally but does not export survey data in this firmware. |
| HT/VHT/HE IE extraction from probes | **POSSIBLE with external sniffer** | Standard 802.11 frame parsing once a probe is captured. |
| Randomized MAC correlation | **THEORETICAL / RESEARCH** | Requires feature extraction (IEs, timing, RSSI trajectory) and probabilistic clustering. Cannot be deterministic. |
| CSI | **REQUIRES FIRMWARE/DRIVER MODIFICATION or RESEARCH FIRMWARE** | mt76 upstream has no public CSI export for MT7981. MediaTek connac3 CSI exists for newer MT7996; not available for MT7981 without research patches. See web search results and ADR-266 references. |
| Beamforming / sounding | **HARDWARE SUPPORTED, NOT EXPORTED** | MT7981 datasheet lists explicit/implicit beamformer and beamformee. No userspace API to read matrices. |
| 2.4 GHz + 5 GHz dual-band observation (associated) | **CONFIRMED (DBDC)** | MT7976CN front-end with MT7981B; `rai0` and `rax0` AP interfaces active. |
| 2.4 GHz + 5 GHz dual-band observation (unassociated) | **NOT on EX520; POSSIBLE with external sensor** | Requires two external sniffer radios or one dual-band monitor adapter that can hop. |
| RF environment baseline (noise) | **PARTIAL** | Noise floor only; no CCA/channel utilization. |

---

## D. Best Achievable Architecture with EX520 Alone

The best achievable architecture **with the EX520 alone** is limited to associated-device and nearby-AP observations:

```text
EX520 GTPR API
   ↓
DEV2_WIFI_APDEV_ASSOCDEV (associated only)
DEV2_HOST_ENTRY (DHCP/LAN host)
iwpriv stat (radio stats, per-antenna RSSI)
iwpriv get_site_survey (nearby APs)
   ↓
Associated-station presence / proximity
Nearby-AP RF environment
No unassociated device detection
```

The realistic pipeline for **unassociated device detection** is to add an external, unmodified Wi-Fi monitor sensor:

```text
External USB Wi-Fi / OpenWrt SBC (monitor mode, mt76/ath9k/rtl8xxxu)
   ↓
Raw 802.11 frame capture (tcpdump / libpcap / raw socket)
   ↓
Probe Request / Beacon / Management frame filter
   ↓
Parse 802.11 header + Information Elements
   ↓
Pseudonymize source MAC (HMAC-SHA256 keyed by sensor secret)
   ↓
Feature extraction:
   - MAC prefix / OUI (for vendor guess)
   - RSSI (radiotap, RCPI/dBm)
   - per-antenna RSSI if present
   - HT/VHT/HE capabilities
   - supported rates
   - vendor-specific IEs
   - channel / band / frequency
   - timestamp / sequence number
   ↓
Observation cluster with candidate identity
   ↓
Temporal tracking (probed periodicity, burst patterns, inter-frame timing)
   ↓
Presence probability
   ↓
Proximity estimate (NEAR/MEDIUM/FAR from calibrated RSSI model)
   ↓
Detectic event envelope → backend
```

If the external sniffer is not available, the only alternative on the EX520 **without firmware modification** is:

```text
EX520 GTPR API
   ↓
DEV2_WIFI_APDEV_ASSOCDEV (associated only)
DEV2_HOST_ENTRY (DHCP/LAN host)
iwpriv stat (radio stats, per-antenna RSSI)
iwpriv get_site_survey (nearby APs)
   ↓
No unassociated device detection
```

---

## E. Minimum Viable RF Detection

The smallest signal set that would produce a useful first prototype for unassociated-device detection:

```text
Probe Request (mgmt subtype 0x04)
+
source MAC (pseudonymized)
+
timestamp
+
band (2.4 GHz / 5 GHz)
+
channel
+
RSSI (dBm or RCPI)
+
HT/VHT/HE capabilities (from IEs)
```

With this, DETECTIC can:

1. Detect that a Wi-Fi radio is physically nearby.
2. Build a candidate pseudonymous identity per randomized-MAC epoch.
3. Correlate observations over time using RSSI trajectory and timing.
4. Group probes that share the same device fingerprint (IEs + capabilities).
5. Estimate NEAR / MEDIUM / FAR from a calibrated RSSI model.

This is the **P0 implementation target** if the external-sensor GO is approved. It is **not achievable on the EX520 alone**.

---

## F. Advanced RF Detection

Additional signals that would improve identity correlation, proximity and movement:

| Capability | What it enables | Availability on EX520 |
| --- | --- | --- |
| Full 802.11 IE parsing | Device fingerprinting across randomized MACs | With monitor VIF |
| Per-antenna / per-chain RSSI | Better RSSI combining, coarse AoA | Likely with radiotap |
| SNR per frame | More stable distance estimate | Not confirmed |
| Channel utilization / CCA | RF environment baseline, movement anomaly | Not exposed |
| RSSI variance / trajectory | Movement classification (APPROACHING/STATIONARY/DEPARTING) | With monitor VIF |
| Dual-band correlation (same device on 2.4 and 5 GHz) | Stronger identity confidence | Requires dual monitor or band switching |
| CSI (per-subcarrier channel response) | Presence without active transmission, fine-grained movement | **Not available** |
| Beamforming feedback matrices | Direction / AoA, better ranging | **Not available** |
| PHY rate per probe | Device capability inference | With monitor VIF |

---

## G. CSI Feasibility Report

| Question | Answer |
| --- | --- |
| Hardware support | **YES** — MT7981B/MT7976 compute channel estimates in the baseband PHY for every received frame. |
| Driver support | **PARTIAL** — The upstream `mt76` driver parses RX descriptors and group-5/6 RX status, but does **not** export complex I/Q channel estimates to userspace for the MT7981/MT7915 family. |
| Firmware support | **NO for CSI export** — The stock TP-Link firmware bundles proprietary MediaTek wireless firmware (`mt7981_wm.bin`, `mt7981_wa.bin`) configured for normal AP operation, not for research/CSI mode. |
| Access method | None on stock firmware. The connac3 CSI feature exists in newer chips (MT7996, MT7925 research) with `MCU_UNI_CMD_CSI_CTRL`. It is not confirmed for the MT7981 build used by the EX520. |
| Unassociated-device applicability | Only for packets received from a transmitting device. Passive sensing of non-transmitting objects (human movement) is a different, more advanced problem requiring MIMO/sounding and research firmware. |
| Movement detection | **NOT ACHIEVABLE** with stock firmware; requires CSI export and signal processing. |
| Distance estimation | **NOT ACHIEVABLE** with stock firmware; CSI would help but does not solve multipath/without calibration. |
| Risk | Attempting to use debugfs / testmode / vendor nl80211 to extract PHY data on a production router with an active AP could crash the wireless subsystem. |

**CSI verdict for EX520 stock:** **NO-GO.** Do not plan around CSI. It would require driver/firmware research that conflicts with the "no firmware modification" constraint.

---

## H. External Sensor Fallback

If the EX520 cannot be made to expose unassociated probe requests safely, the minimum additional hardware is:

### H.1 Option A — USB Wi-Fi monitor adapter (lowest cost, lowest complexity)

```text
EX520 (routing, associated-device data)
   |
   +-- USB 2.0/3.0 port
       |
       +-- Linux-capable USB Wi-Fi adapter with monitor mode
           (e.g., MT7612U, RTL8812AU, Atheros AR9271)
           |
           +-- runs tcpdump / airodump-ng / custom sniff
           +-- sends observations to host or to EX520
```

- Cost: USD 10–30.
- Complexity: Low.
- Gain: Full probe-request capture on 2.4 and/or 5 GHz.
- Constraints: The EX520's USB port may be unavailable or occupied; the adapter needs a small SBC/host or must run on the router itself if a driver is installed.

### H.2 Option B — Small external SBC with dual-band monitor

```text
EX520
   +-- Raspberry Pi / Orange Pi / GL.iNet with monitor-capable Wi-Fi
       +-- runs DETECTIC "probe" sensor
       +-- communicates with EX520 over LAN/Wi-Fi
```

- Cost: USD 30–80.
- Gain: Independent, configurable, no EX520 changes.

### H.3 Option C — OpenWrt on a secondary cheap router

A Cudy WR3000 V1 or similar can run OpenWrt and host a monitor VIF, then report to the DETECTIC backend. This does **not** involve the EX520 at all, so it is a fallback, not a modification.

### Recommendation

**H.1 or H.2** are the most pragmatic fallbacks. Keep the EX520 untouched and add a small, low-cost sniffer that can run OpenWrt or a minimal Linux.

---

## I. Experimental Roadmap (P0–P3)

### Phase 0 — Documentation / static analysis (COMPLETED)

No router changes. Already completed by DETECTIC: GTPR mapping, HAL analysis, iwpriv inventory, `rssi_semantics.md`, `m11_probe_requests.md`.

### Phase 1 — Read-only capability discovery on live router

1. Confirm current tool list on the live EX520:
   ```bash
   which iw iwconfig iwpriv iwlist tcpdump ls
   ls /sys/class/ieee80211/
   ls /sys/class/net/
   cat /proc/net/wireless
   ```
2. Confirm `DEV2_WIFI_DE_UNASSOCSTA` still returns 9003.
3. Confirm `iwpriv` and `iwconfig` responses.

**Status:** SAFE, READ-ONLY.

### Phase 2 — Passive frame observation (requires root shell, no firmware modification)

1. Verify the EX520's driver type:
   ```bash
   ls /sys/class/ieee80211/      # if absent, cfg80211 not available
   cat /proc/net/wireless
   iwpriv rai0 get_driverinfo
   ```
2. Test the available `iwpriv` commands (`stat`, `get_site_survey`, `show`) for any undocumented unassociated-station or probe data.
3. **Do NOT attempt `bbp`/`mac`/`rf`/`rx` register operations on the production unit**; these can crash the wireless subsystem.
4. If `/sys/class/ieee80211/` is present on a different firmware revision, then attempt `iw`/`tcpdump` with extreme caution on a non-production unit.

**Expected result on current stock EX520V:** No standard monitor interface. Probe capture is not achievable on the router itself.

**Status:** LOW RISK if read-only; MEDIUM RISK if register-level `iwpriv` commands are used.

### Phase 3 — Controlled RF observation

1. Place known Wi-Fi devices near the EX520:
   - Associated and non-associated.
   - 2.4 GHz and 5 GHz.
   - Randomized MAC enabled.
   - At 1 m, 3 m, 5 m, 10 m.
2. Correlate probe periodicity, RSSI, IEs.
3. Build per-device fingerprints.

### Phase 4 — Feature correlation

Cluster observations and measure whether the same physical device can be recognized across different randomized MACs.

### Phase 5 — Proximity estimation

Calibrate RSSI-to-distance for the specific environment. Output broad categories first.

### Phase 6 — Movement estimation

Use RSSI trajectory and variance to classify APPROACHING / STATIONARY / DEPARTING.

### Phase 7 — CSI / advanced PHY

**DO NOT PROCEED** on EX520 unless a safe, supported CSI export is found. Consider moving to Option B/C external hardware.

---

## J. Test Matrix (proposed, not executed)

| Scenario | Expected observable with external sensor | Success criteria |
| --- | --- | --- |
| Device connected to EX520 | Already proven via GTPR | Existing tests continue to pass |
| Device NOT connected, Wi-Fi scanning | Probe Requests captured | >0 probe frames from the device in 60 s |
| Device connected to another AP | Probe Requests (looking for that AP or wildcard) | Captured on the external sensor's channel |
| Wi-Fi enabled but idle | Occasional wildcard probes or null frames | Periodicity observed |
| Wi-Fi actively scanning | Frequent Probe Requests | Burst pattern captured |
| Randomized MAC enabled | New MAC on every probe burst | Fingerprint clustering matches |
| 2.4 GHz device | Probes on 2.4 GHz channel | Signal on 2.4 GHz interface |
| 5 GHz device | Probes on 5 GHz channel | Signal on 5 GHz interface |
| Wi-Fi 6 device | HE Capabilities IE present | HE MAC/PHY cap parsed |
| older Wi-Fi device | HT or VHT caps, no HE | Correct legacy caps parsed |
| Device moving toward sensor | RSSI increases over 10–30 s | Trend slope > threshold |
| Device moving away | RSSI decreases | Trend slope < threshold |
| Device stationary | RSSI variance low | Variance below threshold |
| Multiple devices simultaneously | Multiple distinct probe streams | Pseudonyms and fingerprints kept separate |

---

## K. Final Decision Matrix

| Capability | EX520 alone (stock) | Additional software only | Firmware modification | External hardware |
| --- | --- | --- | --- | --- |
| Detect unassociated Wi-Fi | **NO** | **NO** (proprietary driver, no cfg80211) | YES (OpenWrt / patched driver) | YES (USB sniffer / SBC) |
| Probe Requests | **NO** | **NO** | YES | YES |
| RSSI | YES (associated) | **NO for unassociated** | YES | YES |
| RF fingerprint (HT/VHT/HE) | **NO** | **NO** | YES | YES |
| Randomized MAC correlation | **NO** | **NO** | YES | YES |
| Approximate proximity | **NO for unassociated** | **NO for unassociated** | YES | YES |
| Movement | **NO** | **NO** | YES (with CSI) | YES (with CSI/multi-antenna) |
| Direction | **NO** | **NO** | NO without array | POSSIBLE with array |
| CSI | **NO** | **NO** | RESEARCH ONLY | YES (dedicated CSI sensor) |
| Passive presence (non-transmitting) | **NO** | **NO** | NO / research | YES (radar/CSI/BLE/PIR) |
| BLE | **NO** | **NO** | NO (no BLE radio) | YES (BLE dongle) |

---

## L. GO / NO-GO / CONDITIONAL-GO Criteria

### NO-GO (recommended for the EX520 alone)

The EX520 **alone** cannot detect or track unassociated Wi-Fi devices on the stock firmware. The proprietary MediaTek driver does not register with cfg80211/nl80211, no `iw`/`tcpdump` is installed, and the available `iwpriv` commands do not expose raw frame data. The GTPR DataElements for unassociated stations are unimplemented stubs. Therefore:

- **NO-GO for implementing an unassociated-device detector directly on the EX520.**
- **NO-GO for CSI, non-transmitting passive presence, or direction finding on the EX520.**

### CONDITIONAL GO (for an external sniffer)

A **GO** is recommended **if** the project accepts adding a small external Wi-Fi monitor sensor (USB adapter or OpenWrt SBC) on the same LAN as the EX520. Conditions:

1. The external sensor is independently powered and does not require EX520 firmware changes.
2. The sensor runs a Linux/OpenWrt stack with `iw` + `tcpdump` or `airodump-ng` and a monitor-capable dual-band adapter.
3. The sensor can send observations to the DETECTIC backend or to the EX520 host for relay.

If these are true → **GO** for implementing the external-sensor probe-request pipeline.

### What would change the NO-GO for the EX520 alone

Only a future firmware revision or an undocumented, safe `iwpriv`/`HAL` command that exposes probe-request data could change the verdict. There is no evidence of this in the current firmware.

---

## M. Implementation-Ready Plan (external sensor fallback)

Since the EX520 alone is **NO-GO** for unassociated-device detection, the implementation target is a small, external Wi-Fi monitor sensor. The EX520 continues to provide associated-station data via the proven GTPR path; the external sensor provides the unassociated-device layer.

### M.1 Exact data source

- **Sensor:** external USB Wi-Fi adapter or OpenWrt SBC running `iw` + `tcpdump` / `airodump-ng`.
- **Interface:** `mon0` monitor VIF on the external sensor (`iw dev wlan0 interface add mon0 type monitor`).
- **Frame type:** 802.11 management Probe Request (`frame_control.subtype == 0x04`).
- **Metadata:** radiotap header (RSSI, noise, channel, MCS, antenna, flags) + 802.11 header + Information Elements.

### M.2 Exact interface / required privileges

- Root or `NET_ADMIN` on the external sensor.
- The sensor must be on the same management LAN as the EX520 or able to reach the DETECTIC backend.
- No privileges or changes on the EX520.

### M.3 Sensor changes

1. New `ExternalRFSensor` driver provider in `src/driver.rs` that reports `UnassociatedStations = Available` when the external sensor is reachable.
2. New `src/probe.rs` or `src/rf.rs` module to:
   - Receive 802.11 frames from the external sensor (pcap, JSON, or netlink).
   - Parse 802.11 headers and IEs.
   - Pseudonymize MACs.
   - Emit `ProbeObservation` or `DeviceNearby` events.
3. Extend `temporal.rs` to track probe-only devices with a shorter missing timeout.

### M.4 Data schema

```rust
struct ProbeObservation {
    device_id: String,      // HMAC pseudonym of source MAC
    mac_oui: Option<String>,
    band: String,
    channel: u32,
    frequency: u32,
    rssi_dbm: Option<i32>,
    rcpi: Option<u32>,
    signal_per_chain: Vec<i32>,
    noise_dbm: i32,
    timestamp: i64,
    ssid: Option<String>,   // wildcard or directed probe
    randomized: bool,
    capabilities_json: String,
    he_capabilities: Option<String>,
    vht_capabilities: Option<String>,
    ht_capabilities: Option<String>,
    vendor_ies: Vec<String>,
    confidence: f64,
}
```

### M.5 Pseudonymization

Use the same keyed HMAC already in `src/crypto.rs`: `HMAC(sensor_secret, raw_mac) → short digest`. Rotate the key per-sensor and never transmit raw MACs.

### M.6 Collection frequency

- Capture in the kernel continuously while the monitor VIF is up.
- Process/aggregate in user-space every 1–5 seconds.
- Send events to backend at most every 10 seconds.

### M.7 Buffering

Use the existing `event_transport.rs` / `ReliableQueue` + `SpoolEventTransport` with a bounded on-disk spool in `/var/tmp`.

### M.8 Backend transport

Send canonical `EventEnvelope` (already in `temporal.rs`) to `/api/v1/events` on the Cloudflare Worker. Schema is already prepared for v3 (device_state, device_sessions).

### M.9 Tests

- Unit tests for 802.11 frame parsing (mock pcap bytes).
- Pseudonymization tests.
- Integration test on the external sensor hardware (e.g., Raspberry Pi or OpenWrt SBC) with known test devices.

### M.10 Observability

- `detectic map --source rf` prints live probe observations.
- JSON log to `/var/tmp/detectic/probe.log`.
- Backend `/api/v1/state` and `/api/v1/sessions` already report device_state.

### M.11 Rollback and failure detection

- If the external sensor fails, fall back to the existing EX520 associated-station sensor.
- Store sensor calibration and configuration in the backend.
- Health check: if the external sensor stops sending heartbeats, mark `UnassociatedStations` as `Unavailable`.

### M.12 Service-continuity safeguards

- The external sensor is independent; no EX520 reboot or VIF change is required.
- Use `FIF_PROBE_REQ` / BPF filter on the sensor to reduce load.
- Test on a non-EX520 device first; the EX520 continues to serve clients normally.
- If the sensor is plugged into the EX520's USB port, validate power and thermal limits.

---

## N. References

1. `investigations/ex520_unassociated_detection_report.md` — static and API-level finding that unassociated devices are not exposed via GTPR.
2. `investigations/m11_probe_requests.md` — live test conclusion that monitor mode is the only remaining path.
3. `investigations/m4_4_wifi_capability.md` — tool inventory and confirmed capabilities.
4. `investigations/m4_4_hal_hardware_runtime.md` — live runtime finding that the MediaTek driver does not register with cfg80211/nl80211 and that `/sys/class/ieee80211/` is absent.
5. `investigations/rssi_semantics.md` — RCPI interpretation and distance model.
6. `investigations/libplatform_api/ANALYSIS.txt` — HAL capability analysis.
7. `src/driver.rs` — capability matrix and `ProbeObservation` placeholder.
8. `src/temporal.rs` — temporal event engine and `DeviceNearby` event kind.
9. Upstream `mt76` driver (Linux 6.6+) — monitor VIF and AP coexistence support (not applicable to EX520 stock firmware, but relevant for external OpenWrt sensors).
10. MediaTek MT7981B/Filogic 820 datasheet — 2×2 2.4 GHz + 2×2 5 GHz, beamforming, OFDMA.
11. `wifi-densepose` research notes — MT7981/MT76 does not export per-packet complex channel estimates in upstream driver.
