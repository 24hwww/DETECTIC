# PHASE 14.7 — FORENSIC RESULT

## 1. Executive Summary

The Web UI on the EX520 uses the **same `so` (set-object) operation** as our CLI `detectic set` command. Both send `operation: "so"` to `/cgi_gdpr?9`. The JavaScript proxy (`proxy.js`) maps `$.dm.set()` directly to `operation: "so"`. There is **no hidden "apply" step** in the Web UI JavaScript for most settings including Telnet and Lifemote.

The critical architectural discovery is that **`rsl_set_dispatch` is a table-based dispatcher** with two function pointer slots per entry. The CGI handler in `httpd` calls `rdp_setObj` → `rsl_setObj` → `rsl_set_dispatch`, which iterates a 24-byte entry table and selects which handler to call based on an `is_update` flag derived from the object data itself. However, **the SET handler (`rsl_setDev2TelnetCfgObj`) only modifies in-memory config fields** — it does NOT call `oal_setTelnetd` to actually start the telnetd daemon.

The **actual service-start function** (`oal_setTelnetd`) resides in a **separate dispatch table entry** with a different object ID (0xbd30), and is not triggered by `so` operations on the Telnet config object (0x1765). This explains why both `so` + `ACT_SAVE_CFG` and post-reboot config persistence fail to start services.

**Live verification**: Setting `telnetLocalEnabled=1` via `so` and/or `ACT_SAVE_CFG` does NOT open port 23. The config is written and persists, but the service never starts.

---

## 2. Web UI Save Path — Reconstructed

### 2.1 JavaScript Layer

The Web UI framework (`proxy.js`) defines:

```javascript
// proxy.js lines 848-865
var dmMethod = new $.dm.Proxy({
    set:  { operation: "so" },     // $.dm.set() → operation "so"
    get:  { operation: "go" },     // $.dm.get() → operation "go"
    op:   { operation: "op" },     // $.dm.op()  → operation "op"
    cgi:  { operation: "cgi" },    // $.dm.cgi() → operation "cgi"
});
```

All requests go to `/cgi_gdpr?9` (AES-encrypted POST).

### 2.2 Web UI Telnet Save Sequence

From `manageCtrl.htm` t_save3 handler (line 1867):

```
1. Build telnetCfg = { telnetLocalEnabled, telnetLocalPort, ... }
2. $.dm.set({ oid: 'DEV2_TELNET_CFG', data: telnetCfg })
   → sends: {"operation":"so","data":{"oid":"DEV2_TELNET_CFG","telnetLocalEnabled":"1",...}}
   → endpoint: /cgi_gdpr?9 (AES encrypted)
3. On success: complete() → eventually $.reload('manageCtrl.htm')
```

**NO `ACT_SAVE_CFG` call. NO separate "apply" step.**

### 2.3 Firewall/ParentCtrl Exception

Only `firewall.htm` and `parentCtrl_v2.htm` call `ACT_SAVE_CFG` after `so`. The comment in `parentCtrl_v2.htm` line 2660 reads:

```javascript
/* Must save config here. See details in http_json.c. */
$.dm.op({ oid: "ACT_SAVE_CFG", ... });
```

This suggests `ACT_SAVE_CFG` is about flash persistence, NOT service application.

### 2.4 Key Finding

The Web UI and CLI use **identical operations**:

| Aspect | Web UI | CLI (`detectic set`) |
|--------|--------|---------------------|
| Operation | `so` | `so` |
| Endpoint | `/cgi_gdpr?9` | `/cgi_gdpr?9` |
| Auth | AES + GTPR | AES + GTPR |
| Data format | `{"operation":"so","data":{...}}` | `{"operation":"so","data":{...}}` |
| Apply step | None | None |
| Service starts? | **Unknown (needs browser test)** | **NO (proven)** |

---

## 3. Dispatch Table Architecture

### 3.1 `rsl_set_dispatch` Function (0x6513c, 532 bytes)

**Arguments:**
- x0: table pointer (array of 24-byte entries)
- x1: table count
- x2: obj_data pointer

**Algorithm:**
```
for i = 0 to count:
    entry = table + (i * 24)
    if entry.object_id == obj_data.object_id:
        flag = is_update_flag(obj_data)  // derived from obj_data fields
        if flag == 1:
            handler = entry.set_handler     // offset 8
        else:
            handler = entry.apply_handler   // offset 16
        if handler != NULL:
            result = handler(obj_data)      // BLR call
            if result == 6: return 0
            if result != 0: log error; return result
```

### 3.2 Entry Structure (24 bytes)

```
struct dispatch_entry {
    uint32_t object_id;    // offset 0: OID to match
    uint32_t tag;          // offset 4: 0x000b0012 (set) or 0x00160011 (other)
    void    *handler;      // offset 8: function pointer (SET handler)
    uint32_t handler_size; // offset 16: function size in bytes
    uint32_t pad;          // offset 20: 0
};
```

