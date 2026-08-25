# Detectic — MediaTek HAL Validation (Stock EX520V)

> **Source:** `investigations/libplatform_api/ANALYSIS.txt` (static reverse
> engineering of `_rootfs/lib/libplatform_api.so`, ARM aarch64, MediaTek
> EasyMesh HAL, 94312 bytes, OpenWrt GCC 8.4.0).
>
> **Purpose:** Document the exact ioctl interface for `getAssociateStaList`,
> `getScanResult`, and `getUnassocStaLinkMetrics` so a standalone Rust
> prototype can validate the HAL as a read-only data source without modifying
> the firmware.
>
> **Constraint:** No firmware writes, no binary patches, no monitor mode, no
> reboots. Read-only validation only.

---

## 1. ioctl substrate (common to all three functions)

| Field | Value |
|---|---|
| Request code | `0x8be1` (`SIOCIWFIRSTPRIV+1`, Linux private wireless ioctl) |
| Socket type | `AF_INET`, `SOCK_DGRAM` (opened once, stored in `g_ioctl_sock`) |
| Interface name | `rax0` / `ra0` / `rai0` (MediaTek naming; the HAL uses `ifreq.ifr_name`) |
| Struct type | `struct iwreq` (standard wext wrapper) |
| Buffer ownership | Caller-allocated; driver writes into user-space buffer pointed to by `u.data.pointer` |
| Length field | `u.data.length` set by caller (input buffer size); driver may modify to report actual size |
| Flags field | `u.data.flags` = 16-bit OID/sub-command (see Table 2) |
| Return code | `0` = success, `-1` = error (errno set) |

### iwreq layout (wire format)

| Offset | Size | Field |
|---|---|---|
| 0x00 | IFNAMSIZ (32) | `ifr_name`: interface name, null-terminated |
| 0x20 | 4 | `ifr_flags`: general flags |
| 0x24 | 4 | `ifr_index`: interface index |
| 0x28 | 4 | `ifr_mtu`: MTU |
| 0x2C | 4 bytes + 16 bytes padding | `ifr_addr`: protocol address |
| 0x3C | 4 bytes + 16 bytes padding | `ifr_broadaddr`: broadcast address |
| 0x50 | 4 bytes + 16 bytes padding | `ifr_dstaddr`: destination address |
| 0x60 | 96 | `ifr_data`: generic pointer to data buffer (`u.data.pointer`) |

`u.data` sub-structure (96 bytes total, but effective content starts at pointer):

| Offset | Size | Field |
|---|---|---|
| 0x00 | 4 | `pointer`: kernel virtual address of user buffer (or userspace pointer) |
| 0x04 | 4 | `length`: input/output buffer size |
| 0x08 | 2 | `flags`: **OID sub-command** (see Table 2) |
| 0x0A | 2 | padding |

### Table 2 — OID / sub-command values placed in `u.data.flags`

| Value (dec) | Value (hex) | Meaning | Function |
|---|---|---|---|
| 2561 | 0x0a01 | Associated station list | `getAssociateStaList` |
| 2563 | 0x0a03 | Unassociated link metrics / timestamp path | `getUnassocStaLinkMetrics` |
| 2565 | 0x0a05 | Radio metrics | `getRadioMetrics` |
| 2820 | 0x0b04 | Scan results / BSS array | `getScanResult` |
| 2821 | 0x0b05 | RSSI (second ioctl) | `getRssi` |

---

## 2. Function 1: `getAssociateStaList`

### Function signature (from disassembly)

```c
int hal_multiap_mtk_getAssociateStaList(const char *ifname, void *out);
```

- `ifname`: interface name (e.g. `"rax0"`)
- `out`: caller-allocated output buffer; driver writes station entries
- **Return**: `0` on success, `-1` on error

### Output buffer layout

The driver writes up to **128 station entries**. Each entry is **640 bytes**.
Total maximum output: `4 + 128 × 640 = 81924 bytes`. The first 4 bytes are
a station count; each 640-byte record follows immediately after.

### Per-station entry (640 bytes, offsets relative to each entry)

The disassembly of `hal_multiap_mtk_getAssociateStaList` (0xaffc–0xb318)
reports a fixed **640-byte** record per station and a maximum of 128 records.
The output buffer begins with a **4-byte little-endian count**; the first
station record starts at byte 4 and each subsequent record is 640 bytes later:

