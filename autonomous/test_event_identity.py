#!/usr/bin/env python3
"""Unit tests for stable device identity in event_reporter.detect_events.

Verifies that a device keeps its fingerprint_id (huella) across band switches,
MAC rotation, disconnect and reconnect cycles — the core consistency property.
"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import event_reporter as er
from identity import AliasMap

SECRET = b"test-secret-key-16b"


def _cfg():
    return er.Config(
        url="", user="", password="", dialect="gdpr-json", sensor_id="s",
        secret=SECRET, smtp_host="", smtp_port=587, smtp_user="",
        smtp_password="", smtp_from="", smtp_to=[], smtp_tls="starttls",
        email_enabled=False, email_mode="individual", poll_interval=30,
        absence_threshold=2, cooldown=300, state_file=None, log_path="",
        retention=86400, alias_map_path=None,
    )


def _dev(mac, hostname, band, radio_mac, level=4, std="n"):
    raw = {
        "MACAddress": mac, "X_TP_HostName": hostname, "X_TP_RadioMac": radio_mac,
        "operatingStandard": std, "signalStrength": "100",
        "X_TP_SignalStrengthLevel": str(level), "active": "1",
    }
    return er._normalize_common(raw, SECRET)


class _NoLog:
    def emit(self, *a, **k):
        pass


class TestEventIdentityConsistency(unittest.TestCase):
    def test_multiband_one_device(self):
        """moto-g42 on 2.4 + 5 = one device entry, two band sub-ids."""
        d24 = _dev("aa:bb:cc:dd:ee:01", "moto-g42", "2.4GHz", "3c:6a:d2:5f:ab:c1", 4, "n")
        d5 = _dev("aa:bb:cc:dd:ee:02", "moto-g42", "5GHz", "3c:6a:d2:5f:ab:c3", 3, "ac")
        cfg = _cfg()
        ev, st = er.detect_events(cfg, {"devices": {}, "baseline": True, "last_poll": 0},
                                  [d24, d5], [], 1000, _NoLog(), AliasMap())
        self.assertEqual(len(ev), 0)  # baseline
        self.assertEqual(len(st["devices"]), 1)
        dev = next(iter(st["devices"].values()))
        self.assertEqual(set(dev["bands"].keys()), {"2.4GHz", "5GHz"})

    def test_band_drop_no_disconnect(self):
        """Dropping one band while still connected on another must NOT disconnect."""
        d24 = _dev("aa:bb:cc:dd:ee:01", "moto-g42", "2.4GHz", "3c:6a:d2:5f:ab:c1", 4, "n")
        d5 = _dev("aa:bb:cc:dd:ee:02", "moto-g42", "5GHz", "3c:6a:d2:5f:ab:c3", 3, "ac")
        cfg = _cfg()
        _, st = er.detect_events(cfg, {"devices": {}, "baseline": True, "last_poll": 0},
                                 [d24, d5], [], 1000, _NoLog(), AliasMap())
        st["baseline"] = False
        ev, st = er.detect_events(cfg, st, [d24], [], 1030, _NoLog(), AliasMap())
        types = [e["event_type"] for e in ev]
        self.assertNotIn("disconnected", types)
        self.assertNotIn("connected", types)

    def test_mac_rotation_no_reconnect(self):
        """Re-adding a band with a rotated MAC must NOT fire a connect event."""
        d24 = _dev("aa:bb:cc:dd:ee:01", "moto-g42", "2.4GHz", "3c:6a:d2:5f:ab:c1", 4, "n")
        d5 = _dev("aa:bb:cc:dd:ee:02", "moto-g42", "5GHz", "3c:6a:d2:5f:ab:c3", 3, "ac")
        cfg = _cfg()
        am = AliasMap()
        _, st = er.detect_events(cfg, {"devices": {}, "baseline": True, "last_poll": 0},
                                 [d24, d5], [], 1000, _NoLog(), am)
        st["baseline"] = False
        _, st = er.detect_events(cfg, st, [d24], [], 1030, _NoLog(), am)
        d5_new = _dev("02:11:22:33:44:55", "moto-g42", "5GHz", "3c:6a:d2:5f:ab:c3", 3, "ac")
        ev, st = er.detect_events(cfg, st, [d24, d5_new], [], 1060, _NoLog(), am)
        types = [e["event_type"] for e in ev]
        self.assertNotIn("connected", types)
        dev = next(iter(st["devices"].values()))
        self.assertGreaterEqual(len(dev["aliases"]), 2)

    def test_full_disconnect_then_reconnect_same_identity(self):
        """After full absence + reconnect with a new MAC, device_id stays the same."""
        d = _dev("aa:bb:cc:dd:ee:01", "moto-g42", "2.4GHz", "3c:6a:d2:5f:ab:c1", 4, "n")
        cfg = _cfg()
        am = AliasMap()
        _, st = er.detect_events(cfg, {"devices": {}, "baseline": True, "last_poll": 0},
                                 [d], [], 1000, _NoLog(), am)
        st["baseline"] = False
        # gone for absence_threshold polls
        _, st = er.detect_events(cfg, st, [], [], 1030, _NoLog(), am)
        ev, st = er.detect_events(cfg, st, [], [], 1060, _NoLog(), am)
        self.assertEqual([e["event_type"] for e in ev], ["disconnected"])
        # reconnect with a brand new randomized MAC, same hostname
        d_new = _dev("02:aa:bb:cc:dd:ee", "moto-g42", "2.4GHz", "3c:6a:d2:5f:ab:c1", 4, "n")
        ev, st = er.detect_events(cfg, st, [d_new], [], 1090, _NoLog(), am)
        self.assertEqual([e["event_type"] for e in ev], ["connected"])
        self.assertEqual(ev[0]["device_id"], d["fingerprint_id"])
        self.assertEqual(ev[0]["fingerprint_id"], d["fingerprint_id"])

    def test_event_payload_carries_alias_and_bands(self):
        d = _dev("aa:bb:cc:dd:ee:01", "moto-g42", "2.4GHz", "3c:6a:d2:5f:ab:c1", 4, "n")
        cfg = _cfg()
        am = AliasMap()
        _, st = er.detect_events(cfg, {"devices": {}, "baseline": True, "last_poll": 0},
                                 [d], [], 1000, _NoLog(), am)
        st["baseline"] = False
        _, st = er.detect_events(cfg, st, [], [], 1030, _NoLog(), am)
        ev, _ = er.detect_events(cfg, st, [], [], 1060, _NoLog(), am)
        e = ev[0]
        self.assertIn("mac_pseudonym", e)
        self.assertIn("aliases", e)
        self.assertIn("bands", e)
        self.assertIn("fingerprint_method", e)
        self.assertIn("fingerprint_confidence", e)


if __name__ == "__main__":
    unittest.main(verbosity=2)
