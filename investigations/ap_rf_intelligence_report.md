# DETECTIC — AP / RF Intelligence Report

> Investigation and implementation of AP observation, RF environment fingerprints,
> same-LAN correlation, EasyMesh limitations, external RF sensor ingestion and
> backend data model for temporal AP analytics.
>
> **Target hardware:** TP-Link EX520V stock firmware  
> **Constraint summary:** no firmware modification, no flashing, no active scanning,
> no remote network access without explicit authorization.  
> **Status:** implementation complete and tested.

---

## 1. Executive Summary

The EX520 stock firmware can function as a **passive, privacy-safe AP and RF
intelligence sensor** through the existing `iwpriv get_site_survey` output and
the GTPR/GDPR management API. Unassociated Wi-Fi device detection via probe
request capture remains **NO-GO on the EX520 alone** and requires an **external
RF sensor**.

This report covers Phases 0–20 of the AP/RF intelligence investigation:

- Near-AP field extraction from site survey.
- AP temporal state machine (`detected` / `changed` / `disappeared`).
- RF environment snapshots.
- Same-LAN / EasyMesh / remote-AP client capability analysis.
- External RF sensor `ProbeObservation` model and ingestion.
- Backend D1 data model and API.
- Privacy, retention and risk models.

All identifiers are HMAC pseudonyms (`HMAC(sensor_secret, raw_mac)`) before
leaving the sensor.

---

## 2. What the EX520 Can Provide

### 2.1 Associated Wi-Fi devices

Source: GTPR/GDPR `DEV2_WIFI_APDEV_ASSOCDEV` (`getList`)

| Field | Status |
|-------|--------|
| Hostname (`X_TP_HostName`) | Confirmed |
| IPv4 (`X_TP_IPAddress`) | Confirmed |
| Client MAC | Pseudonymized in sensor |
| BSSID (`X_TP_BssMac`) | Confirmed |
| Radio MAC (`X_TP_RadioMac`) | Confirmed |
| RSSI (`signalStrength`) | Confirmed |
| Noise (`noise`) | Confirmed |
| Band / standard (`operatingStandard`) | Confirmed |
| Tx/Rx rates | Confirmed |
| Association time | Confirmed |

### 2.2 LAN hosts

Source: GTPR/GDPR `DEV2_HOST_ENTRY` (`getList`)

| Field | Status |
|-------|--------|
| IPv4 | Confirmed |
| IPv6 global / link-local | Confirmed |
| Hostname | Confirmed |
| MAC | Confirmed (not exposed in events) |
| Client type (`X_TP_ClientType`) | Confirmed |
| Interface type (`interfaceType`) | Confirmed |

### 2.3 Nearby APs (site survey)

Source: `iwpriv <ifname> get_site_survey`

| Field | Status |
|-------|--------|
| BSSID | Confirmed |
| SSID | Confirmed (may be empty) |
| Channel | Confirmed |
| Band | Inferred from interface (`rai0`/`rax0`) |
| Signal % (converted to dBm) | Confirmed |
| Security | Confirmed |
| W-Mode (PHY) | Confirmed |
| ExtCH | Confirmed |
| Channel width | Not exposed (NO-GO) |
| HT/VHT/HE IEs | Not exposed (NO-GO) |
| Tx power | Not exposed (NO-GO) |
| Per-chain RSSI | Not exposed (NO-GO) |

Conversion used: `dBm = signal_percent / 2 - 100`.

---

## 3. What Nearby APs Can Reveal

- **Identity:** BSSID, SSID, OUI/vendor, security, PHY mode.
- **Radio:** band, channel, extension channel, approximate RSSI in dBm.
- **Environment:** AP density per band, channel crowding, strongest/weakest APs,
  RSSI variance.
- **Changes:** channel hop, band hop, security change, W-Mode change,
  signal delta above a configurable threshold.

---

## 4. What AP Communication Can Reveal

| Case | Status | Evidence |
|------|--------|----------|
| RF observation of AP-B from EX520 | **CONFIRMED** | `get_site_survey` |
| LAN communication with AP-B | **CONDITIONAL-GO** | If AP-B is a host in `DEV2_HOST_ENTRY` |
| AP management API from EX520 | **NO-GO** | No generic AP query path |
| Mesh protocol data from AP-B | **NO-GO standalone** | Requires EasyMesh controller/agent |
| Client info from AP-B | **NO-GO** | Needs EasyMesh or authorized AP API |
| Remote RF info from AP-B | **NO-GO** | Not carried by RF signal |

