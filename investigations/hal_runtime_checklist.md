# Detectic — MediaTek HAL Runtime Checklist (Stock EX520V)

> **One-page, 10-minute validation sheet.** Use this the first time a shell is
> obtained on the EX520V to confirm the MediaTek HAL can be used as a read-only
> data source for Detectic, without changing firmware.
>
> **Constraint reminder:** no firmware writes, no `iwpriv set`, no `doScan`,
> no unnecessary reboots.

---

## 1. What you need

### On the router

- A root shell (via the `REMOTE_ACCESS_OBJECTS.md` telnet/dropbear path or
  equivalent).
- The wireless driver module `mt76` loaded:
  ```bash
  lsmod | grep -E 'mt76|mt7915|mt7981'
  ```
- A writable scratch partition for the probe binary, e.g. `/var/run/misc_rw`
  or `/tmp`.

### On this workstation

- `prototypes/mtk_hal_probe` built for `aarch64-unknown-linux-musl`
  (see `.cargo/config.toml` for the target):
  ```bash
  cargo build --release --target aarch64-unknown-linux-musl
  ```
- `scp` or `curl` to copy the binary to the router.
- (Optional) `strace` installed on the router for `ioctl` verification.

---

## 2. Pre-flight checks (read-only)

Run these on the router before any `ioctl` is attempted.  All are safe and
non-destructive.

```bash
uname -a
# Expected: Linux, aarch64 / ARM64, MediaTek SoC reference

cat /proc/cpuinfo | head -5
# Expected: ARMv8, e.g. Cortex-A53

ls -l /sys/class/net
# Expected: interfaces such as lo, br-lan, eth0, ra0, rai0, rax0

iw dev
# Expected: at least one MediaTek phy/Interface

lsmod | grep mt76
# Expected: mt76, mt7915e, mt798x drivers present

# Verify the probe binary architecture and static linking
file /var/run/misc_rw/mtk_hal_probe
# Expected: ELF 64-bit LSB executable, ARM aarch64, version 1 (SYSV), statically linked
```

---

## 3. One-shot `ioctl` validation (under 10 minutes)

The probe binary performs three independent read-only `ioctl` calls.  Run it
and compare with the expected output.

```bash
# Copy the binary (one-time)
scp target/aarch64-unknown-linux-musl/release/mtk_hal_probe root@192.168.0.1:/var/run/misc_rw/

# Run the read-only probe
/var/run/misc_rw/mtk_hal_probe --validate
```

### 3.1 `getAssociateStaList` — OID `0x0a01`

```text
Expected outcome:
- ioctl(fd, 0x8be1, ...) returns >= 0
- First 4 bytes of the output buffer = a small integer N, 0 <= N < 128
- Each of the N 640-byte records:
  - bytes 0x08..0x0d are a non-zero 6-byte MAC
  - bytes 0x10..0x13 contain an RCPI/AID-like 32-bit value
  - bytes 0x38.. contain link metrics including the RCPI
```

If `N == 0` the band has no associated stations (valid; not an error).
If the call returns `-1`, check privileges and interface name (`ra0` vs `rax0`).

### 3.2 `getScanResult` — OID `0x0b04`

```text
Expected outcome:
- ioctl returns >= 0
- First 4 bytes = N, 0 <= N < 128
- Each of the N 52-byte records:
  - bytes 0x00..0x20 contain a NUL-padded SSID
  - bytes 0x21..0x26 are the AP BSSID
  - bytes 0x27..0x2a are a 4-byte RSSI/RCPI metric (values 0..127)
```

A value of `0` means no neighbor APs were in the scan cache.
The probe does **not** trigger a new scan; it only reads the existing cache.

### 3.3 `getUnassocStaLinkMetrics` — OID `0x0a03`

```text
Expected outcome for a KNOWN non-associated STA MAC:
- ioctl returns >= 0
- 24-byte output contains the echoed target MAC at offset 0x00
- remaining bytes contain channel/RSSI and a timestamp

Expected outcome for an UNKNOWN or random MAC:
- ioctl returns -1 or the 24-byte buffer is all zeros
```

This confirms the function is **directed**: it requires a known MAC and does
not enumerate nearby unknown clients.

---

## 4. Optional: verify with `strace`

If `strace` is available, capture the exact `ioctl` request and return value:

```bash
strace -e trace=ioctl -o /tmp/mtk_probe.trace /var/run/misc_rw/mtk_hal_probe --validate

cat /tmp/mtk_probe.trace | grep 0x8be1
# Expected lines: ioctl(fd, 0x8be1, {ifr_name="rax0", u.data.flags=0x0a01/0x0b04/0x0a03})
```

If `ioctl` fails with `EPERM` (Operation not permitted), the process lacks
sufficient privileges — run as `root` or add `CAP_NET_ADMIN`.

---

## 5. Read-only diagnostic commands

Use these if the probe returns unexpected results.  All are safe.

```bash
# Interface list and names
ip link show
iw dev

# Current channel and noise (driver-level, no scan trigger)
iw dev rax0 survey dump 2>/dev/null || true
iw dev ra0 station dump 2>/dev/null || true

# Confirm the driver private-ioctl range
ls -l /proc/driver/mt76* 2>/dev/null || true

# Wireless extensions info (if present)
iwpriv 2>/dev/null | head -5 || true
```

---

## 6. Safety / rollback

| Do this | Why |
|---|---|
| Work from `/var/run/misc_rw/` or `/tmp` | The original firmware partitions are untouched. |
| Run the probe as a non-persistent binary | No files in `/etc`, `/lib`, or `/usr` are modified. |
| Avoid `doScan` / `iwpriv set` | Those are writes; this milestone is read-only. |
| Stop the probe with `Ctrl-C` or `kill` | The process holds a socket but no driver state is altered. |
| Delete the probe binary when done | `rm /var/run/misc_rw/mtk_hal_probe` leaves the router exactly as found. |
| Do **not** run with `strace -f` unless necessary | Follow-fork mode can be noisy; the probe is single-threaded. |

If anything goes wrong, the router requires **no recovery action** beyond
stopping the process and deleting the probe binary.

---

## 7. Expected end-state

After 10 minutes you should be able to answer:

1. Does `ioctl(..., 0x8be1, ...)` with OID `0x0a01` return a 640-byte station
   count and records? → confirms `getAssociateStaList` layout.
2. Does OID `0x0b04` return 52-byte BSS entries? → confirms `getScanResult`
   is usable for neighbor AP discovery.
3. Does OID `0x0a03` fail or return zeros for an unknown MAC? → confirms
   `getUnassocStaLinkMetrics` is directed, not a discovery tool.
4. Are the live RCPI values in the 100–110 range? → confirms the RSSI
   semantics documented in `rssi_semantics.md`.

Record the output in `investigations/hal_runtime_notes.md` and attach the
`strace` log if available.
