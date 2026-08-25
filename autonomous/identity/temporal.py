"""Temporal correlation of observations per pseudonymous identity.

Tracks first_seen, last_seen, observation_count, connected/disconnected counts,
bands seen, RSSI history. Pure domain logic; persistence is delegated to a
repository implementation.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import List, Optional


@dataclass
class TemporalRecord:
    pseudonym: str
    first_seen: int
    last_seen: int
    observation_count: int = 1
    connected_count: int = 0
    disconnected_count: int = 0
    bands_seen: set = field(default_factory=set)
    rssi_history: List[int] = field(default_factory=list)

    def to_dict(self) -> dict:
        return {
            "pseudonym": self.pseudonym,
            "first_seen": self.first_seen,
            "last_seen": self.last_seen,
            "observation_count": self.observation_count,
            "connected_count": self.connected_count,
            "disconnected_count": self.disconnected_count,
            "bands_seen": sorted(self.bands_seen),
            "rssi_history": self.rssi_history[-20:],
        }

    @property
    def presence_state(self) -> str:
        """first_seen / still_present / temporarily_missing / disappeared."""
        # Without an explicit timeout threshold we cannot infer disappearance
        # here; callers supply recency. Default mapping:
        if self.observation_count == 1:
            return "first_seen"
        return "still_present"

    @classmethod
    def from_dict(cls, d: dict) -> "TemporalRecord":
        return cls(
            pseudonym=d["pseudonym"],
            first_seen=d["first_seen"],
            last_seen=d["last_seen"],
            observation_count=d.get("observation_count", 1),
            connected_count=d.get("connected_count", 0),
            disconnected_count=d.get("disconnected_count", 0),
            bands_seen=set(d.get("bands_seen", [])),
            rssi_history=list(d.get("rssi_history", [])),
        )


def update_record(
    rec: Optional[TemporalRecord], pseudonym: str, ts: int, *, connected: bool, band=None, rssi=None
) -> TemporalRecord:
    if rec is None:
        rec = TemporalRecord(pseudonym=pseudonym, first_seen=ts, last_seen=ts, observation_count=0)
    rec.last_seen = ts
    rec.observation_count += 1
    if connected:
        rec.connected_count += 1
    else:
        rec.disconnected_count += 1
    if band:
        rec.bands_seen.add(band)
    if rssi is not None:
        rec.rssi_history.append(rssi)
    return rec
