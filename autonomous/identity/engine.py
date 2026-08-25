"""Device Identity Engine — orchestrates the full inference pipeline.

This module is vendor-agnostic. It consumes a normalized `Observation` and a
secret, and returns a `DeviceIdentity`. Raw MAC/BSSID are used ONLY locally for
OUI resolution and deterministic pseudonymization; they are never embedded in
the returned identity object or transmitted.
"""

from __future__ import annotations

import hashlib
import hmac
from typing import List, Optional

from .classifier import extract_brand, infer_device_class
from .evidence import Evidence, EvidenceType, combine_confidence, make_evidence
from .fingerprint import fingerprint
from .mac import MacType, classify_mac, normalize_mac
from .model import (
    DeviceIdentity,
    EntityType,
    Evidence as EvidenceItem,
    Observation,
)
from .oui import manufacturer as oui_manufacturer
from .repository import InMemoryRepositories
from .temporal import TemporalRecord, update_record


def _pseudonym(secret: bytes, identifier: str) -> str:
    norm = normalize_mac(identifier) or identifier
    return hmac.new(secret, norm.encode(), hashlib.sha256).hexdigest()[:16]


class DeviceIdentityEngine:
    def __init__(self, repos: Optional[InMemoryRepositories] = None):
        self.repos = repos or InMemoryRepositories()

    def identify(
        self,
        obs: Observation,
        secret: bytes,
        *,
        persist: bool = True,
    ) -> DeviceIdentity:
        # 1. MAC classification
        mac_type = classify_mac(obs.mac)
        randomized = mac_type in (MacType.LOCAL_RANDOMIZED, MacType.LOCAL_ADMINISTERED, MacType.INVALID)

        # 2. Manufacturer (only from globally-unique OUI)
        manufacturer: Optional[str] = None
        if not randomized and obs.mac:
            manufacturer = oui_manufacturer(obs.mac)
        if manufacturer:
            mac_type_ev = make_evidence(
                EvidenceType.OUI_MATCH, f"OUI => {manufacturer}", "oui", 0.35
            )
        else:
            mac_type_ev = make_evidence(
                EvidenceType.OUI_MATCH,
                f"no manufacturer (mac_type={mac_type.value})",
                "oui",
                0.0,
            )

        # 3. Brand + device class
        brand = extract_brand(obs.hostname, manufacturer)
        device_class, class_ev = infer_device_class(
            obs.hostname, manufacturer, obs.protocol, obs.band, obs.mac
        )

        # 4. Model fingerprint
        model_guess, candidates, model_ev = fingerprint(obs.hostname, brand)

        evidence: List[EvidenceItem] = [mac_type_ev] + class_ev + model_ev

        # 5. Pseudonyms
        pseudo = _pseudonym(secret, obs.mac) if obs.mac else _pseudonym(secret, obs.ssid or obs.source)
        bssid_pseudo = _pseudonym(secret, obs.bssid) if obs.bssid else None

        # 6. Temporal correlation (optional persistence)
        temporal: Optional[TemporalRecord] = None
        if persist and self.repos is not None:
            existing = self.repos.get_temporal(pseudo)
            temporal = update_record(
                existing,
                pseudo,
                obs.timestamp,
                connected=(obs.association_state.value == "ASSOCIATED"),
                band=obs.band,
                rssi=obs.rssi,
            )
            self.repos.put_temporal(temporal)
            if temporal and temporal.observation_count > 1:
                evidence.append(
                    make_evidence(
                        EvidenceType.TEMPORAL_CORRELATION,
                        f"seen {temporal.observation_count} times",
                        "temporal",
                        0.15,
                    )
                )

        # 7. Confidence
        confidence = combine_confidence(evidence)

        identity = DeviceIdentity(
            pseudonym=pseudo,
            mac_type=mac_type,
            entity_type=obs.entity_type
            if isinstance(obs.entity_type, EntityType)
            else EntityType(obs.entity_type),
            manufacturer=manufacturer,
            device_class=device_class,
            model_guess=model_guess,
            confidence=confidence,
            bssid_pseudonym=bssid_pseudo,
            ssid=obs.ssid,
            evidence=evidence,
            candidates=candidates,
        )

        if persist and self.repos is not None:
            self.repos.put_identity(identity)
            if candidates:
                self.repos.put_candidates(pseudo, [c.to_dict() for c in candidates])

        return identity

    def identify_network(
        self, obs: Observation, secret: bytes, *, persist: bool = True
    ) -> dict:
        """Identify an observed Wi-Fi network (AP) by BSSID."""
        bssid_pseudo = _pseudonym(secret, obs.bssid) if obs.bssid else None
        manufacturer = oui_manufacturer(obs.bssid) if obs.bssid else None
        net = {
            "bssid_pseudonym": bssid_pseudo,
            "ssid": obs.ssid,
            "manufacturer": manufacturer,
            "band": obs.band,
            "channel": obs.channel,
            "rssi": obs.rssi,
            "protocol": obs.protocol,
            "security": obs.vendor_information.get("security"),
            "first_seen": obs.timestamp,
            "last_seen": obs.timestamp,
        }
        if persist and self.repos is not None and bssid_pseudo:
            self.repos.put_network(net)
        return net
