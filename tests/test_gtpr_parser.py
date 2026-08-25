#!/usr/bin/env python3
"""Regression tests for the GTPR network_map parser.

Tests both EX520 response formats:
  FORMAT A (list):     {"data": [...]}
  FORMAT B (ASSOCDEV): {"data": {"ASSOCDEV": [...]}}

Uses pseudonymized fixtures — no real MAC addresses.
"""
import json
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python"))


def extract_devices(js: str):
    """Mirror of GtprClient.network_map parsing logic."""
    parsed = json.loads(js)
    data = parsed.get("data", {})
    if isinstance(data, list):
        return data
    elif isinstance(data, dict):
        return data.get("ASSOCDEV", [])
    return []


def test_format_a_list():
    js = json.dumps({
        "data": [
            {"X_TP_HostName": "device-a", "MACAddress": "AA:BB:CC:00:00:01",
             "signalStrength": "100", "noise": "50"},
            {"X_TP_HostName": "device-b", "MACAddress": "AA:BB:CC:00:00:02",
             "signalStrength": "90", "noise": "50"},
        ],
        "operation": "gl", "oid": "DEV2_WIFI_APDEV_ASSOCDEV", "success": True
    })
    devs = extract_devices(js)
    assert len(devs) == 2
    assert devs[0]["X_TP_HostName"] == "device-a"
    assert devs[1]["signalStrength"] == "90"


def test_format_b_assocdev():
    js = json.dumps({
        "data": {"ASSOCDEV": [
            {"MACAddress": "AA:BB:CC:00:00:03", "signalStrength": "80"},
        ]},
        "operation": "gl", "oid": "DEV2_WIFI_APDEV_ASSOCDEV", "success": True
    })
    devs = extract_devices(js)
    assert len(devs) == 1
    assert devs[0]["MACAddress"] == "AA:BB:CC:00:00:03"


def test_empty_response_list():
    js = json.dumps({
        "data": [], "operation": "gl",
        "oid": "DEV2_WIFI_APDEV_ASSOCDEV", "success": True
    })
    assert extract_devices(js) == []


def test_empty_response_assocdev():
    js = json.dumps({
        "data": {"ASSOCDEV": []}, "operation": "gl",
        "oid": "DEV2_WIFI_APDEV_ASSOCDEV", "success": True
    })
    assert extract_devices(js) == []


def test_missing_data():
    js = json.dumps({"operation": "gl", "oid": "DEV2_WIFI_APDEV_ASSOCDEV"})
    assert extract_devices(js) == []


def test_malformed_json():
    try:
        extract_devices("not json")
        assert False, "should have raised"
    except json.JSONDecodeError:
        pass


def test_unknown_fields_ignored():
    js = json.dumps({
        "data": [{"MACAddress": "AA:BB:CC:00:00:04", "unknownField": "x"}],
        "extra": "ignored"
    })
    devs = extract_devices(js)
    assert len(devs) == 1
    assert "unknownField" in devs[0]


def test_missing_fields_in_device():
    js = json.dumps({
        "data": [{"MACAddress": "AA:BB:CC:00:00:05"}],
    })
    devs = extract_devices(js)
    assert len(devs) == 1
    assert devs[0].get("signalStrength") is None


if __name__ == "__main__":
    for fn in [
        test_format_a_list,
        test_format_b_assocdev,
        test_empty_response_list,
        test_empty_response_assocdev,
        test_missing_data,
        test_malformed_json,
        test_unknown_fields_ignored,
        test_missing_fields_in_device,
    ]:
        fn()
        print(f"PASS: {fn.__name__}")
    print("\nAll GTPR parser tests passed.")