```text
buffer[0..3]   = station_count
buffer[4 + i*640 .. 4 + (i+1)*640] = station_entry_i
```

The documented field offsets below are **per-entry** (relative to the start of
each 640-byte record):

| Offset | Size | Field |
|---|---|---|
| 0x00–0x07 | 8 | Uninterpreted header / padding |
| 0x08 | 6 | **Station MAC address** |
| 0x0E–0x0F | 2 | Padding (compiler-inserted) |
| 0x10 | 4 | 32-bit value; likely AID / RCPI |
| 0x14 | 36 | Traffic statistics |
| 0x38 | — | Link metrics, including the RSSI / RCPI value |
| 0x70 | 528 | Association request frame (through 0x27F) |

> **Note:** `getAssociateStaList` does **not** return the AP's BSSID or the
> station's SSID; those must be obtained separately (e.g. `getBSSID` or the
> GTPR `DEV2_WIFI_APDEV` tables).  The exact RCPI offset inside the link
> metrics block would require dynamic tracing on the device.

> **Note:** The 640-byte stride and these offsets were taken from the
> `libplatform_api.so` disassembly; they must be confirmed with a live `strace`
> or gdb session on the router before any production parsing is trusted.

---

## 3. Function 2: `getScanResult`

### Function signature (from disassembly)

```c
int hal_multiap_mtk_getScanResult(void *out, const char *ifname);
```

- `out`: caller-allocated output buffer; driver fills BSS entries
- `ifname`: interface name
- **Return**: `0` on success, `-1` on error

### Output buffer layout

The driver writes an array of **52-byte BSS entries**, maximum **128**.
The first 4 bytes of the buffer contain the **station count** (actual number
of BSS entries following).

Total maximum output: `4 + (128 × 52) = 4 + 6656 = 6660 bytes`.

### Per-BSS entry (52 bytes, offset 0x00–0x33 within the entry)

| Offset | Size | Field |
|---|---|---|
| 0x00 | 33 | SSID (NUL-terminated string; padded with nulls) |
| 0x21 | 6 | BSSID / MAC address of the Access Point |
| 0x28 | 4 | 32-bit value; likely RSSI (signal strength) or channel number |

> **Interpretation of the 4-byte field (offset 0x28 of each entry):**
> The analysis states: "offset 0x28 : 32-bit value (likely RSSI or channel)".
> Given the context of neighbor scanning and the values observed
> (104, 106, 108, 110 in the live router output — see §5), this field most
> probably encodes **RSSI** in the vendor's internal metric format, not raw
> dBm. The precise conversion will be addressed in §5.

### Summary

| Property | Value |
|---|---|
| Max entries | 128 |
| Entry size | 52 bytes |
| Count location | `buffer[0..3]` (first 4 bytes, big-endian or native) |
| Total max buffer | 6660 bytes |
| Input parameters | `ifname` only (no band/channel filter in this OID) |

---

## 4. Function 3: `getUnassocStaLinkMetrics`

### Function signature (from disassembly)

```c
int hal_multiap_mtk_getUnassocStaLinkMetrics(uint8_t band, uint8_t *staMac, void *out);
```

- `band`: radio band (0 = 2.4 GHz, 1 = 5 GHz, or vendor-specific encoding)
- `staMac`: **6-byte MAC address of the target station** (input; the driver
  measures RSSI of frames from this specific STA)
- `out`: caller-allocated output buffer; driver writes 24 bytes
- **Return**: `0` on success, `-1` on error

### Output buffer layout (24 bytes)

The driver writes exactly **24 bytes** containing the target STA's metadata.

| Offset | Size | Field |
|---|---|---|
| 0x00 | 6 | **staMac** — the same 6-byte MAC that was passed as input |
| 0x06 | 1 | **noise** — result of `radio_noise_check()` on the returned byte |
| 0x07–0x09 | 3 | 3 bytes from OID `0x0a05` (radio metrics) |
| 0x0A–0x1D | remaining bytes | interleaved/padded; full offset-level layout would require dynamic tracing to confirm |

> **Critical scope limitation:** This function requires the caller to **already
> know the STA MAC**. It is a *directed measurement*, not a discovery
> mechanism. Supplying an unknown or random MAC will not enumerate nearby
> devices; it will only measure (or fail to measure) the specified MAC.