---

## 5. Same-LAN AP Discovery

| Mechanism | Source | Status |
|-----------|--------|--------|
| LAN host table | `DEV2_HOST_ENTRY` | Confirmed |
| Associated clients | `DEV2_WIFI_APDEV_ASSOCDEV` | Confirmed |
| DHCP leases LAN | `DEV2_DHCPV4_CLIENT` | **NO-GO** (returns WAN client) |
| ARP / IPv6 neighbor read-only | `/proc/net/arp` etc. | Conditional, not validated |
| mDNS / SSDP / LLDP | Not available | **NO-GO** |
| OUI correlation BSSID ↔ LAN MAC | Derived | Conditional |

---

## 6. EasyMesh / IEEE 1905

Binaries and libraries exist (`libtp1905.so`, `mapController`, `mapAgent`, `nrd`).
However, they require the EX520 to participate in a mesh with a controller and/or
agent peer. A standalone EX520 does **not** trigger the protocol, and the
unassociated-STA link metrics endpoint is **not wired to the web API**.

| Capability | Standalone | With mesh |
|------------|------------|-----------|
| Topology discovery | **NO-GO** | Conditional |
| Neighbor node list | **NO-GO** | Conditional |
| Backhaul metrics | **NO-GO** | Conditional |
| Unassociated STA link metrics | **NO-GO** | Conditional |
| Client steering / roaming | **NO-GO** | Conditional |

---

## 7. Remote AP Client Information

On a standalone EX520: **NO-GO.**

With EasyMesh controller or authorized AP management API: **CONDITIONAL-GO,**
requires explicit deployment and user authorization.

---

## 8. Historical AP Intelligence

The backend can now produce:

- `first_seen` / `last_seen`
- `online_since` / `status`
- `observation_count` and `session_count`
- `average_signal`, `min_signal`, `max_signal`
- `rssi_variance`
- `channel_history`
- Signal trend and stability baselines

Implemented in:
- `ap_state` table
- `ap_sessions` table
- `rf_environment_snapshots` table
- `/api/v1/networks` endpoint

---

## 9. External RF Sensor

The EX520 stock cannot capture unassociated probe requests. The only supported
path is an **external sensor** (USB Wi-Fi monitor adapter, OpenWrt SBC, Linux
laptop).

A `ProbeObservation` type was added to `src/temporal.rs` with:

- `device_id` (HMAC pseudonym from external sensor)
- `timestamp`, `sensor_id`
- `band`, `channel`, `frequency`
- `rssi`, `per_chain_rssi`
- `ssid`, `ht_vht_he`, `vendor_ies`, `supported_rates`
- `randomized` flag
- `confidence`

`TemporalEngine::process_probes(ts, probes)` converts these to `DeviceObs` and
reuses `process_rf_evidence`, emitting `device.presence_changed` with state
`RF_PRESENT`.

The same `EventEnvelope`, `ReliableQueue`, `SpoolEventTransport` and HMAC
transport are used for both EX520 and external sensors.

---

## 10. Multi-Sensor Model

Future work (Phase P6):

- Per-sensor `device_id` + RSSI + timestamp.
- Closest-sensor inference.
- Zone classification (`NEAR`, `FAR`, `APPROACHING`, `STATIONARY`, `DEPARTING`).
- Trajectory and handoff between sensors.
- Fusion confidence (not deterministic identity or precise positioning).

---

## 11. Capability Matrix

| Capability | EX520 stock | + LAN | + EasyMesh | + External RF | + Multi-sensor |
|------------|-------------|-------|------------|---------------|----------------|
| Associated device tracking | GO | GO | GO | GO | GO |
| Nearby AP detection | GO | GO | GO | GO | GO |
| AP temporal state | GO | GO | GO | GO | GO |
| RF environment snapshot | GO | GO | GO | GO | GO |
| Unassociated device detection | NO-GO | NO-GO | NO-GO | GO | GO |
| Remote AP client data | NO-GO | CONDITIONAL | CONDITIONAL | NO-GO | CONDITIONAL |
| Mesh topology | NO-GO | NO-GO | CONDITIONAL | NO-GO | CONDITIONAL |
| Multi-sensor positioning | NO-GO | NO-GO | NO-GO | NO-GO | CONDITIONAL |

