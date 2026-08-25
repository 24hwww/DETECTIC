"""MAC address classification — independent of any vendor API."""

from __future__ import annotations

import re
from typing import Optional

from .model import MacType

# 12 hex digits, optional separators : - .
_MAC_RE = re.compile(r"^([0-9a-fA-F]{2}[:.\-]?){5}[0-9a-fA-F]{2}$")


def normalize_mac(mac: Optional[str]) -> Optional[str]:
    """Return lowercase, separator-free 12-hex MAC, or None if not parseable."""
    if not mac:
        return None
    s = str(mac).strip().lower()
    if not _MAC_RE.match(s):
        return None
    return re.sub(r"[:.\-]", "", s)


def _first_octet(mac_clean: str) -> int:
    return int(mac_clean[0:2], 16)


def classify_mac(mac: Optional[str]) -> MacType:
    """Classify a MAC address.

    Uses the two defined IEEE bits in the first octet:
      bit 0 (0x01) -> multicast (vs unicast)
      bit 1 (0x02) -> locally administered (vs globally unique / burned-in)
    """
    clean = normalize_mac(mac)
    if clean is None:
        return MacType.INVALID
    first = _first_octet(clean)
    multicast = bool(first & 0x01)
    local = bool(first & 0x02)

    if multicast and local:
        return MacType.LOCAL_ADMINISTERED
    if multicast:
        # Multicast + globally unique OUI is unusual but possible.
        return MacType.GLOBAL_MULTICAST
    if local:
        # On 802.11, the locally-administered bit on a client STA almost always
        # indicates a randomized/privacy MAC. We report the precise bit-level
        # class and the randomization inference separately.
        return MacType.LOCAL_RANDOMIZED
    return MacType.GLOBAL_UNICAST


def is_randomized(mac: Optional[str]) -> bool:
    return classify_mac(mac) == MacType.LOCAL_RANDOMIZED


def oui(prefix_clean_mac: Optional[str]) -> Optional[str]:
    """Return the 6-hex OUI (uppercase) or None."""
    clean = normalize_mac(prefix_clean_mac)
    if clean is None or len(clean) < 6:
        return None
    return clean[0:6].upper()
