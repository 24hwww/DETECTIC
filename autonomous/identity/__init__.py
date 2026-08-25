"""DETECTIC Device Identity & Wi-Fi Fingerprinting Engine.

Vendor-agnostic identity inference. Adapters (e.g. EX520 collector) must
translate raw observations into the `Observation` model before calling the
engine. Raw MAC/BSSID never leave the engine boundary.
"""

from .classifier import extract_brand, infer_device_class
from .engine import DeviceIdentityEngine
from .evidence import (
    Evidence,
    EvidenceType,
    combine_confidence,
    confidence_label,
    confidence_word,
    make_evidence,
)
from .fingerprint import fingerprint
from .mac import classify_mac, is_randomized, normalize_mac, oui
from .model import (
    AssociationState,
    DeviceClass,
    DeviceIdentity,
    EntityType,
    Evidence as EvidenceItem,
    MacType,
    ModelCandidate,
    Observation,
    WifiNetworkIdentity,
)
from .oui import OuiResolver, manufacturer, resolve
from .repository import InMemoryRepositories
from .temporal import TemporalRecord, update_record

__all__ = [
    "DeviceIdentityEngine",
    "Observation",
    "DeviceIdentity",
    "WifiNetworkIdentity",
    "ModelCandidate",
    "EvidenceItem",
    "Evidence",
    "EvidenceType",
    "MacType",
    "EntityType",
    "DeviceClass",
    "AssociationState",
    "OuiResolver",
    "InMemoryRepositories",
    "TemporalRecord",
    "classify_mac",
    "is_randomized",
    "normalize_mac",
    "oui",
    "manufacturer",
    "resolve",
    "extract_brand",
    "infer_device_class",
    "fingerprint",
    "combine_confidence",
    "confidence_label",
    "confidence_word",
    "make_evidence",
    "update_record",
]
