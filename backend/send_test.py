#!/usr/bin/env python3
"""Test client: sends a simulated sensor snapshot to the Detectic backend.

Usage:
    python3 send_test.py [--url http://localhost:8080/api/v1/events]
                         [--sensor ex520-001]
                         [--secret dev-secret-change-me]
"""

import argparse
import hashlib
import hmac
import json
import random
import time
import urllib.request


def make_snapshot(sensor_id, secret, device_count=5):
    now = int(time.time())
    devices = []
    for i in range(device_count):
        pseudo = hashlib.sha256(f"device-{i}".encode()).hexdigest()[:16]
        devices.append({
            "pseudonym": pseudo,
            "rssi": random.randint(-80, -30),
            "source": "wifi",
            "standard": random.choice(["ax", "ac", "n"]),
            "radio_mac": hashlib.sha256(f"radio-{i}".encode()).hexdigest()[:16],
        })

    payload = {
        "sensor_id": sensor_id,
        "id": hashlib.sha256(f"{sensor_id}|{now}".encode()).hexdigest()[:16],
        "captured_at": now,
        "devices": devices,
    }
    return payload


def send(url, sensor_id, secret, payload):
    body = json.dumps(payload, separators=(",", ":")).encode()
    sig = hmac.new(secret.encode(), body, hashlib.sha256).hexdigest()

    req = urllib.request.Request(
        url,
        data=body,
        headers={
            "Content-Type": "application/json",
            "X-Detectic-Sensor": sensor_id,
            "X-Detectic-Signature": sig,
        },
        method="POST",
    )

    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read().decode())
            print(f"[OK] {resp.status} — {json.dumps(data)}")
            return True
    except urllib.error.HTTPError as e:
        body = e.read().decode() if e.fp else ""
        print(f"[FAIL] HTTP {e.code} — {body}")
        return False
    except Exception as e:
        print(f"[FAIL] {e}")
        return False


def main():
    ap = argparse.ArgumentParser(description="Test Detectic backend")
    ap.add_argument("--url", default="http://localhost:8080/api/v1/events")
    ap.add_argument("--sensor", default="ex520-001")
    ap.add_argument("--secret", default="dev-secret-change-me")
    ap.add_argument("--count", type=int, default=5, help="Number of test snapshots")
    ap.add_argument("--devices", type=int, default=5, help="Devices per snapshot")
    args = ap.parse_args()

    print(f"Sending {args.count} snapshots to {args.url}")
    print(f"Sensor: {args.sensor}")
    print()

    ok = 0
    fail = 0
    for i in range(args.count):
        payload = make_snapshot(args.sensor, args.secret, args.devices)
        if send(args.url, args.sensor, args.secret, payload):
            ok += 1
        else:
            fail += 1
        time.sleep(0.2)

    print()
    print(f"Results: {ok} sent, {fail} failed")

    # Query back
    base = args.url.rsplit("/api/v1/events", 1)[0]
    for endpoint in ["/api/v1/stats", "/api/v1/devices", "/api/v1/healthz"]:
        try:
            req = urllib.request.Request(base + endpoint)
            with urllib.request.urlopen(req, timeout=5) as resp:
                data = json.loads(resp.read().decode())
                print(f"\n{endpoint}:")
                print(json.dumps(data, indent=2)[:500])
        except Exception as e:
            print(f"\n{endpoint}: {e}")


if __name__ == "__main__":
    main()
