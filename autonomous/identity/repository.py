"""Repository interfaces for the identity domain.

The identity engine operates ONLY against these abstractions. No Drizzle,
Prisma, SQLite, PostgreSQL, or D1 specifics appear in the domain layer.

Concrete implementations (in-memory, JSON file, or D1-backed) are provided
separately and injected by the application/collector.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Dict, List, Optional

from .model import DeviceIdentity
from .temporal import TemporalRecord


class ObservationRepository(ABC):
    """Persists raw normalized observations (privacy-safe: no raw MAC/BSSID)."""

    @abstractmethod
    def add(self, observation: dict, identity: DeviceIdentity) -> None:
        ...


class IdentityRepository(ABC):
    """Persists derived device identity state keyed by pseudonym."""

    @abstractmethod
    def get(self, pseudonym: str) -> Optional[DeviceIdentity]:
        ...

    @abstractmethod
    def put(self, identity: DeviceIdentity) -> None:
        ...


class TemporalRepository(ABC):
    """Persists temporal correlation records keyed by pseudonym."""

    @abstractmethod
    def get(self, pseudonym: str) -> Optional[TemporalRecord]:
        ...

    @abstractmethod
    def put(self, record: TemporalRecord) -> None:
        ...


class FingerprintRepository(ABC):
    """Persists model-candidate fingerprints keyed by pseudonym."""

    @abstractmethod
    def get_candidates(self, pseudonym: str) -> List[dict]:
        ...

    @abstractmethod
    def put_candidates(self, pseudonym: str, candidates: List[dict]) -> None:
        ...


class NetworkRepository(ABC):
    """Persists observed Wi-Fi network identities keyed by BSSID pseudonym."""

    @abstractmethod
    def get(self, bssid_pseudonym: str) -> Optional[dict]:
        ...

    @abstractmethod
    def put(self, network: dict) -> None:
        ...


class InMemoryRepositories:
    """Default null/in-memory implementation used by tests and lightweight runs."""

    def __init__(self) -> None:
        self.observations: List[Dict] = []
        self.identities: Dict[str, DeviceIdentity] = {}
        self.temporals: Dict[str, TemporalRecord] = {}
        self.fingerprints: Dict[str, List[dict]] = {}
        self.networks: Dict[str, dict] = {}

    # --- ObservationRepository ---
    def add(self, observation: dict, identity: DeviceIdentity) -> None:
        self.observations.append(
            {"observation": observation, "identity": identity.to_dict()}
        )

    # --- IdentityRepository ---
    def get_identity(self, pseudonym: str) -> Optional[DeviceIdentity]:
        return self.identities.get(pseudonym)

    def put_identity(self, identity: DeviceIdentity) -> None:
        self.identities[identity.pseudonym] = identity

    # --- TemporalRepository ---
    def get_temporal(self, pseudonym: str) -> Optional[TemporalRecord]:
        return self.temporals.get(pseudonym)

    def put_temporal(self, record: TemporalRecord) -> None:
        self.temporals[record.pseudonym] = record

    # --- FingerprintRepository ---
    def get_candidates(self, pseudonym: str) -> List[dict]:
        return self.fingerprints.get(pseudonym, [])

    def put_candidates(self, pseudonym: str, candidates: List[dict]) -> None:
        self.fingerprints[pseudonym] = candidates

    # --- NetworkRepository ---
    def get_network(self, bssid_pseudonym: str) -> Optional[dict]:
        return self.networks.get(bssid_pseudonym)

    def put_network(self, network: dict) -> None:
        key = network.get("bssid_pseudonym")
        if key:
            self.networks[key] = network


import json
import os
import threading
from typing import Dict as _Dict


class JsonFileRepositories(InMemoryRepositories):
    """File-backed implementation for cross-run temporal correlation.

    Persists identity state, temporal records, fingerprints and networks to a
    single JSON document. Failures are swallowed so identity computation can
    never break the observation pipeline.
    """

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
                self.identities = {
                    k: (DeviceIdentity(**_strip_enums(v)) if isinstance(v, dict) else v)
                    for k, v in data.get("identities", {}).items()
                }
                self.temporals = {
                    k: TemporalRecord.from_dict(v)
                    for k, v in data.get("temporals", {}).items()
                }
                self.fingerprints = data.get("fingerprints", {})
                self.networks = data.get("networks", {})
        except (OSError, ValueError, TypeError):
            # Start fresh if the file is corrupt/unreadable.
            pass

    def _save(self) -> None:
        try:
            os.makedirs(os.path.dirname(os.path.abspath(self._path)), exist_ok=True)
            data = {
                "identities": {k: v.to_dict() for k, v in self.identities.items()},
                "temporals": {k: v.to_dict() for k, v in self.temporals.items()},
                "fingerprints": self.fingerprints,
                "networks": self.networks,
            }
            tmp = self._path + ".tmp"
            with open(tmp, "w", encoding="utf-8") as f:
                json.dump(data, f, indent=2)
            os.replace(tmp, self._path)
        except (OSError, ValueError, TypeError):
            pass

    def put_identity(self, identity: DeviceIdentity) -> None:
        super().put_identity(identity)
        with self._lock:
            self._save()

    def put_temporal(self, record: TemporalRecord) -> None:
        super().put_temporal(record)
        with self._lock:
            self._save()

    def put_candidates(self, pseudonym: str, candidates: List[dict]) -> None:
        super().put_candidates(pseudonym, candidates)
        with self._lock:
            self._save()

    def put_network(self, network: dict) -> None:
        super().put_network(network)
        with self._lock:
            self._save()


def _strip_enums(v: _Dict) -> _Dict:
    """Best-effort reconstruct DeviceIdentity; on failure return dict as-is."""
    try:
        v = dict(v)
        v["mac_type"] = MacType(v["mac_type"])
        v["entity_type"] = EntityType(v["entity_type"])
        v["device_class"] = DeviceClass(v["device_class"])
        v["evidence"] = [Evidence(**e) for e in v.get("evidence", [])]
        v["candidates"] = [ModelCandidate(**c) for c in v.get("candidates", [])]
        return v
    except (KeyError, ValueError, TypeError):
        return v

