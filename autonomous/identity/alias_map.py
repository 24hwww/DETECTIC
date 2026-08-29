"""Bidirectional alias map: stable fingerprint_id <-> MAC pseudonyms.

A single physical device may present multiple MAC pseudonyms over time
(randomized MAC rotation, dual-band per-band MACs). The alias map records every
MAC pseudonym observed for a given fingerprint_id so the device's history can
be reconstructed even when the MAC changes.

Persisted as a small JSON document alongside the identity state. Failures are
swallowed so alias tracking can never break the observation pipeline.
"""

from __future__ import annotations

import json
import os
import threading
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Set


@dataclass
class AliasRecord:
    fingerprint_id: str
    pseudonyms: Set[str] = field(default_factory=set)
    first_seen: int = 0
    last_seen: int = 0
    hostnames: Set[str] = field(default_factory=set)
    bands: Set[str] = field(default_factory=set)

    def to_dict(self) -> dict:
        return {
            "fingerprint_id": self.fingerprint_id,
            "pseudonyms": sorted(self.pseudonyms),
            "first_seen": self.first_seen,
            "last_seen": self.last_seen,
            "hostnames": sorted(h for h in self.hostnames if h),
            "bands": sorted(self.bands),
        }

    @classmethod
    def from_dict(cls, d: dict) -> "AliasRecord":
        return cls(
            fingerprint_id=d["fingerprint_id"],
            pseudonyms=set(d.get("pseudonyms", [])),
            first_seen=int(d.get("first_seen", 0)),
            last_seen=int(d.get("last_seen", 0)),
            hostnames=set(d.get("hostnames", [])),
            bands=set(d.get("bands", [])),
        )


class AliasMap:
    """In-memory bidirectional alias map."""

    def __init__(self) -> None:
        self._by_fp: Dict[str, AliasRecord] = {}
        self._pseudo_to_fp: Dict[str, str] = {}

    def register(
        self,
        fingerprint_id: str,
        pseudonym: str,
        *,
        ts: int = 0,
        hostname: Optional[str] = None,
        band: Optional[str] = None,
    ) -> AliasRecord:
        rec = self._by_fp.get(fingerprint_id)
        if rec is None:
            rec = AliasRecord(fingerprint_id=fingerprint_id, first_seen=ts, last_seen=ts)
            self._by_fp[fingerprint_id] = rec
        rec.pseudonyms.add(pseudonym)
        self._pseudo_to_fp[pseudonym] = fingerprint_id
        if ts:
            rec.last_seen = max(rec.last_seen, ts)
            if not rec.first_seen:
                rec.first_seen = ts
            else:
                rec.first_seen = min(rec.first_seen, ts)
        if hostname:
            rec.hostnames.add(hostname)
        if band:
            rec.bands.add(band)
        return rec

    def get(self, fingerprint_id: str) -> Optional[AliasRecord]:
        return self._by_fp.get(fingerprint_id)

    def fingerprint_of(self, pseudonym: str) -> Optional[str]:
        return self._pseudo_to_fp.get(pseudonym)

    def aliases(self, fingerprint_id: str) -> List[str]:
        rec = self._by_fp.get(fingerprint_id)
        return sorted(rec.pseudonyms) if rec else []

    def all(self) -> Dict[str, AliasRecord]:
        return dict(self._by_fp)

    def to_dict(self) -> dict:
        return {
            "aliases": [r.to_dict() for r in self._by_fp.values()],
            "pseudo_to_fp": dict(self._pseudo_to_fp),
        }

    @classmethod
    def from_dict(cls, d: dict) -> "AliasMap":
        m = cls()
        for r in d.get("aliases", []):
            rec = AliasRecord.from_dict(r)
            m._by_fp[rec.fingerprint_id] = rec
            for p in rec.pseudonyms:
                m._pseudo_to_fp[p] = rec.fingerprint_id
        # Backfill reverse index from pseudo_to_fp if aliases were empty
        for p, fp in d.get("pseudo_to_fp", {}).items():
            m._pseudo_to_fp.setdefault(p, fp)
            if fp in m._by_fp:
                m._by_fp[fp].pseudonyms.add(p)
        return m


class JsonAliasMap(AliasMap):
    """File-backed alias map for cross-run persistence."""

    def __init__(self, path: str):
        super().__init__()
        self._path = path
        self._lock = threading.Lock()
        self._load()

    def _load(self) -> None:
        try:
            if os.path.exists(self._path):
                with open(self._path, "r", encoding="utf-8") as f:
                    data = json.load(f)
                loaded = AliasMap.from_dict(data)
                self._by_fp = loaded._by_fp
                self._pseudo_to_fp = loaded._pseudo_to_fp
        except (OSError, ValueError, TypeError):
            pass

    def _save(self) -> None:
        try:
            os.makedirs(os.path.dirname(os.path.abspath(self._path)), exist_ok=True)
            tmp = self._path + ".tmp"
            with open(tmp, "w", encoding="utf-8") as f:
                json.dump(self.to_dict(), f, indent=2)
            os.replace(tmp, self._path)
        except (OSError, ValueError, TypeError):
            pass

    def register(self, fingerprint_id, pseudonym, *, ts=0, hostname=None, band=None) -> AliasRecord:
        rec = super().register(fingerprint_id, pseudonym, ts=ts, hostname=hostname, band=band)
        with self._lock:
            self._save()
        return rec