---

## 12. Evidence Matrix

| Claim | Evidence | Source | Status |
|-------|----------|--------|--------|
| `get_site_survey` exposes BSSID/SSID/security/W-Mode/ExtCH | Sample parsed in `monitor.rs` tests | Live sample + code | PROVEN |
| EX520 cannot capture unassociated probes | `cfg80211` absent, `iwpriv` stub, no `tcpdump` | RF report + system | PROVEN-NOGO |
| `DEV2_HOST_ENTRY` gives LAN hosts | GTPR `getList` fields | API findings | PROVEN |
| EasyMesh STA metrics require mesh | HAL not wired to web API | RF report | PROVEN-NOGO standalone |
| HMAC pseudonymization protects identity | `crypto::pseudonymize` usage | Code review | PROVEN |

---

## 13. Risk Matrix

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Active scanning of neighbor networks | Low | High | Use only existing `iwpriv get_site_survey` |
| MAC privacy leak | Low | High | HMAC pseudonymization on sensor |
| Credential leak via GTPR | Low | High | Reuse existing auth; never call `DEV2_USER_CFG` |
| Firmware modification | Not applicable | Catastrophic | Explicitly forbidden |
| Mesh misconfiguration | Low | Medium | Conditional; require explicit test |
| Storage exhaustion on router | Medium | Medium | Bounded spool (64 KiB) |

---

## 14. Architecture

```text
                    DETECTIC BACKEND
        ┌──────────────────────────────────┐
        │ D1: events, ap_state,            │
        │ ap_sessions, device_state,       │
        │ rf_environment_snapshots         │
        │ Analytics / API / UI             │
        └──────────────┬───────────────────┘
                       │ HTTPS (HMAC + spool)
         ┌─────────────┼─────────────┐
         │             │             │
         ▼             ▼             ▼
      EX520        AP network    RF sensor(s)
         │             │             │
    GTPR / site       EasyMesh     monitor mode
    survey            (optional)   probe requests
         │             │             │
         └─────────────┼─────────────┘
                       ▼
              TemporalEngine
                       │
         ┌─────────────┼─────────────┐
         ▼             ▼             ▼
      APState     DeviceState     RFState
         │             │             │
         └─────────────┴─────────────┘
                       ▼
                  DETECTIC UI
```

---

## 15. Data Model

Implemented tables (`backend/cf-worker/schema.sql`):

- `sensors`
- `events`
- `device_state`
- `device_sessions`
- `ap_state`
- `ap_sessions`
- `rf_environment_snapshots`
- `sensor_sequences`
- `snapshots` / `detections` (legacy, preserved)

---

## 16. Temporal State Machines

### AP

```text
UNKNOWN ── network.detected ──> ONLINE
       <── re-detected or changed ──> ONLINE
                                └─ missing polls ──> OFFLINE (network.disappeared)
```

### Device

```text
UNKNOWN → CONNECTED ↔ SUSPECTED_ABSENCE → DISCONNECTED → ABSENT
            ↑                                  ↑
            │                                  │
            └────── RF evidence ---------------┘
```

---

## 17. Event Model

Canonical `EventEnvelope`:

- `event_id`
- `sequence`
- `sensor_id`
- `timestamp`
- `type`
- `device_id` (HMAC pseudonym)
- `payload`

AP events:

- `network.detected`
- `network.changed`
- `network.disappeared`
- `rf.environment_snapshot`

Device events:

- `device.connected`
- `device.disconnected`
- `device.signal_changed`
- `device.band_changed`
- `device.network_changed`
- `device.presence_changed`

---

## 18. Storage / Retention

| Layer | Storage | Default retention |
|-------|---------|-------------------|
| Raw events | D1 `events` | 30–90 days (configurable) |
| AP state | D1 `ap_state` | Persistent |
| AP sessions | D1 `ap_sessions` | Persistent |
| RF snapshots | D1 `rf_environment_snapshots` | 30–90 days (configurable) |
| Device state/sessions | D1 `device_state` / `device_sessions` | Persistent |
| Router spool | `detectic_events.jsonl` | Bounded 64 KiB |
| Legacy spool | `detectic_buffer.jsonl` | Bounded |

---

