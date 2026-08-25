#!/usr/bin/env python3
"""Detectic — Canonical HMAC Contract V1 test vectors.

Defines the canonical authentication protocol used by:
  - Rust sensor (src/publisher.rs)
  - Python collector (autonomous/collector.py)
  - Cloudflare Worker (backend/cf-worker/src/index.ts)

Contract:
  algorithm:   HMAC-SHA256
  key:         UTF-8 bytes of the canonical secret string
  signed:      "<timestamp>\n<body>"
  signature:   lowercase hexadecimal
  headers:     X-Detectic-Sensor, X-Detectic-Signature, X-Detectic-Timestamp
  replay:      ±300s window

These test vectors use NON-PRODUCTION secrets only.
"""
import hashlib
import hmac
import json
import time

# --- Non-production test vectors ---
TEST_SECRET = "detectic-test-secret-v1-not-production"
TEST_SENSOR = "test-sensor-001"
TEST_BODY = json.dumps(
    {"sensor_id": "test-sensor-001", "devices": [{"pseudonym": "abc"}]},
    separators=(",", ":"),
)
TEST_TIMESTAMP = 1700000000

EXPECTED_SIG = hmac.new(
    TEST_SECRET.encode("utf-8"),
    f"{TEST_TIMESTAMP}\n{TEST_BODY}".encode("utf-8"),
    hashlib.sha256,
).hexdigest()


def sign(secret: str, timestamp: int, body: str) -> str:
    """Produce a canonical HMAC-SHA256 signature."""
    signed = f"{timestamp}\n{body}".encode("utf-8")
    return hmac.new(secret.encode("utf-8"), signed, hashlib.sha256).hexdigest()


def verify(secret: str, sensor_id: str, signature: str, timestamp: int,
           body: str, now: int = None, window: int = 300) -> bool:
    """Verify a canonical HMAC signature with replay protection."""
    if not secret or not signature:
        return False
    now = now if now is not None else int(time.time())
    if abs(now - timestamp) > window:
        return False
    expected = sign(secret, timestamp, body)
    return hmac.compare_digest(expected, signature)


def test_valid_signature():
    sig = sign(TEST_SECRET, TEST_TIMESTAMP, TEST_BODY)
    assert sig == EXPECTED_SIG
    assert verify(TEST_SECRET, TEST_SENSOR, sig, TEST_TIMESTAMP, TEST_BODY,
                  now=TEST_TIMESTAMP)


def test_invalid_signature():
    bad = "0" * 64
    assert not verify(TEST_SECRET, TEST_SENSOR, bad, TEST_TIMESTAMP, TEST_BODY,
                      now=TEST_TIMESTAMP)


def test_altered_body():
    sig = sign(TEST_SECRET, TEST_TIMESTAMP, TEST_BODY)
    altered = TEST_BODY.replace("abc", "xyz")
    assert not verify(TEST_SECRET, TEST_SENSOR, sig, TEST_TIMESTAMP, altered,
                      now=TEST_TIMESTAMP)


def test_altered_timestamp():
    sig = sign(TEST_SECRET, TEST_TIMESTAMP, TEST_BODY)
    assert not verify(TEST_SECRET, TEST_SENSOR, sig, TEST_TIMESTAMP + 1,
                      TEST_BODY, now=TEST_TIMESTAMP)


def test_wrong_sensor_id():
    """Wrong sensor_id → wrong secret lookup → reject."""
    sig = sign(TEST_SECRET, TEST_TIMESTAMP, TEST_BODY)
    # Simulate lookup with a different secret for a different sensor
    assert not verify("other-secret", "other-sensor", sig, TEST_TIMESTAMP,
                      TEST_BODY, now=TEST_TIMESTAMP)


def test_replayed_request():
    """Request outside ±300s window → reject."""
    sig = sign(TEST_SECRET, TEST_TIMESTAMP, TEST_BODY)
    # 600s outside window
    assert not verify(TEST_SECRET, TEST_SENSOR, sig, TEST_TIMESTAMP,
                      TEST_BODY, now=TEST_TIMESTAMP + 600)


def test_expired_request():
    """Request exactly at boundary (301s) → reject."""
    sig = sign(TEST_SECRET, TEST_TIMESTAMP, TEST_BODY)
    assert not verify(TEST_SECRET, TEST_SENSOR, sig, TEST_TIMESTAMP,
                      TEST_BODY, now=TEST_TIMESTAMP + 301)
    # But 300s is OK
    assert verify(TEST_SECRET, TEST_SENSOR, sig, TEST_TIMESTAMP, TEST_BODY,
                  now=TEST_TIMESTAMP + 300)


if __name__ == "__main__":
    for fn in [
        test_valid_signature,
        test_invalid_signature,
        test_altered_body,
        test_altered_timestamp,
        test_wrong_sensor_id,
        test_replayed_request,
        test_expired_request,
    ]:
        fn()
        print(f"PASS: {fn.__name__}")
    print(f"\nEXPECTED_SIG = {EXPECTED_SIG}")
    print("All HMAC contract tests passed.")