### 3.3 `is_update` Flag Derivation

From disassembly of `rsl_set_dispatch`:

```
if obj_data.first_2_bytes <= 0x2ea (746):
    flag = flag_func(obj_data)           // call function at 0x52e90
else:
    flag = (obj_data.field_at_offset_4 & 1) ? 1 : 0
```

**Critical**: The flag determines whether to call the SET handler (offset 8) or the APPLY handler (offset 16). For `so` operations on Telnet, the flag appears to select the SET handler.

### 3.4 Key Dispatch Table Entries Found

| File Offset | Object ID | Tag | Handler | Size | Identity |
|-------------|-----------|-----|---------|------|----------|
| 0x10848 | 0x2d88 | 0x000b0012 | 0x1baf8c | 0x158 (344) | `rsl_setDev2LifemoteAgentObj` |
| 0x13c98 | 0x1765 | 0x000b0012 | 0x10b8d0 | 0x650 (1616) | `rsl_setDev2TelnetCfgObj` |
| 0x17dc0 | 0xbd30 | 0x000b0012 | 0x1f9d64 | 0xb4 (180) | `oal_setTelnetd` |
| 0x19be0 | 0xa3fe | 0x000b0012 | 0x10b544 | 0x370 (880) | `rsl_initTelnetCfgObj` |
| 0x12ed8 | 0x2db3 | 0x000b0012 | 0x1bb0e4 | 0xbc (188) | `rsl_initDev2LifemoteAgentObj` |

**Key observation**: `rsl_setDev2TelnetCfgObj` (SET handler for Telnet config) and `oal_setTelnetd` (the function that actually starts telnetd) are in **separate dispatch entries with different object IDs**. They are NOT two handlers of the same entry.

---

## 4. Apply Handler Call Graph

### 4.1 `rsl_setDev2TelnetCfgObj` (0x10b8d0, 1616 bytes) — SET Handler

This function **only modifies config fields** in memory. It does NOT call `oal_setTelnetd`.

```
rsl_setDev2TelnetCfgObj(obj_data)
  ├── sub_51320()           // utility/logging
  ├── sub_51820() x6        // utility/logging
  ├── sub_104178()          // internal
  ├── sub_52650()           // internal
  ├── sub_104a08()          // internal
  ├── sub_54c60()           // internal
  ├── sub_50010() x7        // utility
  ├── sub_524d0() x2        // utility
  ├── sub_50a70() x2        // utility
  └── sub_50670()           // utility
```

**NO call to `oal_setTelnetd`, `oal_app_setLocalTelnetAccess`, or `oal_app_setRemoteTelnetAccess`.**

### 4.2 `oal_setTelnetd` (0x1f9d64, 180 bytes) — Service Starter

```c
// Disassembly reconstruction
void oal_setTelnetd(uint8_t enable, uint16_t port, uint16_t mode) {
    char cmd[256];
    memset(cmd, 0, sizeof(cmd));
    
    if (mode == 0) return;  // early return if mode is 0
    
    // Build command: "telnetd -p %d"
    snprintf(cmd, 256, "telnetd -p %d", port);
    system(cmd);  // START telnetd!
    
    // If mode == 0x17 (23): also do something additional
    if (mode == 23) {
        snprintf(cmd, 256, ...additional command...);
        system(cmd);
    }
    
    if (enable) {
        // "telnetd -p %d &" pattern at 0x29c188
        // Start telnetd in background
    }
    
    return 0;
}
```

**String references confirm**: `"telnetd -p %d"`, `"telnetd -p %d &"`, `"telnetd"`

### 4.3 `oal_app_setLocalTelnetAccess` (0x1f9cf0, 116 bytes)

```
oal_app_setLocalTelnetAccess()
  ├── sub_51100()    // first call
  └── sub_51100()    // second call
```

This is a thin wrapper that calls the same utility twice (likely for two different telnet-related operations).

---

## 5. IPC / Event / Message Findings

### 5.1 cos Daemon Message Loop

From cos binary strings:

```
cos_init flow:
  sys_init → ipt_init → initMiscIsp → dm_init → dm_getObj → dm_postHook
  → os_threadCreate → rdp_init → msg_init → msg_srvInit → [main loop]
  
Main loop:
  msg_recv → rdp_action → [dispatch handler]
```

**Event types in cos:**
- `EVENT_CONFIG`
- `EVENT_DETECT`
- `EVENT_LINK`
- `EVENT_MESH`
- `EVENT_TIMER`
- `EVENT_ADDRESS`

### 5.2 IPC Mechanism

