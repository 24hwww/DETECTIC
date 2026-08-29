#!/usr/bin/env python3
"""Secret-gating tests for the autonomous collector / event_reporter.

Verifies that a missing sensor secret fails closed (no silent fallback to the
well-known development secret) unless AUTONOMOUS_ALLOW_DEV_SECRET=1 is set.
Run: python3 -m unittest autonomous.tests.test_secret_gating
"""
import os
import sys
import unittest
from pathlib import Path

AUTONOMOUS = str(Path(__file__).resolve().parents[1])
sys.path.insert(0, AUTONOMOUS)
import collector  # noqa: E402
import event_reporter  # noqa: E402


def _noop_dotenv(path):
    return None


class SecretGatingTest(unittest.TestCase):
    def setUp(self):
        self._saved_env = dict(os.environ)
        self._saved_c_load = collector.load_dotenv
        self._saved_e_load = event_reporter.load_dotenv
        collector.load_dotenv = _noop_dotenv
        event_reporter.load_dotenv = _noop_dotenv
        for k in ("AUTONOMOUS_SECRET", "DETECTIC_SECRET", "AUTONOMOUS_ALLOW_DEV_SECRET"):
            os.environ.pop(k, None)

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._saved_env)
        collector.load_dotenv = self._saved_c_load
        event_reporter.load_dotenv = self._saved_e_load

    def test_collector_fails_closed_without_secret(self):
        with self.assertRaises(ValueError):
            collector.load_config()

    def test_event_reporter_fails_closed_without_secret(self):
        with self.assertRaises(ValueError):
            event_reporter.load_config()

    def test_collector_dev_opt_in_returns_dev_secret(self):
        os.environ["AUTONOMOUS_ALLOW_DEV_SECRET"] = "1"
        cfg = collector.load_config()
        self.assertEqual(cfg.secret, b"detectic-autonomous-dev-secret")

    def test_event_reporter_dev_opt_in_returns_dev_secret(self):
        os.environ["AUTONOMOUS_ALLOW_DEV_SECRET"] = "1"
        cfg = event_reporter.load_config()
        self.assertEqual(cfg.secret, b"detectic-autonomous-dev-secret")

    def test_event_reporter_rejects_malformed_hex(self):
        os.environ["AUTONOMOUS_SECRET"] = "nothex!!"
        with self.assertRaises(ValueError):
            event_reporter.load_config()


if __name__ == "__main__":
    unittest.main()
