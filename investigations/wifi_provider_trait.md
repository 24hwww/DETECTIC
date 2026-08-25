# Detectic — WiFiProvider Trait (Milestone M4)

> **Design time only** — no production code is changed. This document defines
> the trait interface that will later support multiple data sources (GTPR web
> API + MediaTek HAL ioctl). The trait is defined in documentation and a
> prototype; implementation is deferred to when shell access is available.

---

## 1. Motivation

The Detectic sensor currently obtains Wi-Fi observations exclusively through
the GTPR/GDPR web API (Milestones M1–M3 proven on the real EX520V). While
this is sufficient for associated station discovery, it has known limitations:

| Limitation | GTPR-only | With HAL provider |
|---|---|---|
| Associated clients | ✅ via `DEV2_WIFI_APDEV_ASSOCDEV` | ✅ faster, richer ioctl output |
| Unknown nearby clients | ❌ (stub handlers in libcmm.so) | ✅ `getScanResult` returns neighboring BSSs |
| Directed RSSI to known MAC | ❌ | ✅ `getUnassocStaLinkMetrics` with known MAC |
| Real-time event notifications | ❌ (poll only) | ✅ netlink `RTM_NEWLINK` events |
| Radio stats (noise, channel) | ❌ | ✅ `getRadioMetrics` + `getRssi` |

Defining a common trait now ensures a clean migration path when shell access
becomes available (via the `REMOTE_ACCESS_OBJECTS.md` telnet/dropbear objects).

---

## 2. Trait architecture

```rust
/// Trait abstracting the Wi-Fi data source for Detectic.
///
/// Implementations exist today for:
/// - `GtprProvider` (the current GTPR web API, Milestones M1–M3)
/// - `MtkHalProvider`   (prototype in `prototypes/mtk_hal_probe/`, requires
///   shell access + ioctl privileges)
///
/// Future implementations may include:
/// - Monitor-mode `tcpdump` parser (if/when monitor mode is enabled)
/// - EasyMesh 1905.1 controller/agent telemetry
trait WiFiProvider {
    /// Return associated stations observed by the provider.
    ///
    /// Corresponds to GTPR `DEV2_WIFI_APDEV_ASSOCDEV` or HAL
    /// `getAssociateStaList` (OID 0x0a01). The output format is normalized
    /// to `AssociatedDevice` so the upper layers (events, persistence, upload)
    /// are provider-agnostic.
    fn associated_devices(&self) -> Vec<AssociatedDevice>;

    /// Return neighboring BSS/AP results observed by the provider.
    ///
    /// Corresponds to GTPR DataElements OIDs (mostly stubs) or HAL
    /// `getScanResult` (OID 0x0b04). Yields BSS entries (APs, not clients)
    /// with RSSI-like metrics and channel information.
    fn nearby_devices(&self) -> Vec<ScanResult>;

    /// Return radio statistics observed by the provider.
    ///
    /// Corresponds to HAL `getRadioMetrics`, `getRssi`, etc. Provides band,
    /// noise floor, and channel-specific information.
    fn radio_stats(&self) -> RadioStats;
}

/// Radio statistics shared across provider implementations.
#[derive(Debug, Clone)]
pub struct RadioStats {
    /// Wireless band (0 = 2.4 GHz, 1 = 5 GHz, or vendor-specific encoding)
    pub band: u8,
    /// Noise floor metric from the driver (vendor-specific; see
    /// investigations/rssi_semantics.md for dBm conversion)
    pub noise_floor: i32,
    /// Current channel number
    pub channel: u8,
}
```

---

## 2. Provider interface contract

| Method | Intended HAL OID | GTPR equivalent | Notes |
|---|---|---|---|
| `associated_devices()` | `0x0a01` getAssociateStaList | `DEV2_WIFI_APDEV_ASSOCDEV` | HAL returns richer per-station link metrics + RSSI |
| `nearby_devices()` | `0x0b04` getScanResult | `DEV2_WIFI_DE_SCAN_RESULT` / DataElements (stub) | Returns APs; client discovery not supported by either source |
| `radio_stats()` | `0x8be1` OID 0x0b05/getRssi + 0x0a05/getRadioMetrics | Not directly exposed via GTPR | Provides band, noise_floor, channel |

---

## 3. Prototype implementation (isolated, no production impact)

The prototype in `prototypes/mtk_hal_probe/` implements a mock
`MtkHalProvider` that satisfies the `WiFiProvider` trait using static
mock data (see `prototypes/mtk_hal_probe/src/main.rs`). When shell access
is available, a real implementation would:

1. Open `fd = socket(AF_INET, SOCK_DGRAM, 0)`
2. Populate `struct iwreq` with `ifr_name = "rax0"` (or `ra0`/`rai0`)
3. Call `ioctl(fd, 0x8be1, &iwreq)` with `flags` set to the desired OID
4. Read the driver-written output buffer and normalize into `AssociatedDevice`,
   `ScanResult`, or `UnassocMetrics`

The prototype already provides the normalization layer; the missing piece
is the actual `ioctl` call, which requires root on the router.

---

## 4. Migration path

Once shell access is established (see `REMOTE_ACCESS_OBJECTS.md` for the
telnet/dropbear enablement path):

1. Add `MtkHalProvider` implementation that calls the real ioctls
2. Update the sensor runtime to select between `GtprProvider` and
   `MtkHalProvider` via configuration or environment
3. Gradually transition event pipelines to consume from the new provider
4. Deprecate GTPR-only paths after validation

No production code changes are required until this point. The trait and
prototype exist purely for forward compatibility and design space exploration.

---

## 5. Artifacts produced

- `investigations/mtk_hal_validation.md` — HAL ioctl structure reference
- `prototypes/mtk_hal_probe/` — Rust prototype + mock data
- This document — trait definition + migration plan