Both `httpd` and `cos` link to `libcmm.so` and share:
- `msg_init`, `msg_srvInit`, `msg_recv`
- `msg_connCliAndSend`, `msg_easySendMsg`, `msg_sendRequest`
- IPC socket: `/var/tmp/apdev_msg_send`

### 5.3 httpd → cos Communication

The httpd binary has:
- `rdp_action` — message handler (same as cos)
- `rdp_setObj` — object setter
- `msg_connCliAndSend` — IPC client send
- `msg_recv` — message receive
- `msg_srvInit` — server initialization

**This means httpd CAN send IPC messages to cos.** The `rdp_setObj` call within httpd's CGI handler may internally send a message to cos via the IPC mechanism.

### 5.4 Handler Registration in cos

cos has 128+ handler functions registered as callbacks, including:
- `wifiWpsCfgChangedHandler`
- `wanConnActiveHandler`
- `dnsServerHandler`
- `resetBtnPressedHandler`
- etc.

**NO telnet-specific handler found** in cos handler list. This is significant — it means telnet service management may be handled entirely within `httpd` or through a generic config-apply mechanism.

---

## 6. `cos` Findings

### 6.1 What `cos` Does

```text
cos_init:
  1. dm_init()          — Load all config from flash into shared memory
  2. dm_getObj()        — Retrieve specific objects
  3. dm_postHook()      — Post-processing hook (BUT DOES NOT trigger per-object apply handlers)
  4. rdp_init()         — Initialize RDP layer
  5. msg_init/SrvInit   — Set up IPC server
  6. Main loop: msg_recv → rdp_action → handle events/messages
```

### 6.2 What `cos` Does NOT Do

- Does NOT call `rsl_setDev2TelnetCfgObj` on config load
- Does NOT call `oal_setTelnetd` on config load
- Does NOT directly start/stop services based on config values
- `dm_postHook` does NOT dispatch to per-object apply handlers

### 6.3 cos Runtime Activities

cos processes events through its message loop:
- Timer events (`EVENT_TIMER`)
- Network state changes (`EVENT_LINK`, `EVENT_DETECT`)
- EasyMesh operations
- DHCP/WAN state changes
- But **NO config-apply events** for Telnet/Lifemote

---

## 7. Definitive Classification

| Mechanism | Classification | Evidence |
|-----------|---------------|----------|
| `ACT_SAVE_CFG` persists to flash | **PROVEN-LIVE** | Phase 14.5: config survives reboot |
| `cos` loads config from flash at boot | **PROVEN-LIVE** | Phase 14.5: GTPR readable after reboot |
| `dm_postHook` triggers apply handlers | **DISPROVEN** | Phase 14.6: Telnet + Lifemote both tested |
| `so` triggers service start | **DISPROVEN** | Live test: `so` Telnet → port23 closed |
| `so` + `ACT_SAVE_CFG` triggers service start | **DISPROVEN** | Phase 14.6: port23 closed |
| Post-reboot config → service start | **DISPROVEN** | Phase 14.5: `state=0` after reboot |
| `rsl_setDev2TelnetCfgObj` starts telnetd | **DISPROVEN** | Static analysis: no call to `oal_setTelnetd` |
| Web UI `so` differs from CLI `so` | **DISPROVEN** | Both send identical `/cgi_gdpr?9` requests |
| httpd sends IPC to cos for service mgmt | **STRONGLY INDICATED** | httpd has `msg_connCliAndSend`, `rdp_action` |
| `oal_setTelnetd` called by dispatch | **UNCONFIRMED** | In separate dispatch entry (obj_id 0xbd30), never triggered by `so` on Telnet config |

---

## 8. Missing Mechanism

The apply mechanism remains **UNRESOLVED** for the Telnet and Lifemote services. The architecture shows:

```text
Web UI Save
  ↓
httpd: rdp_setObj("DEV2_TELNET_CFG")
  ↓
rsl_set_dispatch → rsl_setDev2TelnetCfgObj  (modifies config fields only)
  ↓
[???]  ← MISSING: no service restart triggered
  ↓
oal_setTelnetd  (never reached)
```

**Possible explanations:**

1. **httpd sends an IPC message to cos after `rdp_setObj`** — cos would need to process it and call `oal_setTelnetd` via a different dispatch path. Evidence: httpd has `msg_connCliAndSend` and `rdp_action`.

2. **The telnetd daemon is managed separately** — possibly started at boot by an init script and controlled via config-only (e.g., iptables rules or wrapper scripts), not by the `oal_setTelnetd` function directly.

3. **The Web UI Telnet enable also doesn't start the service in real-time** — it may require a reboot, and the Phase 14.5 test showed even reboot doesn't start it (perhaps because the init mechanism differs from the `dm_postHook` hypothesis).

4. **There is a separate service management daemon** — not `cos`, not `httpd` — that watches config changes and applies them. Evidence: cos has `checkAndRestartDetectProcess` in its strings, suggesting a polling/checking mechanism.

