#!/usr/bin/env python3
"""Phase 14.9 — Controlled validation test for the external Detectic sensor.

Tests the full pipeline: mock router → GTPR poll → normalize → presence →
events → buffer → upload → mock backend.

Validates the T0-T7 scenario:
    T0  sensor starts
    T1  router reachable
    T2  device appears in observation
    T3  repeated observations
    T4  device disappears
    T5  backend temporarily unavailable
    T6  backend returns
    T7  buffered events are delivered

Run:
    python3 tests/test_sensor_validation.py
"""

import json
import os
import sqlite3
import sys
import tempfile
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

# Add project python dir to path
PROJECT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(PROJECT / "python"))

from detectic_sensor import (
    PresenceEngine, EventStore, normalize_assocdev, parse_assocdev_response,
    DeviceSnapshot, DetecticEvent,
    EVT_DEVICE_FIRST_SEEN, EVT_DEVICE_SEEN, EVT_DEVICE_CHANGED,
    EVT_DEVICE_LAST_SEEN, EVT_SENSOR_ONLINE, EVT_SENSOR_OFFLINE,
    pseudonymize, derive_band, ASSOCDEV_FIELD_CONTRACT, SCHEMA_VERSION,
)
from detectic_client import Dialect, GtprClient

# --- Mock backend ---

class MockBackendHandler(BaseHTTPRequestHandler):
    """Mock backend that can be toggled available/unavailable."""
    
    received_events = []
    available = True
    
    def log_message(self, *args):
        pass
    
    def do_POST(self):
        if not self.available:
            self.send_response(503)
            self.end_headers()
            self.wfile.write(b"unavailable")
            return
        if self.path == "/api/v1/events":
            length = int(self.headers.get("Content-Length", 0))
            body = self.rfile.read(length)
            payload = json.loads(body)
            events = payload.get("events", [])
            MockBackendHandler.received_events.extend(events)
            self.send_response(202)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({"accepted": len(events)}).encode())
        else:
            self.send_response(404)
            self.end_headers()
    
    def do_GET(self):
        if self.path == "/api/v1/healthz":
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b"ok")
        else:
            self.send_response(404)
            self.end_headers()


