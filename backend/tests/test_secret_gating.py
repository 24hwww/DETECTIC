#!/usr/bin/env python3
"""Secret-gating tests for backend/server.py.

Verifies production fails closed rather than silently using the well-known
development credentials, and that the development fallback requires an explicit
opt-in. Run: python3 -m unittest backend.tests.test_secret_gating
"""
import os
import sys
import tempfile
import unittest
from pathlib import Path

BACKEND = str(Path(__file__).resolve().parents[1])
sys.path.insert(0, BACKEND)
import server  # noqa: E402


class LoadSensorsTest(unittest.TestCase):
    def setUp(self):
        self._saved_env = dict(os.environ)
        self._saved_file = server.SENSORS_FILE
        fd, self._tmp = tempfile.mkstemp(suffix="sensors.json")
        os.close(fd)
        os.unlink(self._tmp)  # ensure it does NOT exist initially
        server.SENSORS_FILE = self._tmp
        # Ensure no external configuration leaks in.
        os.environ.pop("DETECTIC_SENSORS", None)
        os.environ.pop("DETECTIC_ALLOW_DEV_FALLBACK", None)

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._saved_env)
        server.SENSORS_FILE = self._saved_file
        if os.path.exists(self._tmp):
            os.unlink(self._tmp)

    def test_uses_env_sensors(self):
        os.environ["DETECTIC_SENSORS"] = '{"ex520-001":"prod-secret"}'
        self.assertEqual(server.load_sensors(), {"ex520-001": "prod-secret"})

    def test_uses_sensors_file(self):
        with open(self._tmp, "w") as f:
            f.write('{"ex520-001":"file-secret"}')
        self.assertEqual(server.load_sensors(), {"ex520-001": "file-secret"})

    def test_fails_closed_without_configuration(self):
        with self.assertRaises(RuntimeError):
            server.load_sensors()

    def test_dev_fallback_requires_opt_in(self):
        os.environ["DETECTIC_ALLOW_DEV_FALLBACK"] = "1"
        got = server.load_sensors()
        self.assertEqual(got, server.DEV_SENSORS)
        # The fallback must write the dev file only after explicit opt-in.
        self.assertTrue(os.path.exists(self._tmp))


if __name__ == "__main__":
    unittest.main()
