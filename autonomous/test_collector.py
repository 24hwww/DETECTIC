#!/usr/bin/env python3
"""Unit tests for the autonomous collector core logic (no router required).

Run:  python3 autonomous/test_collector.py
"""
import os
import sys
import tempfile
import time
import unittest
from unittest import mock

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import collector as c


class TestScheduling(unittest.TestCase):
    def test_align_slot_boundaries(self):
        self.assertEqual(c.align_slot(1_752_000_000), 1_752_000_000)
        self.assertEqual(c.align_slot(1_752_000_001), 1_752_000_000)
        self.assertEqual(c.align_slot(1_752_000_299), 1_752_000_000)
        self.assertEqual(c.align_slot(1_752_000_300), 1_752_000_300)
        self.assertEqual(c.align_slot(1_752_000_301), 1_752_000_300)

    def test_slot_is_5min_multiple(self):
        for ts in [1_750_000_000, 1_755_000_000, 1_753_000_123]:
            s = c.align_slot(ts)
            self.assertEqual(s % 300, 0)

    def test_capture_id_deterministic(self):
        a = c.capture_id_for("detectic-ex520-live", 1_752_000_000)
        b = c.capture_id_for("detectic-ex520-live", 1_752_000_000)
        cid = c.capture_id_for("detectic-ex520-live", 1_752_000_300)
        self.assertEqual(a, b)
        self.assertNotEqual(a, cid)
        self.assertEqual(len(a), 12)


class TestNormalize(unittest.TestCase):
    SECRET = b"0" * 32

    def test_pseudonymizes_no_raw_mac(self):
        raw = [{
            "MACAddress": "AA:BB:CC:11:22:33",
            "X_TP_RadioMac": "3c:6a:d2:5f:ab:c3",
            "X_TP_HostName": "phone",
            "X_TP_IPAddress": "192.168.0.5",
            "signalStrength": "100",
            "X_TP_SignalStrengthLevel": "4",
            "noise": "90",
            "operatingStandard": "ac",
            "lastDataDownlinkRate": "1234",
            "lastDataUplinkRate": "567",
            "active": "1",
        }]
        devs = c.normalize_devices(raw, self.SECRET)
        self.assertEqual(len(devs), 1)
        d = devs[0]
        self.assertNotIn("MACAddress", d)
        self.assertNotIn("aa:bb:cc", d["pseudonym"])
        self.assertEqual(d["band"], "5GHz")
        self.assertEqual(d["operating_standard"], "ac")
        self.assertEqual(d["tx_rate_kbps"], 1234)
        self.assertEqual(d["rx_rate_kbps"], 567)
        self.assertEqual(d["status"], "active")
        self.assertEqual(d["hostname"], "phone")

    def test_deterministic_pseudonym(self):
        raw = [{"MACAddress": "AA:BB:CC:11:22:33", "active": "1"}]
        d1 = c.normalize_devices(raw, self.SECRET)
        d2 = c.normalize_devices(raw, self.SECRET)
        self.assertEqual(d1[0]["pseudonym"], d2[0]["pseudonym"])

    def test_payload_hash_deterministic(self):
        devs = [{"pseudonym": "abc", "band": "2.4GHz"}]
        self.assertEqual(c.payload_hash(devs), c.payload_hash(devs))