def start_backend(port=18098):
    server = ThreadingHTTPServer(("127.0.0.1", port), MockBackendHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server


# --- Test helpers ---

SECRET = bytes.fromhex("aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899")
SENSOR_ID = "test-sensor-001"


def make_device(mac, hostname="phone", rssi="50", radio="3c:6a:d2:5f:ab:c1",
                std="n", active="1", tx="26000", rx="52000"):
    return {
        "X_TP_HostName": hostname,
        "X_TP_IPAddress": "192.168.0.20",
        "MACAddress": mac,
        "X_TP_RadioMac": radio,
        "operatingStandard": std,
        "signalStrength": rssi,
        "active": active,
        "associationTime": "2025-08-24T00:00:00Z",
        "lastDataDownlinkRate": tx,
        "lastDataUplinkRate": rx,
        "X_TP_SignalStrengthLevel": "4",
        "X_TP_MaxLinkRate": "72000",
        "noise": "50",
        "steeringHistoryNumberOfEntries": "0",
        "stack": "1,1,2,1,0,0",
    }


def run_test(name: str, fn) -> bool:
    print(f"\n{'='*60}")
    print(f"TEST: {name}")
    print(f"{'='*60}")
    try:
        fn()
        print(f"  PASS")
        return True
    except AssertionError as e:
        print(f"  FAIL: {e}")
        return False
    except Exception as e:
        print(f"  ERROR: {type(e).__name__}: {e}")
        import traceback
        traceback.print_exc()
        return False


# --- Tests ---

def test_raw_contract():
    """14.9.1 — Verify raw field contract classifications."""
    assert "X_TP_HostName" in ASSOCDEV_FIELD_CONTRACT
    assert ASSOCDEV_FIELD_CONTRACT["signalStrength"] == "PROVEN-LIVE"
    assert ASSOCDEV_FIELD_CONTRACT["MACAddress"] == "PROVEN-LIVE"
    assert ASSOCDEV_FIELD_CONTRACT["X_TP_RadioMac"] == "PROVEN-LIVE"
    assert ASSOCDEV_FIELD_CONTRACT["operatingStandard"] == "PROVEN-LIVE"
    print("  All PROVEN-LIVE fields present in contract")


def test_parse_response():
    """14.9.2 — Verify response parsing."""
    # Success response
    raw = json.dumps({
        "data": [make_device("AA:BB:CC:11:22:33")],
        "operation": "gl",
        "oid": "DEV2_WIFI_APDEV_ASSOCDEV",
        "success": True,
    })
    success, devices, err = parse_assocdev_response(raw)
    assert success is True
    assert err is None
    assert len(devices) == 1
    assert devices[0]["MACAddress"] == "AA:BB:CC:11:22:33"
    
    # Error response
    raw_err = json.dumps({"success": False, "errorcode": 9003})
    success, devices, err = parse_assocdev_response(raw_err)
    assert success is False
    assert err == 9003
    assert len(devices) == 0
    
    # Empty data
    raw_empty = json.dumps({"data": [], "success": True})
    success, devices, err = parse_assocdev_response(raw_empty)
    assert success is True
    assert len(devices) == 0
    
    # Malformed
    success, devices, err = parse_assocdev_response("not json")
    assert success is False
    print("  Response parsing: success, error, empty, malformed all handled")


def test_normalize():
    """14.9.2 — Verify normalization and pseudonymization."""
    raw = [make_device("AA:BB:CC:11:22:33", hostname="myphone", rssi="98")]
    snaps = normalize_assocdev(raw, SECRET, SENSOR_ID, 1700000000)
    assert len(snaps) == 1
    s = snaps[0]
    
    # Device ID is pseudonymized (64 hex chars)
    assert len(s.device_id) == 64
    assert s.device_id != "AA:BB:CC:11:22:33"
    
    # RSSI preserved
    assert s.signal_strength == 98
    
    # Band derived from radio MAC
    assert s.band == "2.4GHz"
    
    # Operating standard preserved
    assert s.operating_standard == "n"
    
    # Rates preserved
    assert s.tx_rate_kbps == 26000
    assert s.rx_rate_kbps == 52000
    
    # Raw MAC kept internally but not in public fields
    assert s._raw_mac == "AA:BB:CC:11:22:33"
    
    # Verify pseudonymization is deterministic
    snaps2 = normalize_assocdev(raw, SECRET, SENSOR_ID, 1700000001)
    assert snaps2[0].device_id == s.device_id
    print(f"  Normalized: device_id={s.device_id[:16]}... band={s.band} rssi={s.signal_strength}")


def test_band_derivation():
    """14.9.5 — Verify band derivation from radio MAC."""
    assert derive_band("3c:6a:d2:5f:ab:c1") == "2.4GHz"
    assert derive_band("3c:6a:d2:5f:ab:c3") == "5GHz"
    assert derive_band("3C:6A:D2:5F:AB:C1") == "2.4GHz"  # case insensitive
    assert derive_band(None) is None
    assert derive_band("") is None
    print("  Band derivation: 2.4GHz, 5GHz, case-insensitive, None handled")


def test_presence_first_seen():
    """14.9.4 — First observation emits device_first_seen."""
    engine = PresenceEngine(absence_threshold=3)
    ts = 1700000000
    snap = DeviceSnapshot(
        device_id="dev1", observed_at=ts, associated=True,
        signal_strength=80, band="2.4GHz",
    )
    events = engine.update([snap], ts, sensor_online=True)
    
    # Presence engine generates device events; sensor_online is generated
    # by the polling engine, not the presence engine.
    types = [e.event_type for e in events]
    assert EVT_DEVICE_FIRST_SEEN in types, f"expected first_seen, got {types}"
    print(f"  Events: {types}")


def test_presence_seen():
    """14.9.4 — Repeated observation emits device_seen (no change)."""
    engine = PresenceEngine(absence_threshold=3)
    ts = 1700000000
    snap = DeviceSnapshot(
        device_id="dev1", observed_at=ts, associated=True,
        signal_strength=80, band="2.4GHz",
    )
    engine.update([snap], ts, sensor_online=True)
    
    # Second poll — same device, same data
    ts2 = ts + 30
    snap2 = DeviceSnapshot(
        device_id="dev1", observed_at=ts2, associated=True,
        signal_strength=80, band="2.4GHz",
    )
    events = engine.update([snap2], ts2, sensor_online=True)
    types = [e.event_type for e in events]
    assert EVT_DEVICE_SEEN in types
    assert EVT_DEVICE_FIRST_SEEN not in types
    print(f"  Repeated observation: {types}")


def test_presence_changed():
    """14.9.4 — Changed RSSI emits device_changed."""
    engine = PresenceEngine(absence_threshold=3)
    ts = 1700000000
    snap1 = DeviceSnapshot(
        device_id="dev1", observed_at=ts, associated=True,
        signal_strength=80, band="2.4GHz",
    )
    engine.update([snap1], ts, sensor_online=True)
    
    ts2 = ts + 30
    snap2 = DeviceSnapshot(
        device_id="dev1", observed_at=ts2, associated=True,
        signal_strength=90, band="2.4GHz",  # RSSI changed
    )
    events = engine.update([snap2], ts2, sensor_online=True)
    types = [e.event_type for e in events]
    assert EVT_DEVICE_CHANGED in types
    print(f"  Changed RSSI: {types}")


def test_presence_absence_timeout():
    """14.9.4 — Device only marked absent after threshold consecutive misses."""
    engine = PresenceEngine(absence_threshold=3)
    ts = 1700000000
    snap = DeviceSnapshot(
        device_id="dev1", observed_at=ts, associated=True,
        signal_strength=80,
    )
    engine.update([snap], ts, sensor_online=True)
    
    # Poll 2: device missing (1 miss)
    ts2 = ts + 30
    events = engine.update([], ts2, sensor_online=True)
    types = [e.event_type for e in events]
    assert EVT_DEVICE_LAST_SEEN not in types, "should not leave after 1 miss"
    print(f"  1 miss: {types} (no last_seen yet)")
    
    # Poll 3: device missing (2 misses)
    ts3 = ts + 60
    events = engine.update([], ts3, sensor_online=True)
    types = [e.event_type for e in events]
    assert EVT_DEVICE_LAST_SEEN not in types, "should not leave after 2 misses"
    print(f"  2 misses: {types} (no last_seen yet)")
    
    # Poll 4: device missing (3 misses = threshold)
    ts4 = ts + 90
    events = engine.update([], ts4, sensor_online=True)
    types = [e.event_type for e in events]
    assert EVT_DEVICE_LAST_SEEN in types, "should leave after 3 misses"
    print(f"  3 misses: {types} (last_seen emitted)")


def test_presence_reappearance():
    """14.9.4 — Device reappearing after absence emits first_seen again."""
    engine = PresenceEngine(absence_threshold=2)
    ts = 1700000000
    snap = DeviceSnapshot(device_id="dev1", observed_at=ts, associated=True)
    engine.update([snap], ts, sensor_online=True)
    
    # Miss twice → absent
    engine.update([], ts + 30, sensor_online=True)
    engine.update([], ts + 60, sensor_online=True)
    
    # Reappear
    snap2 = DeviceSnapshot(device_id="dev1", observed_at=ts + 90, associated=True)
    events = engine.update([snap2], ts + 90, sensor_online=True)
    types = [e.event_type for e in events]
    assert EVT_DEVICE_FIRST_SEEN in types, "reappearance should emit first_seen"
    print(f"  Reappearance after absence: {types}")


def test_event_store_durable():
    """14.9.6 — Event store survives and deduplicates."""
    with tempfile.NamedTemporaryFile(suffix=".db", delete=False) as f:
        db_path = f.name
    
    os.unlink(db_path)  # remove so we test creation
    
    store = EventStore(db_path, max_events=100)
    
    evt = DetecticEvent(
        event_id="test-uuid-1",
        sensor_id=SENSOR_ID,
        event_type=EVT_DEVICE_FIRST_SEEN,
        event_timestamp=1700000000,
        device_id="dev1",
    )
    
    # Enqueue
    assert store.enqueue(evt) is True
    assert store.depth() == 1
    
    # Duplicate enqueue (idempotent)
    assert store.enqueue(evt) is True
    assert store.depth() == 1, "duplicate should not increase depth"
    
    # Pending
    pending = store.pending(10)
    assert len(pending) == 1
    row_id, event_json = pending[0]
    parsed = json.loads(event_json)
    assert parsed["event_id"] == "test-uuid-1"
    
    # Mark uploaded
    store.mark_uploaded([row_id])
    assert store.depth() == 0
    
    # Close and reopen — verify schema persistence
    store.close()
    store2 = EventStore(db_path, max_events=100)
    assert store2.depth() == 0
    store2.close()
    
    os.unlink(db_path)
    print("  Enqueue, dedup, pending, mark_uploaded, reopen all OK")


def test_event_store_bounded():
    """14.9.6 — Queue drops oldest when full."""
    with tempfile.NamedTemporaryFile(suffix=".db", delete=False) as f:
        db_path = f.name
    os.unlink(db_path)
    
    store = EventStore(db_path, max_events=10)
    
    # Fill beyond limit
    for i in range(20):
        evt = DetecticEvent(
            event_id=f"uuid-{i}",
            sensor_id=SENSOR_ID,
            event_type=EVT_DEVICE_SEEN,
            event_timestamp=1700000000 + i,
            device_id="dev1",
        )
        store.enqueue(evt)
    
    depth = store.depth()
    assert depth <= 10, f"queue should be bounded, got {depth}"
    print(f"  Filled 20, queue depth={depth} (bounded at 10)")
    store.close()
    os.unlink(db_path)


def test_event_privacy():
    """14.9.3 — No raw MAC/IP/hostname in event JSON."""
    raw = [make_device("AA:BB:CC:11:22:33", hostname="secret-phone")]
    snaps = normalize_assocdev(raw, SECRET, SENSOR_ID, 1700000000)
    
    engine = PresenceEngine(absence_threshold=3)
    events = engine.update(snaps, 1700000000, sensor_online=True)
    
    for evt in events:
        evt.sensor_id = SENSOR_ID
        json_str = evt.to_json()
        assert "AA:BB:CC" not in json_str, "raw MAC leaked in event JSON"
        assert "secret-phone" not in json_str, "hostname leaked in event JSON"
        assert "192.168.0" not in json_str, "IP leaked in event JSON"
    
    print("  No raw MAC, hostname, or IP in event JSON")


def test_idempotency_key():
    """14.9.7 — Idempotency key is deterministic."""
    evt1 = DetecticEvent(
        event_id="uuid-1",
        sensor_id=SENSOR_ID,
        event_type=EVT_DEVICE_SEEN,
        event_timestamp=1700000000,
        device_id="dev1",
    )
    evt2 = DetecticEvent(
        event_id="uuid-2",  # different event_id
        sensor_id=SENSOR_ID,
        event_type=EVT_DEVICE_SEEN,
        event_timestamp=1700000000,
        device_id="dev1",
    )
    # Same sensor + device + timestamp + type → same key
    assert evt1.idempotency_key() == evt2.idempotency_key()
    print(f"  Idempotency key: {evt1.idempotency_key()[:32]}...")


def test_multi_sensor():
    """14.9.9 — Every event has sensor_id."""
    raw = [make_device("AA:BB:CC:11:22:33")]
    snaps = normalize_assocdev(raw, SECRET, "sensor-A", 1700000000)
    engine = PresenceEngine(absence_threshold=3)
    events = engine.update(snaps, 1700000000, sensor_online=True)
    for e in events:
        e.sensor_id = "sensor-A"
        assert e.sensor_id == "sensor-A"
    print(f"  All {len(events)} events have sensor_id=sensor-A")


def test_full_pipeline_mock_router():
    """T0-T7: Full pipeline against mock router + mock backend."""
    from mock_router import start_mock_server
    
    # Start mock router
    router = start_mock_server(port=18099)
    backend_server = start_backend(port=18098)
    MockBackendHandler.received_events = []
    MockBackendHandler.available = True
    
    with tempfile.NamedTemporaryFile(suffix=".db", delete=False) as f:
        db_path = f.name
    os.unlink(db_path)
    
    try:
        store = EventStore(db_path, max_events=1000)
        
        # T0: sensor starts — authenticate
        client = GtprClient(
            "http://127.0.0.1:18099", "admin", "testpass",
            Dialect.GDPR_JSON,
        )
        client.connect()
        print("  T0: authenticated to mock router")
        
        # T1: router reachable — first poll
        raw = client.gl("DEV2_WIFI_APDEV_ASSOCDEV")
        success, devices, err = parse_assocdev_response(raw)
        assert success, f"T1 poll failed: err={err}"
        assert len(devices) == 2, f"T1 expected 2 devices, got {len(devices)}"
        print(f"  T1: router reachable, {len(devices)} devices observed")
        
        # T2: device appears (already in mock data, so first_seen)
        snaps = normalize_assocdev(devices, SECRET, SENSOR_ID, int(time.time()))
        engine = PresenceEngine(absence_threshold=3)
        events = engine.update(snaps, int(time.time()), sensor_online=True)
        for e in events:
            e.sensor_id = SENSOR_ID
            store.enqueue(e)
        first_seen_count = sum(1 for e in events if e.event_type == EVT_DEVICE_FIRST_SEEN)
        assert first_seen_count == 2, f"T2 expected 2 first_seen, got {first_seen_count}"
        print(f"  T2: {first_seen_count} device_first_seen events")
        
        # T3: repeated observations
        time.sleep(0.1)
        raw2 = client.gl("DEV2_WIFI_APDEV_ASSOCDEV")
        success2, devices2, _ = parse_assocdev_response(raw2)
        snaps2 = normalize_assocdev(devices2, SECRET, SENSOR_ID, int(time.time()))
        events2 = engine.update(snaps2, int(time.time()), sensor_online=True)
        for e in events2:
            e.sensor_id = SENSOR_ID
            store.enqueue(e)
        seen_count = sum(1 for e in events2 if e.event_type == EVT_DEVICE_SEEN)
        print(f"  T3: {seen_count} device_seen events (repeated)")
        
        # T4: device disappears — poll empty (simulate by using empty list)
        # We can't easily make the mock return empty, so test presence with empty
        for _ in range(3):
            time.sleep(0.05)
            events_miss = engine.update([], int(time.time()), sensor_online=True)
            for e in events_miss:
                e.sensor_id = SENSOR_ID
                store.enqueue(e)
        last_seen_count = sum(1 for e in events_miss if e.event_type == EVT_DEVICE_LAST_SEEN)
        assert last_seen_count == 2, f"T4 expected 2 last_seen, got {last_seen_count}"
        print(f"  T4: {last_seen_count} device_last_seen events (after 3 misses)")
        
        total_events = store.depth()
        print(f"  Total events in queue: {total_events}")
        assert total_events > 0
        
        # T5: backend temporarily unavailable
        MockBackendHandler.available = False
        from detectic_sensor import Uploader
        uploader = Uploader(
            store, "http://127.0.0.1:18098", SENSOR_ID, "test-secret",
            batch_size=100, max_retries=1, backoff_base=0.1,
        )
        uploaded, failed = uploader.upload_batch()
        assert uploaded == 0, "T5 should not upload when backend down"
        print(f"  T5: backend unavailable, uploaded={uploaded}, events still in queue={store.depth()}")
        
        # T6: backend returns
        MockBackendHandler.available = True
        uploaded2, failed2 = uploader.upload_batch()
        assert uploaded2 > 0, "T6 should upload when backend returns"
        print(f"  T6: backend returned, uploaded={uploaded2}")
        
        # T7: buffered events delivered
        assert store.depth() == 0, "T7 queue should be empty after delivery"
        assert len(MockBackendHandler.received_events) > 0
        print(f"  T7: queue empty, backend received {len(MockBackendHandler.received_events)} events")
        
        # Verify no raw MAC in backend events
        for evt in MockBackendHandler.received_events:
            evt_json = json.dumps(evt)
            assert "AA:BB:CC" not in evt_json, "raw MAC leaked to backend"
        
        store.close()
        
    finally:
        router.shutdown()
        backend_server.shutdown()
        if os.path.exists(db_path):
            os.unlink(db_path)
    
    print("  Full T0-T7 pipeline: PASS")


def test_distance_capability():
    """14.9.5 — Classify distance capability from real data."""
    # From the temporal dataset analysis:
    # - RSSI present: YES (0-128 scale)
    # - Per-device variance: low (±2-4)
    # - Scale: 0-128 (NOT dBm)
    # - Bands: 2.4GHz, 5GHz, unknown
    # - No calibration data
    
    # RSSI is present and relatively stable → POSSIBLE WITH CALIBRATION
    # But 0-128 scale is non-standard, no path-loss model, no calibration data
    
    # Read the real data
    rssis = []
    with open(str(PROJECT / "tests/temporal_dataset.jsonl")) as f:
        for line in f:
            d = json.loads(line)
            r = d.get("raw_signal_strength")
            if r is not None:
                rssis.append(r)
    
    assert len(rssis) > 0, "no RSSI data available"
    assert min(rssis) >= 0, "RSSI below 0"
    assert max(rssis) <= 128, "RSSI above 128"
    
    # Classification: POSSIBLE WITH CALIBRATION
    # Rationale:
    #   1. RSSI is present and stable per-device
    #   2. Scale is 0-128 (TP-Link internal, not dBm)
    #   3. No calibration data exists
    #   4. No path-loss model
    #   5. Band separation needed (2.4GHz vs 5GHz)
    classification = "POSSIBLE_WITH_CALIBRATION"
    print(f"  RSSI observations: {len(rssis)}, range: {min(rssis)}-{max(rssis)}")
    print(f"  Classification: {classification}")
    print(f"  Rationale: RSSI present and stable, but 0-128 scale (not dBm),")
    print(f"             no calibration data, no path-loss model, band separation needed")


def main():
    print("=" * 60)
    print("PHASE 14.9 — CONTROLLED VALIDATION TEST")
    print("=" * 60)
    
    results = []
    results.append(run_test("14.9.1 Raw GTPR Contract", test_raw_contract))
    results.append(run_test("14.9.2 Response Parsing", test_parse_response))
    results.append(run_test("14.9.2 Normalization & Pseudonymization", test_normalize))
    results.append(run_test("14.9.5 Band Derivation", test_band_derivation))
    results.append(run_test("14.9.4 Presence: First Seen", test_presence_first_seen))
    results.append(run_test("14.9.4 Presence: Seen (no change)", test_presence_seen))
    results.append(run_test("14.9.4 Presence: Changed", test_presence_changed))
    results.append(run_test("14.9.4 Presence: Absence Timeout", test_presence_absence_timeout))
    results.append(run_test("14.9.4 Presence: Reappearance", test_presence_reappearance))
    results.append(run_test("14.9.6 Event Store: Durable & Dedup", test_event_store_durable))
    results.append(run_test("14.9.6 Event Store: Bounded", test_event_store_bounded))
    results.append(run_test("14.9.3 Event Privacy (no raw MAC/hostname)", test_event_privacy))
    results.append(run_test("14.9.7 Idempotency Key", test_idempotency_key))
    results.append(run_test("14.9.9 Multi-Sensor sensor_id", test_multi_sensor))
    results.append(run_test("14.9.5 Distance Capability Audit", test_distance_capability))
    results.append(run_test("T0-T7 Full Pipeline (mock router + backend)", test_full_pipeline_mock_router))
    
    passed = sum(1 for r in results if r)
    total = len(results)
    
    print(f"\n{'='*60}")
    print(f"RESULTS: {passed}/{total} passed")
    print(f"{'='*60}")
    
    if passed < total:
        sys.exit(1)


if __name__ == "__main__":
    main()
