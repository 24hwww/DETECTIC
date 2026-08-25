"""Device-class inference from observation evidence.

Evidence sources (in priority order of specificity):
  1. hostname token patterns (e.g. "moto-g42", "amazon-...", "*-tv")
  2. OUI class_hint (weak, only as fallback)
  3. protocol/band heuristics (e.g. 802.11ac/ax on 5GHz => modern client)

Device class is NEVER asserted from OUI alone with high confidence.
"""

from __future__ import annotations

import re
from typing import List, Optional, Tuple

from .model import DeviceClass, Evidence, EvidenceType
from .oui import OuiResolver

_resolver = OuiResolver()

# (compiled regex, device_class, weight)
_HOSTNAME_RULES: List[Tuple[re.Pattern, DeviceClass, float]] = [
    (re.compile(r"^moto[-_]?", re.I), DeviceClass.SMARTPHONE, 0.4),
    (re.compile(r"realme", re.I), DeviceClass.SMARTPHONE, 0.4),
    (re.compile(r"(iphone|ipad)", re.I), DeviceClass.SMARTPHONE, 0.4),
    (re.compile(r"(galaxy|sm-[a-z0-9]+)", re.I), DeviceClass.SMARTPHONE, 0.4),
    (re.compile(r"(redmi|poco|mi[0-9])", re.I), DeviceClass.SMARTPHONE, 0.4),
    (re.compile(r"(pixel)", re.I), DeviceClass.SMARTPHONE, 0.4),
    (re.compile(r"(oneplus|oppo|vivo|huawei|honor|xiaomi)", re.I), DeviceClass.SMARTPHONE, 0.35),
    (re.compile(r"(smarttv|[-_]tv|androidtv|webos|tizen)", re.I), DeviceClass.TV, 0.45),
    (re.compile(r"(firetv|firestick|roku|chromecast|shield|appletv|-mi-box|mi-box)", re.I), DeviceClass.STREAMING_DEVICE, 0.45),
    (re.compile(r"(echo|alexa|echodot|echo-)", re.I), DeviceClass.SMART_SPEAKER, 0.45),
    (re.compile(r"^amazon-", re.I), DeviceClass.SMART_SPEAKER, 0.35),
    (re.compile(r"(xbox)", re.I), DeviceClass.GAME_CONSOLE, 0.45),
    (re.compile(r"(playstation|ps[0-9]|ps5|ps4)", re.I), DeviceClass.GAME_CONSOLE, 0.45),
    (re.compile(r"(nintendo|switch)", re.I), DeviceClass.GAME_CONSOLE, 0.45),
    (re.compile(r"(camera|cam|webcam|nestcam|arlo)", re.I), DeviceClass.CAMERA, 0.4),
    (re.compile(r"(printer|hp[0-9]|epson|canon|brother|lexmark)", re.I), DeviceClass.PRINTER, 0.4),
    (re.compile(r"(laptop|notebook|pc[-_]|desktop|-pc$)", re.I), DeviceClass.LAPTOP, 0.4),
    (re.compile(r"(router|[-_]ap$|gateway|ap[0-9])", re.I), DeviceClass.ACCESS_POINT, 0.4),
    (re.compile(r"(iot|esp|arduino|sensor|thermostat|bulb|plug)", re.I), DeviceClass.IOT, 0.4),
]

_BAND_PROTOCOL_SCORE = 0.1


def extract_brand(hostname: Optional[str], manufacturer: Optional[str]) -> Optional[str]:
    """Return a normalized brand hint from hostname or manufacturer."""
    if hostname:
        low = hostname.lower()
        for brand, pat in [
            ("Motorola", r"moto[-_]"),
            ("Realme", r"realme"),
            ("Apple", r"iphone|ipad|mac|apple"),
            ("Samsung", r"galaxy|sm-"),
            ("Xiaomi", r"redmi|poco|mi[0-9]|xiaomi"),
            ("Google", r"pixel"),
            ("OnePlus", r"oneplus"),
            ("Oppo", r"oppo"),
            ("Vivo", r"vivo"),
            ("Huawei", r"huawei|honor"),
            ("Amazon", r"amazon|echo|alexa|fire"),
            ("LG", r"\blg[-_]"),
            ("Sony", r"sony|playstation"),
            ("Nintendo", r"nintendo|switch"),
            ("Microsoft", r"xbox"),
            ("TP-Link", r"tp-link|reyes|archer"),
            ("Roku", r"roku"),
            ("Vizio", r"vizio"),
        ]:
            if re.search(pat, low, re.I):
                return brand
    if manufacturer:
        return manufacturer
    return None


def infer_device_class(
    hostname: Optional[str],
    manufacturer: Optional[str],
    protocol: Optional[str] = None,
    band: Optional[str] = None,
    mac: Optional[str] = None,
) -> Tuple[DeviceClass, List[Evidence]]:
    """Return (device_class, evidence_list). Default class is UNKNOWN."""
    evidence: List[Evidence] = []

    # 1. Hostname token rules.
    if hostname:
        for pat, cls, weight in _HOSTNAME_RULES:
            if pat.search(hostname):
                evidence.append(
                    Evidence(
                        type=EvidenceType.HOSTNAME_MATCH,
                        value=f"{hostname} => {cls.value}",
                        weight=weight,
                        source="hostname",
                    )
                )
                # hostname rule is specific enough to return immediately
                return cls, evidence

    # 2. OUI class_hint (weak evidence only — never assert class from OUI alone).
    if mac:
        hint = _resolver.class_hint(mac)
        if hint and hint in DeviceClass.__members__:
            evidence.append(
                Evidence(
                    type=EvidenceType.OUI_MATCH,
                    value=f"OUI hint => {hint}",
                    weight=0.1,
                    source="oui",
                )
            )

    if manufacturer:
        evidence.append(
            Evidence(
                type=EvidenceType.OUI_MATCH,
                value=f"manufacturer={manufacturer} (no class)",
                weight=0.05,
                source="oui",
            )
        )

    # 3. Protocol/band heuristic (very weak).
    if protocol in ("ac", "ax") or band == "5GHz":
        evidence.append(
            Evidence(
                type=EvidenceType.PROTOCOL_MATCH,
                value=f"protocol={protocol} band={band}",
                weight=_BAND_PROTOCOL_SCORE,
                source="observation",
            )
        )

    # No hostname rule matched: we do not assert a device class from OUI alone.
    return DeviceClass.UNKNOWN, evidence
