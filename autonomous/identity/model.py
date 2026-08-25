"""DETECTIC Device Identity — normalized domain model.

This module is intentionally free of any TP-Link / EX520 specific structures.
It defines the observation and identity data structures used by the identity
engine. Adapters (e.g. the EX520 collector) must translate vendor-specific
responses into these structures before passing them to the engine.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import List, Optional


class MacType(str, Enum):
    GLOBAL_UNICAST = "GLOBAL_UNICAST"
    GLOBAL_MULTICAST = "GLOBAL_MULTICAST"
    LOCAL_ADMINISTERED = "LOCAL_ADMINISTERED"
    LOCAL_RANDOMIZED = "LOCAL_RANDOMIZED"
    INVALID = "INVALID"
    UNKNOWN = "UNKNOWN"


class EntityType(str, Enum):
    CONNECTED_CLIENT = "CONNECTED_CLIENT"
    NON_CONNECTED_CLIENT = "NON_CONNECTED_CLIENT"
    ACCESS_POINT = "ACCESS_POINT"
    UNKNOWN = "UNKNOWN"


class DeviceClass(str, Enum):
    SMARTPHONE = "SMARTPHONE"
    TABLET = "TABLET"
    LAPTOP = "LAPTOP"
    DESKTOP = "DESKTOP"
    TV = "TV"
    STREAMING_DEVICE = "STREAMING_DEVICE"
    GAME_CONSOLE = "GAME_CONSOLE"
    SMART_SPEAKER = "SMART_SPEAKER"
    IOT = "IOT"
    CAMERA = "CAMERA"
    PRINTER = "PRINTER"
    ROUTER = "ROUTER"
    ACCESS_POINT = "ACCESS_POINT"
    UNKNOWN = "UNKNOWN"


class AssociationState(str, Enum):
    ASSOCIATED = "ASSOCIATED"
    DISASSOCIATED = "DISASSOCIATED"
    UNKNOWN = "UNKNOWN"


class EvidenceType(str, Enum):
    OUI_MATCH = "OUI_MATCH"
    HOSTNAME_MATCH = "HOSTNAME_MATCH"
    CAPABILITY_MATCH = "CAPABILITY_MATCH"
    VENDOR_IE_MATCH = "VENDOR_IE_MATCH"
    BAND_MATCH = "BAND_MATCH"
    PROTOCOL_MATCH = "PROTOCOL_MATCH"
    PHY_MATCH = "PHY_MATCH"
    TEMPORAL_CORRELATION = "TEMPORAL_CORRELATION"
    RF_CORRELATION = "RF_CORRELATION"
    HISTORICAL_FINGERPRINT = "HISTORICAL_FINGERPRINT"


@dataclass
class Evidence:
    """A single piece of explainable inference evidence."""

    type: EvidenceType
    value: str
    weight: float
    source: str

    def to_dict(self) -> dict:
        return {
            "type": self.type.value,
            "value": self.value,
            "weight": round(self.weight, 4),
            "source": self.source,
        }


@dataclass
class Observation:
    """Normalized observation produced by an adapter (EX520, future sensors).

    Raw MAC / BSSID are accepted by the engine for local OUI processing only and
    are never persisted or transmitted outside the engine boundary.
    """

    sensor_id: str
    timestamp: int
    entity_type: EntityType = EntityType.UNKNOWN
    # Raw identifiers — used only internally for OUI / pseudonym computation.
    mac: Optional[str] = None
    bssid: Optional[str] = None
    # Extracted / observed attributes (all optional).
    ssid: Optional[str] = None
    hostname: Optional[str] = None
    band: Optional[str] = None
    channel: Optional[int] = None
    protocol: Optional[str] = None
    phy_rate_kbps: Optional[int] = None
    rssi: Optional[int] = None
    noise: Optional[int] = None
    signal_level: Optional[int] = None
    capabilities: dict = field(default_factory=dict)
    vendor_information: dict = field(default_factory=dict)
    association_state: AssociationState = AssociationState.UNKNOWN
    source: str = "unknown"

    def to_dict(self) -> dict:
        return {
            "sensor_id": self.sensor_id,
            "timestamp": self.timestamp,
            "entity_type": self.entity_type.value,
            "ssid": self.ssid,
            "hostname": self.hostname,
            "band": self.band,
            "channel": self.channel,
            "protocol": self.protocol,
            "phy_rate_kbps": self.phy_rate_kbps,
            "rssi": self.rssi,
            "noise": self.noise,
            "signal_level": self.signal_level,
            "capabilities": self.capabilities,
            "vendor_information": self.vendor_information,
            "association_state": self.association_state.value,
            "source": self.source,
        }


@dataclass
class DeviceIdentity:
    """Result of identity inference for a single observation."""

    pseudonym: str
    mac_type: MacType
    entity_type: EntityType
    manufacturer: Optional[str] = None
    device_class: DeviceClass = DeviceClass.UNKNOWN
    model_guess: Optional[str] = None
    confidence: float = 0.0
    bssid_pseudonym: Optional[str] = None
    ssid: Optional[str] = None
    evidence: List[Evidence] = field(default_factory=list)
    candidates: List["ModelCandidate"] = field(default_factory=list)

    def confidence_label(self) -> str:
        from .evidence import confidence_label

        return confidence_label(self.confidence)

    def to_dict(self) -> dict:
        return {
            "pseudonym": self.pseudonym,
            "mac_type": self.mac_type.value,
            "entity_type": self.entity_type.value,
            "manufacturer": self.manufacturer,
            "device_class": self.device_class.value,
            "model_guess": self.model_guess,
            "confidence": round(self.confidence, 4),
            "confidence_label": self.confidence_label(),
            "bssid_pseudonym": self.bssid_pseudonym,
            "ssid": self.ssid,
            "evidence": [e.to_dict() for e in self.evidence],
            "candidates": [c.to_dict() for c in self.candidates],
        }


@dataclass
class ModelCandidate:
    model: str
    confidence: float

    def to_dict(self) -> dict:
        return {"model": self.model, "confidence": round(self.confidence, 4)}


@dataclass
class WifiNetworkIdentity:
    """Identity for an observed Wi-Fi network (external AP or own)."""

    bssid_pseudonym: str
    ssid: Optional[str] = None
    manufacturer: Optional[str] = None
    band: Optional[str] = None
    channel: Optional[int] = None
    rssi: Optional[int] = None
    protocol: Optional[str] = None
    security: Optional[str] = None
    first_seen: Optional[int] = None
    last_seen: Optional[int] = None

    def to_dict(self) -> dict:
        return {
            "bssid_pseudonym": self.bssid_pseudonym,
            "ssid": self.ssid,
            "manufacturer": self.manufacturer,
            "band": self.band,
            "channel": self.channel,
            "rssi": self.rssi,
            "protocol": self.protocol,
            "security": self.security,
            "first_seen": self.first_seen,
            "last_seen": self.last_seen,
        }