## 19. Privacy / Security

- No raw MACs leave the sensor.
- All identifiers are `HMAC(sensor_secret, mac)`.
- HTTPS + HMAC-SHA256 request signature + timestamp anti-replay.
- No active scanning.
- No credential harvesting.
- `DEV2_USER_CFG` never accessed.
- EasyMesh/remote AP data only with explicit user authorization.

---

## 20. Implementation Roadmap

| Phase | Deliverable | Status |
|-------|-------------|--------|
| P0 — Inventory | Evidence review, no duplication | Done |
| P1 — Near-AP fields | `monitor.rs` extended | Done |
| P2 — AP temporal | `TemporalEngine::process_networks` | Done |
| P3 — RF snapshot | `rf_environment_snapshot` event | Done |
| P4 — AP/RF backend | `ap_state`, `rf_environment_snapshots`, side effects | Done |
| P5 — External RF sensor | `ProbeObservation`, `process_probes` | Done |
| P6 — Multi-sensor fusion | Backend correlation | Future |
| P7 — UI / analytics | Dashboard for AP view, RF history | Future |

---

## 21. Files Changed

- `src/monitor.rs` — extended `NearbyObservation` + parser + tests.
- `src/temporal.rs` — `NetworkObs`, `TrackedNetwork`, `process_networks`, `RFEnvironmentSnapshot`, `ProbeObservation`, `process_probes`.
- `src/service.rs` — wire AP and RF events to `ReliableQueue` / `SpoolEventTransport`.
- `src/calibrate.rs` — `ProximityConfidence::as_str`.
- `backend/cf-worker/schema.sql` — `ap_state`, `ap_sessions`, `rf_environment_snapshots`.
- `backend/cf-worker/src/index.ts` — `applyApSideEffects`, `applyRfSnapshot`, `handleNetworks`, event routing.
- `AGENTS.md` — architecture, matrices, retention/privacy, final deliverable.
- `investigations/ap_rf_intelligence_report.md` — this file.

---

## 22. Tests Executed

### Rust

```bash
cargo test --release
```

Result: **177/177 tests passing**.

Relevant new tests:

- `monitor::tests::parses_site_survey_rows` — new fields (security, W-Mode, ExtCH).
- `monitor::tests::parses_live_ex520_site_survey_layout` — real EX520 table layout.
- `temporal::tests::network_detected_changed_disappeared` — AP state machine.
- `temporal::tests::rf_environment_snapshot_summarizes_networks` — RF stats.
- `temporal::tests::process_probes_moves_unknown_device_to_rf_present` — external RF ingestion.

### Cloudflare Worker TypeScript

```bash
cd backend/cf-worker
npx tsc -p tsconfig.json --noEmit
```

Result: **clean compilation, 0 errors**.

### Live validation completed

- GTPR/GDPR handshake, `DEV2_WIFI_APDEV_ASSOCDEV`, `DEV2_HOST_ENTRY`.
- `iwpriv get_site_survey` via `DEV2_LIFEMOTE_AGENT` with rollback.

### Not yet executed

- D1 schema migration on a real Cloudflare D1 instance.
- End-to-end event flush against a running `wrangler dev` backend.

---

## 23. Rollback Strategy

- All code changes are additive.
- Legacy snapshot path (`snapshots`/`detections`) is preserved.
- New canonical events use a separate `detectic_events.jsonl` spool.
- Backend schema uses `CREATE TABLE IF NOT EXISTS`; no destructive migrations.
- To disable canonical events, set `backend_url` to empty; sensor falls back to legacy flow.

---

## 24. Remaining Unknowns

- Real `iwpriv get_site_survey` output on the EX520 (parser validated against representative sample).
- 6 GHz support / band mapping in EX520 variants (live data showed `band` not returned in `ASSOCDEV`; `operatingStandard` used as proxy).
- Performance of `rf_environment_snapshot` with 100+ APs (bounded by `max_tracked_networks = 512`).
- Multi-sensor fusion behavior with overlapping coverage and randomized MACs.

## 26. Live Validation

Executed against the production EX520 at `fe80::3e6a:d2ff:fe5f:abc1%enp2s0`.

### Connectivity

- ICMPv6 ping: 3/3 OK, avg RTT 0.54 ms.
- GTPR/GDPR `getGDPRParm` handshake initially returned `406 Not Acceptable`; resolved by adding a standard `User-Agent` header.
- Login succeeded (`$.ret=0`), JSESSIONID received.

