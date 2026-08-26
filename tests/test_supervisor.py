#!/usr/bin/env python3
"""Unit tests for the EX520 Edge Supervisor (deploy/ex520_package/watchdog.py).

These tests do not require a live EX520.  They exercise the state machine,
checksum verification, trigger idempotency, exponential backoff, secret
redaction, and health detection by injecting stub functions.
"""
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import MagicMock

# The supervisor expects DETECTIC_PASSWORD in the environment at import time.
os.environ.setdefault("DETECTIC_PASSWORD", "test-secret")

# Add the package directory to the path so we can import watchdog.
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "deploy", "ex520_package"))

import watchdog  # type: ignore


class FakeClock:
    def __init__(self, start: float = 0.0):
        self.t = start

    def __call__(self) -> float:
        return self.t

    def sleep(self, n: float) -> None:
        self.t += n


class EdgeSupervisorTests(unittest.TestCase):
    def _make(self, router=False, gtpr=False, trigger=False, tcp=False, config=None):
        cfg = config or watchdog.SupervisorConfig(
            poll_interval=1,
            down_threshold=5,
            phoenix_grace=2,
            health_timeout=10,
        )
        return watchdog.EdgeSupervisor(
            is_router_reachable=lambda: router,
            is_gtpr_ready=lambda: gtpr,
            do_trigger=lambda: trigger,
            tcp_probe_fn=lambda h, p, t: tcp,
            config=cfg,
            logger=lambda msg: None,
        )

    def test_state_unknown_to_router_down(self):
        sv = self._make(router=False)
        sv.tick()
        self.assertEqual(sv.state.state, watchdog.State.ROUTER_DOWN)

    def test_cold_boot_triggers_after_threshold(self):
        clock = FakeClock()
        sv = self._make(config=watchdog.SupervisorConfig(
            poll_interval=1,
            down_threshold=5,
            phoenix_grace=0,
            health_timeout=10,
        ))
        # Both reachability probes are False until the clock passes 10.
        sv._router = lambda: clock.t >= 10
        sv._gtpr = lambda: clock.t >= 10

        triggered = []
        sv._trigger_func = lambda: triggered.append(True) or True
        original_time = watchdog.time.time
        original_sleep = watchdog.time.sleep
        watchdog.time.time = clock
        watchdog.time.sleep = clock.sleep
        try:
            sv.tick()  # t=0, router down
            clock.sleep(1)
            sv.tick()  # t=1
            clock.sleep(6)
            sv.tick()  # t=7, still down, armed
            self.assertEqual(sv.state.state, watchdog.State.ROUTER_DOWN)
            self.assertTrue(sv.state.armed)
            # Now router comes back up
            clock.sleep(4)
            sv.tick()
            self.assertEqual(sv.state.state, watchdog.State.SENSOR_STARTING)
            self.assertEqual(len(triggered), 1)
        finally:
            watchdog.time.time = original_time
            watchdog.time.sleep = original_sleep

    def test_no_trigger_without_sustained_down(self):
        sv = self._make(router=True)
        sv._gtpr = lambda: True
        sv._trigger_func = MagicMock(return_value=True)
        sv.tick()
        sv._router = lambda: False
        sv.tick()
        sv._router = lambda: True
        sv.tick()
        sv._trigger_func.assert_not_called()

    def test_trigger_idempotency(self):
        clock = FakeClock()
        config = watchdog.SupervisorConfig(
            poll_interval=1,
            down_threshold=1,
            phoenix_grace=0,
            health_timeout=10,
            min_boot_interval=60,
        )
        sv = self._make(config=config)
        sv._router = lambda: True
        sv._gtpr = lambda: True
        sv._trigger_func = MagicMock(return_value=True)
        sv.state.armed = True
        sv.state.last_trigger = None

        original_time = watchdog.time.time
        original_sleep = watchdog.time.sleep
        watchdog.time.time = clock
        watchdog.time.sleep = clock.sleep
        try:
            sv.tick()  # trigger once
            sv._trigger_func.assert_called_once()
            sv._trigger_func.reset_mock()

            # Another tick before min_boot_interval should not re-trigger.
            clock.sleep(5)
            sv.tick()
            sv._trigger_func.assert_not_called()
        finally:
            watchdog.time.time = original_time
            watchdog.time.sleep = original_sleep

    def test_exponential_backoff(self):
        sv = self._make(router=True, gtpr=True, trigger=False)
        sv._trigger_func = lambda: False
        original_time = watchdog.time.time
        original_sleep = watchdog.time.sleep
        watchdog.time.time = lambda: 100.0
        watchdog.time.sleep = lambda n: None
        try:
            sv.state.last_trigger = 0.0
            sv._recover()
            self.assertEqual(sv.state.backoff_level, 1)
            sv._recover()
            self.assertEqual(sv.state.backoff_level, 2)
            sv._recover()
            self.assertEqual(sv.state.backoff_level, 3)
            sv._recover()
            self.assertEqual(sv.state.backoff_level, 4)
            self.assertTrue(sv.state.degraded)
        finally:
            watchdog.time.time = original_time
            watchdog.time.sleep = original_sleep

    def test_secret_redaction(self):
        msg = "user=admin password=supersecret token=abc123"
        out = watchdog.redact(msg)
        self.assertNotIn("supersecret", out)
        self.assertNotIn("abc123", out)
        self.assertIn("password=***", out)
        self.assertIn("token=***", out)

    def test_sensor_health_detected_via_log(self):
        with tempfile.TemporaryDirectory() as tmp:
            pkg = Path(tmp)
            sensor_log = pkg / "sensor_log.txt"
            sensor_log.write_text("ok")
            old_pkg = watchdog.PACKAGE_ROOT
            try:
                watchdog.PACKAGE_ROOT = str(pkg)
                sv = self._make(router=True, gtpr=True, trigger=True)
                sv.state.last_trigger = 0.0
                sv._check_sensor_health()
                self.assertAlmostEqual(sv.state.last_health, watchdog.time.time(), places=2)
            finally:
                watchdog.PACKAGE_ROOT = old_pkg

    def test_package_integrity_valid(self):
        with tempfile.TemporaryDirectory() as tmp:
            pkg = Path(tmp)
            data = b"A" * 1024
            (pkg / "detectic.aa").write_bytes(data)
            (pkg / "detectic.ab").write_bytes(b"B" * 512)
            import hashlib

            h = hashlib.sha256(data).hexdigest()
            (pkg / "detectic.aa.sha256").write_text(h)
            h2 = hashlib.sha256(b"B" * 512).hexdigest()
            (pkg / "detectic.ab.sha256").write_text(h2)

            old = watchdog.PACKAGE_ROOT
            try:
                watchdog.PACKAGE_ROOT = str(pkg)
                self.assertTrue(watchdog.check_package_integrity())
            finally:
                watchdog.PACKAGE_ROOT = old

    def test_package_integrity_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            pkg = Path(tmp)
            (pkg / "detectic.aa").write_bytes(b"A" * 1024)
            (pkg / "detectic.aa.sha256").write_text("0" * 64)
            old = watchdog.PACKAGE_ROOT
            try:
                watchdog.PACKAGE_ROOT = str(pkg)
                self.assertFalse(watchdog.check_package_integrity())
            finally:
                watchdog.PACKAGE_ROOT = old


if __name__ == "__main__":
    unittest.main()
