# libplatform_api.so — Implications for Detectic

> **Source:** `investigations/libplatform_api/ANALYSIS.txt` (static RE of
> `_rootfs/lib/libplatform_api.so`, ARM aarch64, MediaTek EasyMesh HAL).
> **Purpose:** Update the conclusions of `ex520_unassociated_detection_report.md`
> in light of the HAL disassembly, and define what this means for the native
> Rust sensor.

---

## 1. Correction to the previous report

`ex520_unassociated_detection_report.md` §4.2 concluded:

> "the 1905.1 protocol would need another EasyMesh device on the network before
> any query/response traffic is generated. Therefore these capabilities are
> **not usable for Detectic on a single stock router**."

That statement is correct **for the 1905 protocol layer** (`libtp1905.so`),
but wrong **at the HAL layer**. The disassembly proves:

> "All functions in the analysed set are local MediaTek HAL helpers … There is
> no socket communication with a remote 1905 controller/agent … They can be
> called locally by any process that links libcutil.so and has privileges to
> open the wireless interface." (ANALYSIS.txt §8)

| Layer | Needs an EasyMesh peer? | Callable locally with shell? |
|---|---|---|
| `libtp1905.so` (1905.1 packets) | Yes | Irrelevant — we skip it |
| `mapController` / `mapAgent` | Yes (each other) | Irrelevant |
| **`libplatform_api.so` (HAL)** | **No** | **Yes** |

The EasyMesh daemons are merely *clients* of the HAL. Nothing prevents another
process from being a client too.

---

## 2. What the HAL exposes (local, read-only unless noted)

| Function | Driver access | Input | Output | Detectic value |
|---|---|---|---|---|
| `hal_multiap_mtk_getAssociateStaList` | ioctl 0x8be1, OID `0x0a01` | ifname | ≤128 stations × 640 B: MAC, traffic, link metrics (RSSI), assoc frame | Same data as GTPR `ASSOCDEV`, no HTTP round-trip |
| `hal_multiap_mtk_getUnassocStaLinkMetrics` | ioctl 0x8be1, OID `0x0a03` | band, **known STA MAC** | 24 B: channel, RSSI, timestamp | Directed RSSI to a **known** MAC, even when it is *not associated* |
| `hal_multiap_mtk_getScanResult` | ioctl 0x8be1, OID `0x0b04` | ifname | ≤128 × 52 B BSS entries (SSID, BSSID, RSSI/ch) | Neighboring **APs**, not clients |
| `hal_multiap_mtk_getRssi` | ioctl 0x8be1, OID `0x0b05` | ifname | int32 RSSI | Interface-level signal |
| `hal_multiap_mtk_getRadioMetrics` | ioctl 0x8be1, OID `0x0a05` | ifname | 10 B: BSSID, noise, counters | Channel noise floor |
| `hal_multiap_mtk_doScan` | `iwpriv … SiteSurvey=` (**writes/triggers**) | ifname | — | Refresh scan data (disruptive-ish) |
| `hal_multiap_mtk_event_init/process` | netlink `RTM_NEWLINK`, groups=1 | — | link-change events | Real-time assoc/disassoc triggers |

### 2.1 Critical scope limit (do not overclaim)

`getUnassocStaLinkMetrics(band, staMac, out)` **takes the MAC as input**.
It is a *directed measurement*, not a passive sniffer:

```text
KNOWN MAC ──► driver measures RSSI on frames from that MAC ──► {RSSI, ts}
UNKNOWN MACs ──► NOT enumerated by any analysed function
```

Therefore the honest capability statement becomes:

| Capability | Stock API (GDPR) | Native HAL (shell required) |
|---|---|---|
| Associated clients + RSSI | ✅ `DEV2_WIFI_APDEV_ASSOCDEV` | ✅ OID 0x0a01 (faster, richer) |
| Discover unknown nearby clients | ❌ | ❌ (no function enumerates them) |
| Track RSSI/presence of a **known** MAC while unassociated | ❌ | ✅ OID 0x0a03 |
| Neighbor AP list | ❌ (stub handlers) | ✅ OID 0x0b04 |
| Real-time events (assoc/disassoc/link) | ❌ (poll only) | ✅ netlink |