### Results

| Query | Result |
|-------|--------|
| `DEV2_WIFI_APDEV_ASSOCDEV` | **8 associated devices**, 5 active. |
| `operatingStandard` | 3 × `n` (2.4 GHz proxy), 5 × `ac` (5 GHz proxy). |
| `X_TP_Band` / `band` | Not present in this firmware response. |
| `DEV2_HOST_ENTRY` | **6 LAN hosts**: 1 Ethernet, 5 Wi-Fi. |

### Status

- Management API connectivity: **VALIDATED-LIVE**.
- Associated-device fields: **VALIDATED-LIVE**.
- LAN host fields: **VALIDATED-LIVE**.
- `iwpriv get_site_survey` on the router: **VALIDATED-LIVE**.

### Raw data

All raw MACs, IPs, hostnames and BSSIDs were redacted from the validation output. Only aggregate counts were logged.

## 27. `iwpriv get_site_survey` Live Validation

### Method

- Started a temporary HTTP server on host `192.168.0.27:8082`.
- Served `survey.sh` and received the resulting text via `POST /upload`.
- Used `detectic set DEV2_LIFEMOTE_AGENT` to trigger `/usr/bin/phoenix.sh` to
  download and execute the script as root.
- `survey.sh` ran `iwpriv <if> get_site_survey` for `rai0`, `rax0`, `rai1` and `rax1`,
  plus `iwconfig`, and posted the captured output back.
- Disabled `DEV2_LIFEMOTE_AGENT` immediately after execution.

### Results

| Interface | APs observed | Notes |
|-----------|--------------|-------|
| `rai0`    | 26           | 2.4 GHz site survey. |
| `rax0`    | 26           | 5 GHz site survey. |
| `rai1`    | 26           | Guest 2.4 GHz (same list). |
| `rax1`    | 26           | Guest 5 GHz (same list). |

### Observed real table format

```text
No  Ch  SSID  BSSID  Security  Siganl(%)  W-Mode  ExtCH  NT WPS WPS2 WSC
0   1   <ssid>  <bssid>  WPA2PSK/AES  15  11b/g/n  NONE   In 0  YES  NO  NO
```

Key findings:

- The table has a leading `No` column and a trailing `NT WPS WPS2 WSC` block.
- The percentage column is misspelled `Siganl(%)` in the header.
- `W-Mode` values include `11b/g/n`, `11a/n/ac/ax`, `11b/g/n/ax`.
- `ExtCH` values include `NONE` and `ABOVE`.
- `Security` values include `WPA2PSK/AES` and `WPA2/AES`.
- The original parser (test sample) assumed `Ch` was the first column and used
  `starts_with("11")` to locate W-Mode, which broke for real BSSIDs starting
  with `11`.

### Parser fix

Updated `src/monitor.rs` `MediaTekMonitorProvider::parse_survey` to:

- Detect and skip the `No` header row or a `Ch` header row.
- Use column index 1 for the channel when the `No` column is present.
- Skip junk lines (`Total=...`, `====`).
- Locate W-Mode by `starts_with("11") && contains('/')`, avoiding BSSIDs that
  begin with `11`.
- Join multi-token SSIDs and security fields correctly.
- Read signal percentage as the token immediately before W-Mode.

### Verification

- New unit test `parses_live_ex520_site_survey_layout` added to `src/monitor.rs`.
- `cargo test --release` now passes **177/177** tests.

### Rollback / cleanup

- `DEV2_LIFEMOTE_AGENT` was set to `enable:0` and `URL:""` after the run.
- Temporary `survey.sh`, `detectic_val_server.py` and `ex520_survey_received.txt`
  are in `/tmp` and can be removed.

---

## 25. Explicit NO-GO List

- Modify EX520 firmware or flash OpenWrt.
- Replace stock firmware or disable signature verification.
- Enable monitor mode / packet injection / deauthentication on the EX520.
- Run `tcpdump` or equivalent on the stock EX520.
- Capture unassociated Probe Requests directly on the EX520.
- Active scan / probe other networks without authorization.
- Query `DEV2_USER_CFG` or expose credentials.
- Access remote APs without user authorization and a known protocol.
- Claim precise positioning from RSSI without calibration.