class TestStore(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.db = os.path.join(self.tmp, "state.db")
        self.store = c.Store(self.db)

    def tearDown(self):
        self.store.close()

    def test_dedup_capture(self):
        cid = c.capture_id_for("s", 1_752_000_000)
        self.store.insert_capture(cid, "r1", "s", 1_752_000_000, 100, c.CAPTURED,
                                  None, None, 0, 0, None)
        self.store.insert_capture(cid, "r2", "s", 1_752_000_000, 100, c.CAPTURED,
                                  None, None, 0, 0, None)
        rows = self.store.conn.execute("SELECT COUNT(*) FROM captures").fetchone()[0]
        self.assertEqual(rows, 1)

    def test_state_machine_transition(self):
        cid = c.capture_id_for("s", 1_752_000_000)
        self.store.insert_capture(cid, "r1", "s", 1_752_000_000, 100, c.CAPTURED,
                                  7, 35, 2, 2, "hash")
        self.store.update_capture(cid, status=c.PERSISTED)
        self.assertEqual(self.store.get_capture(cid)["status"], c.PERSISTED)
        self.store.insert_delivery(f"dlv-{cid}", cid, "rep-1", 1, c.DELIVERED, None)
        self.store.update_capture(cid, status=c.DELIVERED)
        cap = self.store.get_capture(cid)
        self.assertEqual(cap["status"], c.DELIVERED)
        dlv = self.store.delivery_for(cid)
        self.assertEqual(dlv["final_status"], c.DELIVERED)
        self.assertEqual(dlv["attempt_number"], 1)

    def test_pending_deliveries_excludes_delivered(self):
        now = int(time.time())
        cid = c.capture_id_for("s", now - 300)
        self.store.insert_capture(cid, "r1", "s", now - 300, 100, c.DELIVERED,
                                  None, None, 1, 1, None)
        cid2 = c.capture_id_for("s", now - 60)
        self.store.insert_capture(cid2, "r2", "s", now - 60, 100, c.DELIVERY_FAILED,
                                  None, None, 1, 1, None)
        pending = self.store.pending_deliveries()
        ids = [p["capture_id"] for p in pending]
        self.assertNotIn(cid, ids)
        self.assertIn(cid2, ids)


class TestDeliveryRetry(unittest.TestCase):
    def test_retries_then_succeeds(self):
        cfg = c.Config(
            db_path=":memory:", sensor_id="s", url="http://x", user="u",
            password="p", secret=b"k" * 32, dialect="json",
            smtp_host="smtp.example", smtp_port=587, smtp_user="u",
            smtp_password="p", smtp_from="a@b.c", smtp_to=["d@e.f"],
            smtp_tls="starttls", email_enabled=True, log_path="",
        )
        store = c.Store(tempfile.mktemp(suffix=".db"))
        cid = c.capture_id_for("s", 1_752_000_000)
        store.insert_capture(cid, "r1", "s", 1_752_000_000, 100, c.REPORT_GENERATED,
                             7, 35, 1, 1, "h")
        calls = {"n": 0}

        def flaky(*a, **kw):
            calls["n"] += 1
            if calls["n"] < 3:
                raise ConnectionError("smtp refused")
            return None

        with mock.patch.object(c, "send_email", side_effect=flaky):
            with mock.patch.object(c.time, "sleep"):  # no real backoff wait
                final, _ = c.deliver_report(cfg, store,
                                            store.get_capture(cid), "t", "<h>", "rep")
        self.assertEqual(final, c.DELIVERED)
        self.assertEqual(store.get_capture(cid)["status"], c.DELIVERED)
        dlv = store.delivery_for(cid)
        self.assertEqual(dlv["attempt_number"], 3)
        store.close()

    def test_all_attempts_fail_marks_failed(self):
        cfg = c.Config(
            db_path=":memory:", sensor_id="s", url="http://x", user="u",
            password="p", secret=b"k" * 32, dialect="json",
            smtp_host="smtp.example", smtp_port=587, smtp_user="u",
            smtp_password="p", smtp_from="a@b.c", smtp_to=["d@e.f"],
            smtp_tls="starttls", email_enabled=True, log_path="",
        )
        store = c.Store(tempfile.mktemp(suffix=".db"))
        cid = c.capture_id_for("s", 1_752_000_000)
        store.insert_capture(cid, "r1", "s", 1_752_000_000, 100, c.REPORT_GENERATED,
                             7, 35, 1, 1, "h")

        def always_fail(*a, **kw):
            raise smtplib.SMTPConnectError(1, "refused")

        with mock.patch.object(c, "send_email", side_effect=always_fail):
            with mock.patch.object(c.time, "sleep"):
                final, attempts = c.deliver_report(cfg, store,
                                                   store.get_capture(cid), "t", "<h>", "rep")
        self.assertEqual(final, c.DELIVERY_FAILED)
        self.assertEqual(attempts, 3)
        self.assertEqual(store.get_capture(cid)["status"], c.DELIVERY_FAILED)
        store.close()


class TestReport(unittest.TestCase):
    def test_report_sanitized(self):
        cfg = c.Config(
            db_path="x", sensor_id="detectic-ex520-live", url="http://x", user="u",
            password="p", secret=b"k" * 32, dialect="json",
            smtp_host="h", smtp_port=587, smtp_user="", smtp_password="",
            smtp_from="a@b.c", smtp_to=["d@e.f"], smtp_tls="starttls",
            email_enabled=True, log_path="",
        )
        cap = {
            "capture_id": "abc123", "scheduled_at": 1_752_000_000,
            "started_at": 1_752_000_010, "completed_at": 1_752_000_020,
            "api_latency_ms": 7, "auth_latency_ms": 35, "status": c.DELIVERED,
            "device_count": 1, "active_device_count": 1,
        }
        devs = [{"pseudonym": "deadbeefcafe1234", "hostname": "phone",
                 "band": "2.4GHz", "signal_strength": 100, "signal_level": 4,
                 "noise": 90, "operating_standard": "n", "tx_rate_kbps": 1000,
                 "rx_rate_kbps": 500, "status": "active"}]
        text, html, report_id = c.build_report(cfg, cap, devs)
        self.assertIn(report_id, text)
        self.assertIn("DETECTIC EX520 AUTONOMOUS OBSERVATION", text)
        self.assertNotIn("AA:BB:CC", text)
        self.assertNotIn("password", text.lower())
        self.assertIn("Report ID", text)
        self.assertIn("Capture ID", text)
        self.assertIn("Scheduled time", text)
        self.assertIn("Capture started", text)
        self.assertIn("Capture finished", text)
        self.assertIn("API latency", text)
        self.assertIn("Auth latency", text)


if __name__ == "__main__":
    unittest.main(verbosity=2)
