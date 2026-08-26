#!/usr/bin/env python3
"""Unit tests for Wi-Fi vs host observation normalization and classification.

These tests do not need a live router.
"""
import hashlib
import hmac as hmac_lib
import importlib.util
import os
import sys
import types

# Stub the detectic_client module before executing the live observation script.
sys.modules["detectic_client"] = types.SimpleNamespace(GtprClient=object)

lo_path = os.path.join(os.path.dirname(__file__), "live_observation_smtp.py")
spec = importlib.util.spec_from_file_location("live_observation_smtp", lo_path)
lo = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lo)


def _pseudonym(mac: str) -> str:
    return hmac_lib.new(
        b"detectic-live-test-secret", mac.encode(), hashlib.sha256
    ).hexdigest()[:16]


def test_normalize_mac():
    assert lo.normalize_mac("3C:6A:D2:5F:AB:C1") == "3c:6a:d2:5f:ab:c1"
    assert lo.normalize_mac("3c-6a-d2-5f-ab-c1") == "3c:6a:d2:5f:ab:c1"
    assert lo.normalize_mac("3c6ad25fabc1") == "3c:6a:d2:5f:ab:c1"
    assert lo.normalize_mac("3C6AD25FABC1") == "3c:6a:d2:5f:ab:c1"
    assert lo.normalize_mac("bad") == ""
    assert lo.normalize_mac("") == ""


def test_mask_mac_is_six_octets():
    masked = lo.mask_mac("3c:6a:d2:5f:ab:c1")
    assert len(masked.split(":")) == 6, f"masked output has {len(masked.split(':'))} octets: {masked}"
    assert masked == "3c:6a:d2:**:ab:c1"


def test_build_device_summary_wifi_and_host_deduplication():
    results = {
        "DEV2_WIFI_APDEV_ASSOCDEV": [
            {
                "MACAddress": "3C:6A:D2:5F:AB:C1",
                "signalStrength": "110",
                "X_TP_HostName": "Phone",
                "X_TP_IPAddress": "192.168.0.10",
                "operatingStandard": "ac",
                "active": "1",
                "associationTime": "123",
                "X_TP_RadioMac": "00:00:00:00:00:00",
            }
        ],
        "DEV2_WIFI_DE_STA": [],
        "DEV2_HOST_ENTRY": [
            {
                "physAddress": "3c:6a:d2:5f:ab:c1",
                "hostName": "Phone",
                "IPAddress": "192.168.0.10",
                "interfaceType": "Wi-Fi",
            },
            {
                "physAddress": "aa:bb:cc:dd:ee:ff",
                "hostName": "Desktop",
                "IPAddress": "192.168.0.20",
                "interfaceType": "Ethernet",
            },
        ],
    }
    devices, summary = lo.build_device_summary(results)

    # Phone appears only once, from the Wi-Fi association.
    phone = [d for d in devices if d["pseudonym"] == _pseudonym("3c:6a:d2:5f:ab:c1")]
    assert len(phone) == 1
    assert phone[0]["source"] == "wifi"
    assert phone[0]["band"] == "5GHz"

    # Desktop appears as a wired host-only entry.
    desktop = [d for d in devices if d["pseudonym"] == _pseudonym("aa:bb:cc:dd:ee:ff")]
    assert len(desktop) == 1
    assert desktop[0]["source"] == "host"
    assert desktop[0]["interface_type"] == "Ethernet"
    assert desktop[0]["band"] == "Ethernet"
    assert desktop[0]["proximity_label"] == "Cabo"

    assert summary["total"] == 2
    assert summary["wifi"] == 1
    assert summary["wired"] == 1


def test_build_device_summary_host_wireless_not_counted_as_wired():
    results = {
        "DEV2_WIFI_APDEV_ASSOCDEV": [],
        "DEV2_WIFI_DE_STA": [],
        "DEV2_HOST_ENTRY": [
            {
                "physAddress": "aa:bb:cc:dd:ee:ff",
                "hostName": "SleepyPhone",
                "IPAddress": "192.168.0.21",
                "interfaceType": "802.11",
            },
        ],
    }
    devices, summary = lo.build_device_summary(results)

    assert len(devices) == 1
    assert devices[0]["source"] == "host"
    assert devices[0]["interface_type"] == "802.11"
    assert devices[0]["band"] == "802.11"
    assert devices[0]["proximity_label"] == "Incerto"
    assert summary["wifi"] == 0
    assert summary["wired"] == 0
    assert summary["total"] == 1


if __name__ == "__main__":
    tests = (
        test_normalize_mac,
        test_mask_mac_is_six_octets,
        test_build_device_summary_wifi_and_host_deduplication,
        test_build_device_summary_host_wireless_not_counted_as_wired,
    )
    for fn in tests:
        try:
            fn()
            print(f"PASS: {fn.__name__}")
        except AssertionError as e:
            print(f"FAIL: {fn.__name__}: {e}")
            raise
    print("All tests passed.")