---

## 5. Summary table

| Function | IOCTL | OID | Input | Output max | Key constraint |
|---|---|---|---|---|---|
| `getAssociateStaList` | 0x8be1 | 0x0a01 | ifname | 4 + 128 × 640 B (81924) | Returns associated stations + link metrics + RSSI per station |
| `getScanResult` | 0x8be1 | 0x0b04 | ifname | 4 + 128 × 52 B (6660) | Returns BSS table (APs); count in first 4 bytes; RSSI-like field per entry |
| `getUnassocStaLinkMetrics` | 0x8be1 | 0x0a03 | band + staMac | 24 B | **Directed measurement** — requires known MAC; does NOT enumerate unknown clients |

---

## 6. C-style struct definitions (for prototype use)

The following Rust/C types mirror the wire format. They are **documentation-only**
for this milestone; the prototype in §2 uses them as conceptual guides but does
not embed them in the binary (ioctl calls are not executed in the prototype).

### 6.1 `iwreq`-adjacent (common)

```c
#define IOCTL_MEDIATEK_GET_STA_LIST   _IOW('W', 0x0a01, struct station_entry_128)
#define IOCTL_MEDIATEK_GET_SCAN       _IOW('W', 0x0b04, struct scan_result_128)
#define IOCTL_MEDIATEK_GET_UNASSOC    _IOW('W', 0x0a03, struct unassoc_metrics_24)
```

### 6.2 Per-station entry (associated list)

```c
#define STA_ENTRIES_MAX 128
#define STA_ENTRY_SIZE 640

typedef struct __attribute__((packed, aligned(1))) {
    uint8_t  header[8];         // 0x00 — uninterpreted / padding
    uint8_t  mac[6];            // 0x08 — station MAC address
    uint8_t  pad1[2];           // 0x0e — compiler padding
    uint32_t aid_rcpi;          // 0x10 — 4 bytes, AID or RCPI
    uint8_t  traffic_stats[36]; // 0x14 — traffic counters
    uint8_t  link_metrics[56];  // 0x38 — includes RSSI / RCPI
    uint8_t  assoc_req[528];    // 0x70 — association request frame
} station_entry_t;
```

> **Note:** The total size is 640 bytes.  The exact RCPI field inside
> `link_metrics[56]` is not confirmed statically; confirm with on-device
> `strace -e trace=ioctl` once shell access is available.

### 6.3 Scan result entry

```c
#define SCAN_ENTRIES_MAX 128
#define SCAN_ENTRY_SIZE 52

typedef struct {
    char     ssid[33];          // 0x00 — SSID, NUL-terminated
    uint8_t  bssid[6];          // 0x21 — BSSID / AP MAC
    uint32_t rssi_metric;       // 0x28 — vendor RSSI metric (see §5)
} scan_entry_t;

typedef struct {
    uint32_t count;             // 0x00 — actual number of entries following
    scan_entry_t entries[SCAN_ENTRIES_MAX];
} scan_result_t;
```

### 6.4 Unassociated metrics (24 bytes)

```c
typedef struct {
    uint8_t  sta_mac[6];        // 0x00 — the input MAC (echoed)
    uint8_t  noise;             // 0x06 — radio_noise_check() result
    uint8_t  metrics[3];        // 0x07-0x09 — from OID 0x0a05
    // total = 24 bytes (any remaining padding to align or filler)
} unassoc_metrics_24_t;
```

---

## 7. Checklist for on-device validation (future, §7)

When shell access is available, the following sequence confirms the HAL is
functional:

1. `ioctl(fd, 0x8be1, &iwreq)` with `flags = 0x0a01` → verify `ret >= 0`
2. Read `buffer[0..3]` → should be `< 128` (actual station count)
3. For each station `i` from 0 to count-1:
   - Read entry at offset `4 + i * 640`
   - Verify `ssid` is not all-null
   - Read `rssi` at offset 0x38 within entry
4. Repeat with `flags = 0x0b04` → verify scan count + BSS entries
5. Repeat with `flags = 0x0a03`, `band = 0`, `staMac = <known>` → verify 24-byte output with echoed MAC

If any step returns `-1`, check: socket privileges, correct interface name
(`rax0` vs `ra0`), and that the driver module (`mt76`) is loaded.