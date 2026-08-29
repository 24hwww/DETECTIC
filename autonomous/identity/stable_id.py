"""Stable device fingerprint (huella) — identity that survives reconnects.

The MAC-based pseudonym (`engine._pseudonym`) is NOT stable across
connect/disconnect cycles for two reasons:

  1. MAC randomization (iOS 14+ / Android 10+): a device that rotates its
     privacy MAC on reconnect gets a brand-new pseudonym, so the same physical
     device appears as "new" every time.
  2. Multi-band clients: many dual-band stations use a different MAC per band,
     so one device on 2.4GHz + 5GHz becomes two pseudonyms.

This module derives a **stable fingerprint_id** from attributes that persist
across reconnects and band switches — primarily the OS-reported hostname,
corroborated by manufacturer (OUI) and device class. The MAC pseudonym is kept
as an **alias** of the fingerprint_id, not as the primary identity.

Identity here is *device* identity, never *person* identity (see AGENTS.md §3).
"""

from __future__ import annotations

import hashlib
import hmac
import re
from dataclasses import dataclass
from typing import Optional

from .mac import MacType, classify_mac, normalize_mac
from .model import DeviceClass
from .oui import manufacturer as oui_manufacturer

# Hostnames that are too generic to be a unique fusion key. Using one of these
# as the sole identity would merge unrelated devices of the same category.
_GENERIC_HOSTNAMES = {
    "", "unknown", "none", "null", "n/a", "na", "device", "phone", "tablet",
    "smartphone", "pc", "computer", "laptop", "desktop", "server", "nas",
    "router", "ap", "gateway", "tv", "smart-tv", "smarttv", "iot", "sensor",
    "generic", "default", "localhost", "android", "android-device",
    "iphone", "ipad", "ios", "galaxy", "redmi", "xiaomi", "realme", "moto",
    "motorola", "amazon", "echo", "alexa", "firetv", "firestick", "roku",
    "windows", "linux", "macbook", "imac", "mac", "apple", "huawei", "honor",
    "oppo", "vivo", "oneplus", "pixel", "sony", "nintendo", "switch", "xbox",
    "playstation", "ps4", "ps5", "esp", "esp32", "esp8266", "arduino",
    "raspberry", "rpi", "chromecast", "appletv", "shield", "webos", "tizen",
}

# A hostname is considered "specific" (unique enough to fuse by) when it is not
# in the generic blacklist AND it carries a discriminating token: a digit, or a
# separator followed by a token of length >= 4. This accepts "moto-g42",
# "realme-9i", "amazon-07a4dcc48", "soporte24hwww" while rejecting "android",
# "iphone", "tv".
_SPECIFIC_TOKEN_RE = re.compile(r"(?:\d|[._-][a-z0-9]{4,})", re.I)


def normalize_hostname(hostname: Optional[str]) -> str:
    if not hostname:
        return ""
    return str(hostname).strip().lower()


def is_generic_hostname(hostname: Optional[str]) -> bool:
    """True when the hostname is too generic to be a stable fusion key."""
    h = normalize_hostname(hostname)
    if h in _GENERIC_HOSTNAMES:
        return True
    # Pure brand/model word with no discriminating token (e.g. "echo", "galaxy").
    if not _SPECIFIC_TOKEN_RE.search(h):
        return True
    return False


@dataclass
class StableFingerprint:
    fingerprint_id: str
    method: str          # "hostname" | "mac" | "mac_randomized"
    confidence: float    # 0..1 — how stable we expect this id to be
    generic_hostname: bool

    def to_dict(self) -> dict:
        return {
            "fingerprint_id": self.fingerprint_id,
            "method": self.method,
            "confidence": round(self.confidence, 4),
            "generic_hostname": self.generic_hostname,
        }


def _hmac(secret: bytes, payload: str) -> str:
    return hmac.new(secret, payload.encode("utf-8"), hashlib.sha256).hexdigest()[:16]


def stable_fingerprint(
    secret: bytes,
    hostname: Optional[str],
    manufacturer: Optional[str],
    device_class: Optional[DeviceClass],
    mac: Optional[str],
    mac_type: Optional[MacType] = None,
) -> StableFingerprint:
    """Compute a stable fingerprint_id for a device observation.

    Priority:
      1. Specific hostname  -> HMAC("h|<hostname>|<manufacturer>|<device_class>")
         (stable across reconnects and bands; the OS-reported name does not
         change with the MAC).
      2. Global (non-randomized) MAC -> HMAC("m|<mac>")  (stable for devices
         that do not randomize; e.g. IoT, smart speakers, TVs).
      3. Randomized MAC, no usable hostname -> HMAC("m|<mac>") with low
         confidence (the MAC may rotate; we cannot do better without a
         hostname, but we still keep a consistent id within a single MAC
         lifetime).

    The "h|" / "m|" prefix prevents collisions between the two namespaces.
    """
    generic = is_generic_hostname(hostname)
    h = normalize_hostname(hostname)
    mfr = (manufacturer or "").strip().lower() or "-"
    dclass = (device_class.value if isinstance(device_class, DeviceClass) else str(device_class or "-")).lower()

    if not generic and h:
        fp = _hmac(secret, f"h|{h}|{mfr}|{dclass}")
        return StableFingerprint(fp, "hostname", 0.9, generic)

    mac_norm = normalize_mac(mac)
    if mac_norm:
        if mac_type is None:
            mac_type = classify_mac(mac)
        randomized = mac_type in (MacType.LOCAL_RANDOMIZED, MacType.LOCAL_ADMINISTERED, MacType.INVALID)
        fp = _hmac(secret, f"m|{mac_norm}")
        if randomized:
            return StableFingerprint(fp, "mac_randomized", 0.3, generic)
        return StableFingerprint(fp, "mac", 0.7, generic)

    # Last resort: derive from hostname even if generic, so we never return an
    # empty id. Two generic-hostname devices of the same class will share an id
    # here — acceptable since we had nothing better.
    fp = _hmac(secret, f"h|{h or 'unknown'}|{mfr}|{dclass}")
    return StableFingerprint(fp, "hostname", 0.2, generic)


def stable_fingerprint_from_observation(secret: bytes, obs) -> StableFingerprint:
    """Convenience: build a StableFingerprint from an identity.Observation.

    Resolves the manufacturer from the OUI only when the MAC is not randomized,
    matching the engine's behavior. Device class is inferred via the classifier
    (the Observation model does not carry it).
    """
    from .classifier import infer_device_class
    from .mac import classify_mac
    mac_type = classify_mac(obs.mac)
    randomized = mac_type in (MacType.LOCAL_RANDOMIZED, MacType.LOCAL_ADMINISTERED, MacType.INVALID)
    mfr = None
    if not randomized and obs.mac:
        mfr = oui_manufacturer(obs.mac)
    device_class, _ = infer_device_class(obs.hostname, mfr, obs.protocol, obs.band, obs.mac)
    return stable_fingerprint(secret, obs.hostname, mfr, device_class, obs.mac, mac_type)
