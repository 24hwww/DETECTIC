"""Evidence weights and confidence scoring — deterministic and testable."""

from __future__ import annotations

from .model import Evidence, EvidenceType

# Evidence weights in [0, 1]. These are combined (not simply averaged) by the
# confidence scorer so that multiple independent confirmations raise confidence
# without exceeding 1.0.
EVIDENCE_WEIGHTS = {
    EvidenceType.OUI_MATCH: 0.35,
    EvidenceType.HOSTNAME_MATCH: 0.40,
    EvidenceType.CAPABILITY_MATCH: 0.20,
    EvidenceType.VENDOR_IE_MATCH: 0.30,
    EvidenceType.BAND_MATCH: 0.10,
    EvidenceType.PROTOCOL_MATCH: 0.10,
    EvidenceType.PHY_MATCH: 0.15,
    EvidenceType.TEMPORAL_CORRELATION: 0.15,
    EvidenceType.RF_CORRELATION: 0.10,
    EvidenceType.HISTORICAL_FINGERPRINT: 0.25,
}

# Maximum contribution per evidence type (so one type cannot saturate alone
# unless it is a very strong signal).
MAX_PER_TYPE = 0.6


def make_evidence(
    etype: EvidenceType, value: str, source: str, weight: float | None = None
) -> Evidence:
    w = weight if weight is not None else EVIDENCE_WEIGHTS.get(etype, 0.1)
    return Evidence(type=etype, value=value, weight=w, source=source)


def confidence_label(score: float) -> str:
    """Map a [0,1] score to a human label per the DETECTIC spec."""
    if score >= 0.90:
        return "very high"
    if score >= 0.75:
        return "high"
    if score >= 0.50:
        return "medium"
    if score >= 0.25:
        return "low"
    return "very low"


def confidence_word(score: float) -> str:
    """Reporting word: Likely / Probable / Possible / Unknown."""
    if score >= 0.75:
        return "Likely"
    if score >= 0.50:
        return "Probable"
    if score >= 0.25:
        return "Possible"
    return "Unknown"


def combine_confidence(evidence: list[Evidence]) -> float:
    """Deterministically combine evidence into a bounded [0,1] score.

    Method: for each evidence item, contribution = min(weight, MAX_PER_TYPE).
    Total confidence = 1 - product(1 - contribution). This is a noisy-OR style
    combination: independent confirmations compound, but the result is always
    within [0, 1].
    """
    if not evidence:
        return 0.0
    remaining = 1.0
    for e in evidence:
        contrib = min(float(e.weight), MAX_PER_TYPE)
        contrib = max(0.0, min(1.0, contrib))
        remaining *= 1.0 - contrib
    score = 1.0 - remaining
    return round(max(0.0, min(1.0, score)), 4)
