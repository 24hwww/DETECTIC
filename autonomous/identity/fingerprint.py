"""Model fingerprinting — probabilistic model-guess layer.

Model identification is evidence-based and probabilistic. We NEVER assert a
specific model with certainty unless the hostname encodes it. Candidates are
returned with per-candidate confidence.
"""

from __future__ import annotations

import re
from typing import List, Optional, Tuple

from .model import Evidence, EvidenceType, ModelCandidate

# Known hostname -> (brand, model) exact or prefix mappings derived from observed
# data and vendor naming conventions. These are conservative.
_KNOWN_MODELS = [
    (re.compile(r"^moto-g42", re.I), "Motorola", "Moto G42"),
    (re.compile(r"^moto-g54", re.I), "Motorola", "Moto G54 5G"),
    (re.compile(r"^moto-g(52|51|53|72|73|82|84)", re.I), "Motorola", "Moto G-series"),
    (re.compile(r"^moto-(e\d+|g\d+)[-_]?", re.I), "Motorola", "Moto"),
    (re.compile(r"^realme-9i", re.I), "Realme", "Realme 9i"),
    (re.compile(r"^realme-(c\d+|gt|narzo|11|10|9)", re.I), "Realme", "Realme"),
    (re.compile(r"^realme-", re.I), "Realme", "Realme"),
    (re.compile(r"^iphone", re.I), "Apple", "iPhone"),
    (re.compile(r"^ipad", re.I), "Apple", "iPad"),
    (re.compile(r"^galaxy-(s\d+|a\d+|note\d+|tab)", re.I), "Samsung", "Galaxy"),
    (re.compile(r"^galaxy", re.I), "Samsung", "Galaxy"),
    (re.compile(r"^pixel-?(\d|xl|pro|6|7|8)", re.I), "Google", "Pixel"),
    (re.compile(r"^redmi", re.I), "Xiaomi", "Redmi"),
    (re.compile(r"^poco", re.I), "Xiaomi", "POCO"),
    (re.compile(r"^mi-?\d", re.I), "Xiaomi", "Mi"),
    (re.compile(r"^amazon-", re.I), "Amazon", "Amazon Echo/device"),
    (re.compile(r"^echo", re.I), "Amazon", "Echo"),
    (re.compile(r"^firetv|^firestick", re.I), "Amazon", "Fire TV"),
    (re.compile(r"^roku", re.I), "Roku", "Roku"),
    (re.compile(r"^xbox", re.I), "Microsoft", "Xbox"),
    (re.compile(r"^playstation|^ps[0-9]", re.I), "Sony", "PlayStation"),
]

_MODEL_WEIGHT = 0.45


def fingerprint(
    hostname: Optional[str], brand: Optional[str]
) -> Tuple[Optional[str], List[ModelCandidate], List[Evidence]]:
    """Return (model_guess, candidates, evidence)."""
    evidence: List[Evidence] = []
    if not hostname:
        return None, [], evidence

    for pat, vendor, model in _KNOWN_MODELS:
        m = pat.search(hostname)
        if m:
            evidence.append(
                Evidence(
                    type=EvidenceType.HOSTNAME_MATCH,
                    value=f"{hostname} => {vendor} {model}",
                    weight=_MODEL_WEIGHT,
                    source="hostname",
                )
            )
            # Single primary candidate when hostname encodes the model.
            candidates = [ModelCandidate(model=model, confidence=_MODEL_WEIGHT)]
            # If brand is corroborated by OUI, raise confidence.
            if brand and brand.lower() in (vendor.lower(),):
                candidates[0].confidence = min(0.82, _MODEL_WEIGHT + 0.37)
                evidence.append(
                    Evidence(
                        type=EvidenceType.OUI_MATCH,
                        value=f"OUI brand matches {vendor}",
                        weight=0.3,
                        source="oui",
                    )
                )
            return model, candidates, evidence

    return None, [], evidence
