"""OUI / manufacturer resolution.

Loads a local, versioned OUI database (data/oui.json) and resolves the first
three bytes of a MAC address to a manufacturer. Lookups are cached in memory.

The database is intentionally independent of application code and can be replaced
with a full IEEE MA-L dump without changing this module.
"""

from __future__ import annotations

import json
import os
from functools import lru_cache
from typing import Dict, List, Optional

from .mac import oui as oui_of

_DB_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data", "oui.json")


class OuiResolver:
    def __init__(self, db_path: str = _DB_PATH):
        self._db_path = db_path
        self._cache: Dict[str, Optional[dict]] = {}

    def _load(self) -> dict:
        try:
            with open(self._db_path, "r", encoding="utf-8") as f:
                data = json.load(f)
            return data.get("OUI", {})
        except (OSError, ValueError):
            return {}

    @lru_cache(maxsize=1)
    def _db(self) -> dict:
        return self._load()

    def resolve_raw(self, oui6: str) -> Optional[dict]:
        """Resolve a 6-hex OUI to {vendor, class_hint?} or None."""
        key = (oui6 or "").upper()
        if key not in self._cache:
            entry = self._db().get(key)
            self._cache[key] = entry
        return self._cache[key]

    def manufacturer(self, mac: Optional[str]) -> Optional[str]:
        oui6 = oui_of(mac)
        if not oui6:
            return None
        entry = self.resolve_raw(oui6)
        if entry and entry.get("vendor"):
            return entry["vendor"]
        return None

    def class_hint(self, mac: Optional[str]) -> Optional[str]:
        oui6 = oui_of(mac)
        if not oui6:
            return None
        entry = self.resolve_raw(oui6)
        return entry.get("class_hint") if entry else None

    def reload(self) -> None:
        self._cache.clear()
        self._db.cache_clear()


# Module-level default resolver (lazy singleton).
_default_resolver = OuiResolver()


def resolve(mac: Optional[str]) -> Optional[dict]:
    return _default_resolver.resolve_raw(oui_of(mac) or "")


def manufacturer(mac: Optional[str]) -> Optional[str]:
    return _default_resolver.manufacturer(mac)


def known_ouis() -> List[str]:
    return list(_default_resolver._db().keys())
