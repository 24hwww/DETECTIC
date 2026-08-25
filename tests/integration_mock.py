#!/usr/bin/env python3
"""Integration test: Detectic binary + mock_router.py end-to-end.

Exercises:
  mock_router
    -> GTPR login
    -> gl() x3
    -> NetworkMap
    -> Collector merge

Does not touch the real EX520V.
"""

import json
import os
import re
import signal
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
BIN = os.path.join(REPO, "target", "release", "detectic")
MOCK = os.path.join(REPO, "python", "mock_router.py")


def wait_for_port(host: str, port: int, timeout: float = 5.0):
    import socket
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with socket.create_connection((host, port), timeout=0.5):
                return
        except OSError:
            time.sleep(0.1)
    raise TimeoutError(f"port {port} did not become ready")


def main():
    if not os.path.exists(BIN):
        print(f"[test] building {BIN} ...")
        subprocess.run(["cargo", "build", "--release"], cwd=REPO, check=True)

    port = 18100
    mock = subprocess.Popen(
        [sys.executable, MOCK, "--port", str(port)],
        cwd=REPO,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )

    def cleanup():
        if mock.poll() is None:
            mock.terminate()
            try:
                mock.wait(timeout=3)
            except subprocess.TimeoutExpired:
                mock.kill()
                mock.wait()

    try:
        wait_for_port("127.0.0.1", port, timeout=8.0)
        env = os.environ.copy()
        env["DETECTIC_PASSWORD"] = "any"
        env["DETECTIC_SECRET"] = "dev-secret-change-me"

        result = subprocess.run(
            [BIN, "--url", f"http://127.0.0.1:{port}", "--user", "user", "map"],
            cwd=REPO,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            timeout=15,
        )

        output = result.stdout
        # The binary prints JSON to stdout (after debug lines on stderr).
        # Find the first '{' that starts the JSON object.
        start = output.find("{\n")
        if start == -1:
            print("--- STDOUT ---")
            print(output)
            print("--------------")
            raise AssertionError("no JSON object in output")

        payload = json.loads(output[start:])
        devices = payload["devices"]

        assert isinstance(devices, list), "devices is not a list"
        assert len(devices) == 3, f"expected 3 devices, got {len(devices)}: {devices}"

        by_source = {}
        for d in devices:
            by_source.setdefault(d.get("source"), []).append(d)

        assert "wifi" in by_source, f"no wifi device: {by_source}"
        assert len(by_source["wifi"]) == 2, f"expected 2 wifi, got {len(by_source.get('wifi', []))}"
        assert "host" in by_source, f"no ethernet host: {by_source}"

        for d in devices:
            assert d.get("mac"), f"missing mac: {d}"
            if d["source"] == "wifi":
                assert d.get("rssi") is not None, f"wifi device missing rssi: {d}"
                assert d.get("standard") is not None, f"wifi device missing standard: {d}"

        print("[test] integration passed")
        print(json.dumps(payload, indent=2))
        return 0
    finally:
        cleanup()
        # Print mock output on failure for diagnosis.
        if "payload" not in dir() or result.returncode != 0:
            leftover = mock.stdout.read() if mock.stdout else ""
            if leftover:
                print("--- MOCK OUTPUT ---")
                print(leftover)


if __name__ == "__main__":
    sys.exit(main())