---

## 9. Detectic Relevance

### Impact on Detectic Architecture

This discovery **does NOT change the fundamental Detectic architecture**. The implications are:

1. **The GTPR/GDPR API remains the proven management path** for reading config and device data.

2. **Config persistence via `so` + `ACT_SAVE_CFG` works** — we can persist settings to flash.

3. **Service startup through config changes is unreliable via API** — the `so` operation modifies config but doesn't guarantee service reconfiguration.

4. **For Detectic deployment**, we should NOT rely on config-driven service management (e.g., enabling Telnet/Lifemote via `so`). Instead:
   - Use the GTPR/GDPR API for **read-only observation** (proven)
   - Use `DEV2_WIFI_APDEV_ASSOCDEV` for device detection (proven)
   - Avoid depending on `so`/`ACT_SAVE_CFG` to start services

5. **The sensor binary should be self-contained** — it should not rely on router-side service management through config objects.

### Recommended Detectic Approach

```text
STAY WITH:
  ✓ GTPR/GDPR read-only API
  ✓ DEV2_WIFI_APDEV_ASSOCDEV polling
  ✓ External HTTP server for sensor data

AVOID:
  ✗ Enabling services via so + ACT_SAVE_CFG
  ✗ Relying on config persistence for service startup
  ✗ Trying to start processes through config objects
```

---

## 10. Next Phase Recommendation

### Phase 14.8 — Finalize Read-Only Sensor Path

Given that config-driven service management is unreliable, the recommended next step is:

1. **Accept the read-only GTPR/GDPR path as the proven sensor interface**
2. **Build the Detectic sensor around `DEV2_WIFI_APDEV_ASSOCDEV` polling**
3. **Deploy the sensor binary externally** (not via router-side execution)
4. **Focus on the HTTP server approach** — run a lightweight sensor on a host machine that polls the EX520 via GTPR/GDPR

This avoids the unsolved apply-handler mystery entirely while still achieving the Detectic MVP.

### Alternative Investigation

If router-side execution is essential, investigate:

1. **The `httpd` IPC path**: Does `rdp_setObj` in httpd send a message to cos? If so, what message type?
2. **The `checkAndRestartDetectProcess` function in cos**: Is this a polling mechanism that checks config and starts services?
3. **An init.d script or procd service**: Could telnetd be started by a different mechanism than the dispatch table?
4. **Direct process execution**: Could the Detectic binary simply call `system("...")` to start processes, bypassing the config system entirely?

---

## Appendix A: Evidence Index

| ID | Description | Classification |
|----|-------------|----------------|
| E-14.7-01 | Web UI proxy.js maps `$.dm.set()` to `operation: "so"` | PROVEN-STATIC |
| E-14.7-02 | `rsl_set_dispatch` table structure: 24-byte entries with SET/APPLY handlers | PROVEN-STATIC |
| E-14.7-03 | `rsl_setDev2TelnetCfgObj` does NOT call `oal_setTelnetd` | PROVEN-STATIC |
| E-14.7-04 | `oal_setTelnetd` executes `system("telnetd -p %d")` | PROVEN-STATIC |
| E-14.7-05 | `oal_setTelnetd` is in separate dispatch entry (obj_id 0xbd30) | PROVEN-STATIC |
| E-14.7-06 | `so` + `ACT_SAVE_CFG` does NOT open port23 | PROVEN-LIVE |
| E-14.7-07 | httpd has `msg_connCliAndSend` and `rdp_action` | PROVEN-STATIC |
| E-14.7-08 | cos has `EVENT_CONFIG`, `EVENT_TIMER` etc. in message loop | PROVEN-STATIC |
| E-14.7-09 | cos has 128 handler functions, none telnet-specific | PROVEN-STATIC |
| E-14.7-10 | Web UI Telnet save uses `so` only, no `ACT_SAVE_CFG` | PROVEN-STATIC |

---

## Appendix B: Dispatch Table Function Address Map

```
Symbol                              Address    Size
─────────────────────────────────────────────────────
rsl_set_dispatch                    0x0006513c 532
rsl_initTelnetCfgObj                0x0010b544 880
rsl_getDev2TelnetCfgObj             0x0010b8b4 28
rsl_setDev2TelnetCfgObj             0x0010b8d0 1616
rsl_initDev2LifemoteAgentObj        0x001bb0e4 188
rsl_getDev2LifemoteAgentObj         0x001baee4 168
rsl_setDev2LifemoteAgentObj         0x001baf8c 344
oal_setTelnetd                      0x001f9d64 180
oal_app_setLocalTelnetAccess        0x001f9cf0 116
oal_app_setRemoteTelnetAccess       0x001f98d0 1056
msg_sendRequest                     0x0022f9ec -
```