This refines AGENTS.md §16's distinction: the EX520 can observe *connected*
devices either way; *environment* observation stays limited to known-MAC
measurement and neighbor-AP scanning — still valuable for presence inference,
cross-sensor correlation, and inter-measurement gap filling.

---

## 3. Two implementation paths for the Rust sensor

### Path A — FFI into the vendor HAL

Link against `libplatform_api.so` + `libcutil.so`, call
`multiap_get_associate_sta_list(...)` etc.

- Pros: exact vendor semantics, less RE work.
- Cons: two extra `.so` dependencies must be shipped/copy-deployed; ABI
  fragility across firmware updates; contradicts AGENTS.md §25 (no heavy
  dependencies).

### Path B — Reimplement the ioctls natively (recommended)

The wire format is fully documented by the analysis:

```c
int fd = socket(AF_INET, SOCK_DGRAM, 0);
struct iwreq req = {0};
strlcpy(req.ifr_name, "rax0", IFNAMSIZ);        // ra0 / rai0 / rax0
req.u.data.pointer = buf;                        // caller buffer
req.u.data.length = buflen;
req.u.data.flags   = 0x0a01;                     // OID sub-command
ioctl(fd, 0x8be1 /* SIOCIWFIRSTPRIV+1 */, &req);
```

- Pros: single static musl binary (current 1.1 MB profile unchanged), no
  runtime deps, survives firmware minor updates as long as the private-ioctl
  OIDs stay stable.
- Cons: struct layouts (640 B/station, 52 B/BSS, 24 B unassoc result) were
  inferred statically; offset-level confirmation needs dynamic tracing on the
  device once shell access exists.

A `MtkHalProvider` implementing the existing `WiFiProvider` boundary
(AGENTS.md §19) maps cleanly onto Path B:

```text
Detectic core
     │
WiFiProvider trait
     ├── GtprClient        (works today, no shell)      ← src/transport.rs
     └── MtkHalProvider    (shell required, ioctl-based) ← future src/hal_linux.rs
```

---

## 4. Prerequisites and ordering

The HAL path is gated on shell access, which is already scoped in
`REMOTE_ACCESS_OBJECTS.md` (telnet/dropbear objects compiled in; live
verification pending). Correct order therefore remains:

```text
1. GTPR sensor (done, works without shell)          ← Milestones M3–M6
2. Shell enablement via GDPR writes                  ← REMOTE_ACCESS_OBJECTS.md
3. Deploy static binary to /var/run/misc/misc_rw     ← DEPLOYMENT_PATHS.md §3
4. Confirm ioctl structs dynamically                 ← strace/gdb on-device
5. Add MtkHalProvider behind the same trait          ← Path B
6. Netlink event source replaces fixed-interval poll ← optional optimization
```

Steps 4–6 must not begin before 2–3 are proven, per AGENTS.md §43
(inspect → measure → prototype).

---

## 5. Security / privacy notes carried forward

- All HAL outputs contain raw MACs → must flow through the existing
  HMAC-SHA256 pseudonymization layer (`crypto::pseudonymize`) before storage
  or upload; never logged raw (AGENTS.md §21).
- `doScan` / `setUnassocLinkMetrics` build `iwpriv` command lines via
  `system()` in the vendor code — our Rust reimplementation must avoid any
  string-formatted command execution entirely.
- Private ioctls require root-equivalent privileges; the sensor should drop
  privileges after opening the socket if possible.

---

## 6. Artifacts

- `investigations/libplatform_api/ANALYSIS.txt` — full static analysis (input).
- This document — corrected conclusions + implementation plan.
