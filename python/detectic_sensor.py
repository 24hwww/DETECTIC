#!/usr/bin/env python3
"""Detectic external sensor — production polling engine for TP-Link EX520.

Turns the proven read-only GTPR/GDPR observation path
(`DEV2_WIFI_APDEV_ASSOCDEV`) into a reliable external Detectic sensor that
runs on a host machine, polls the router, normalizes observations into
structured events, tracks presence with an explicit absence timeout,
buffers events durably, and uploads them to the Detectic backend via
HTTPS with idempotency.

Architecture:

    EX520 (unmodified, read-only)
       |
       | GTPR/GDPR IPv6 link-local
       | DEV2_WIFI_APDEV_ASSOCDEV
       v
    PollingEngine (auth, poll, retry, backoff)
       |
       v
    Normalizer (raw ASSOCDEV -> DeviceSnapshot)
       |
       v
    PresenceEngine (snapshot diff -> events, absence timeout)
       |
       v
    EventStore (durable SQLite queue, bounded)
       |
       v
    Uploader (HTTPS POST, idempotency, retry, ack-after-delivery)
       |
       v
    Detectic Backend

The poller does NOT depend on backend availability.  Events are buffered
locally and delivered when the backend returns.

Safety: only read-only `gl` operations are sent to the EX520.  No `so`,
no `ACT_SAVE_CFG`, no reboots, no config changes, no uploads to router.

Usage:
    export DETECTIC_URL='http://[fe80::3e6a:d2ff:fe5f:abc1%enp2s0]'
    export DETECTIC_USER=admin
    export DETECTIC_PASSWORD='<password>'
    export DETECTIC_SENSOR_ID=home-001
    export DETECTIC_SECRET='<hex secret for pseudonymization>'
    export DETECTIC_BACKEND_URL='http://localhost:8080'
    export DETECTIC_BACKEND_TOKEN='<sensor secret for HMAC auth>'
    python3 detectic_sensor.py run

    # One-shot capture (no upload, prints events):
    python3 detectic_sensor.py capture

    # Health check:
    python3 detectic_sensor.py health
"""

from __future__ import annotations

import argparse
import hashlib
import hmac
import json
import logging
import os
import signal
import sqlite3
import sys
import threading
import time
import uuid
from dataclasses import dataclass, field, asdict
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional, Tuple

# Reuse the proven GTPR client
from detectic_client import GtprClient, Dialect

logger = logging.getLogger("detectic.sensor")

# ---------------------------------------------------------------------------
# 14.9.1 — Raw GTPR Contract for DEV2_WIFI_APDEV_ASSOCDEV
# ---------------------------------------------------------------------------

# The following field map is derived from PROVEN-LIVE observations documented
# in ex520-network-map-gdpr.md, tests/temporal_dataset.jsonl, and the mock
# router fixture.  Each field is classified:
#
#   PROVEN-LIVE   — observed in real EX520 responses
#   PROVEN-STATIC — present in firmware rootfs / data model XML
#   UNAVAILABLE   — confirmed absent in this OID's response
#   UNKNOWN       — not yet observed, may appear in some firmware versions

ASSOCDEV_FIELD_CONTRACT: Dict[str, str] = {
    "X_TP_HostName":              "PROVEN-LIVE",
    "X_TP_IPAddress":             "PROVEN-LIVE",
    "MACAddress":                 "PROVEN-LIVE",
    "X_TP_RadioMac":              "PROVEN-LIVE",
    "operatingStandard":          "PROVEN-LIVE",
    "signalStrength":             "PROVEN-LIVE",   # 0-128 scale, NOT dBm
    "active":                     "PROVEN-LIVE",   # "1" or "0"
    "associationTime":            "PROVEN-LIVE",   # RFC3339 timestamp
    "lastDataDownlinkRate":       "PROVEN-LIVE",   # kbps
    "lastDataUplinkRate":         "PROVEN-LIVE",   # kbps
    "X_TP_SignalStrengthLevel":   "PROVEN-LIVE",   # 0-4
    "X_TP_MaxLinkRate":           "PROVEN-LIVE",   # kbps
    "noise":                      "PROVEN-LIVE",
    "steeringHistoryNumberOfEntries": "PROVEN-LIVE",
    "stack":                      "PROVEN-LIVE",   # OneMesh "1,1,2,N,0,0"
}

# Fields NOT present in ASSOCDEV (confirmed absent):
#   - No explicit "band" field — band is derived from X_TP_RadioMac
#   - No "channel" field
#   - No "BSSID" field (X_TP_RadioMac is the radio MAC, not BSSID)
#   - No "distance" or "position" field

# Response envelope:
#   {"data": [device_dict, ...], "operation": "gl", "oid": "DEV2_WIFI_APDEV_ASSOCDEV", "success": true}
# Error envelope:
#   {"success": false, "errorcode": <int>}
#   errorcode 9003 = permission denied / object not accessible for role
#   errorcode 9804 = case not found / OID not supported
#   [error]71111 = unencrypted body (should never happen with this client)


# ---------------------------------------------------------------------------
# 14.9.3 — Event Model
# ---------------------------------------------------------------------------

SCHEMA_VERSION = "2.0"

# Event types
EVT_DEVICE_FIRST_SEEN = "device_first_seen"
EVT_DEVICE_SEEN       = "device_seen"
EVT_DEVICE_CHANGED    = "device_changed"
EVT_DEVICE_LAST_SEEN  = "device_last_seen"
EVT_SENSOR_ONLINE     = "sensor_online"
EVT_SENSOR_OFFLINE    = "sensor_offline"


@dataclass
class DeviceSnapshot:
    """Normalized observation of a single associated device."""
    device_id: str           # HMAC-SHA256 pseudonym (never raw MAC)
    observed_at: int         # epoch seconds
    associated: bool
    signal_strength: Optional[int] = None       # 0-128
    signal_level: Optional[int] = None          # 0-4
    noise: Optional[int] = None
    operating_standard: Optional[str] = None    # n, ax, ac, etc.
    radio_id: Optional[str] = None              # pseudonymized radio MAC
    tx_rate_kbps: Optional[int] = None
    rx_rate_kbps: Optional[int] = None
    max_link_rate_kbps: Optional[int] = None
    band: Optional[str] = None                  # derived: "2.4GHz" / "5GHz"
    # Raw hostname is kept internally for diffing but NEVER sent to backend.
    _hostname: Optional[str] = field(default=None, repr=False)
    # Raw MAC kept internally for pseudonymization only, NEVER sent.
    _raw_mac: Optional[str] = field(default=None, repr=False)


@dataclass
class DetecticEvent:
    """A single Detectic sensor event, ready for backend ingestion."""
    event_id: str            # UUID v4 — idempotency key
    sensor_id: str
    event_type: str
    event_timestamp: int     # epoch seconds
    device_id: Optional[str] = None   # present for device events
    snapshot: Optional[Dict[str, Any]] = None  # DeviceSnapshot as dict
    schema_version: str = SCHEMA_VERSION

    def to_json(self) -> str:
        return json.dumps(asdict(self), separators=(",", ":"))

    def idempotency_key(self) -> str:
        """Deterministic key for backend deduplication."""
        parts = f"{self.sensor_id}|{self.device_id or ''}|{self.event_timestamp}|{self.event_type}"
        return hashlib.sha256(parts.encode()).hexdigest()


# ---------------------------------------------------------------------------
# 14.9.4 — Presence Engine
# ---------------------------------------------------------------------------

@dataclass
class DeviceTracker:
    """Per-device presence state."""
    device_id: str
    first_seen: int = 0
    last_seen: int = 0
    consecutive_seen: int = 0
    consecutive_missing: int = 0
    present: bool = False
    observation_count: int = 0
    last_snapshot: Optional[DeviceSnapshot] = None


class PresenceEngine:
    """Tracks device presence across polling snapshots.

    Uses an explicit absence timeout: a device is only marked as
    'last seen' (left) after `absence_threshold` consecutive polls
    without observing it.  A single missing poll does NOT trigger
    a departure event.
    """

    def __init__(self, absence_threshold: int = 3):
        """
        Args:
            absence_threshold: number of consecutive missing polls
                               before declaring a device absent.
        """
        self.absence_threshold = absence_threshold
        self._trackers: Dict[str, DeviceTracker] = {}
        self._first_seen_emitted: set = set()

    def update(
        self,
        snapshots: List[DeviceSnapshot],
        timestamp: int,
        sensor_online: bool,
    ) -> List[DetecticEvent]:
        """Process a new snapshot list and emit events.

        Args:
            snapshots: devices observed in this poll (may be empty).
            timestamp: epoch seconds of this poll.
            sensor_online: whether the router was reachable this poll.

        Returns:
            List of events to emit.
        """
        events: List[DetecticEvent] = []
        observed_ids = {s.device_id for s in snapshots}
        snap_map = {s.device_id: s for s in snapshots}

        # Update observed devices
        for snap in snapshots:
            did = snap.device_id
            t = self._trackers.get(did)
            if t is None:
                # First time seeing this device
                t = DeviceTracker(
                    device_id=did,
                    first_seen=timestamp,
                    last_seen=timestamp,
                    consecutive_seen=1,
                    consecutive_missing=0,
                    present=True,
                    observation_count=1,
                    last_snapshot=snap,
                )
                self._trackers[did] = t
                events.append(self._make_event(
                    EVT_DEVICE_FIRST_SEEN, timestamp, snap
                ))
                self._first_seen_emitted.add(did)
            else:
                t.consecutive_seen += 1
                t.consecutive_missing = 0
                t.last_seen = timestamp
                t.observation_count += 1

                # If device was absent and is now back, emit first_seen
                if not t.present:
                    t.present = True
                    t.first_seen = timestamp
                    events.append(self._make_event(
                        EVT_DEVICE_FIRST_SEEN, timestamp, snap
                    ))
                else:
                    # Check for changes
                    changed = self._diff_snapshots(t.last_snapshot, snap)
                    if changed:
                        events.append(self._make_event(
                            EVT_DEVICE_CHANGED, timestamp, snap
                        ))
                    else:
                        events.append(self._make_event(
                            EVT_DEVICE_SEEN, timestamp, snap
                        ))
                t.last_snapshot = snap

        # Update missing devices
        for did, t in list(self._trackers.items()):
            if did not in observed_ids:
                t.consecutive_missing += 1
                t.consecutive_seen = 0
                if t.consecutive_missing >= self.absence_threshold:
                    if t.present:
                        t.present = False
                        events.append(self._make_event(
                            EVT_DEVICE_LAST_SEEN, timestamp,
                            t.last_snapshot
                        ))

        return events

    def _diff_snapshots(
        self,
        old: Optional[DeviceSnapshot],
        new: DeviceSnapshot,
    ) -> bool:
        """Return True if any observable field changed."""
        if old is None:
            return False
        fields = [
            "signal_strength", "signal_level", "noise",
            "operating_standard", "radio_id", "tx_rate_kbps",
            "rx_rate_kbps", "max_link_rate_kbps", "band",
            "associated",
        ]
        for f in fields:
            if getattr(old, f, None) != getattr(new, f, None):
                return True
        return False

    def _make_event(
        self,
        event_type: str,
        timestamp: int,
        snap: Optional[DeviceSnapshot],
    ) -> DetecticEvent:
        snap_dict = None
        device_id = None
        if snap:
            device_id = snap.device_id
            # Strip internal fields before sending
            snap_dict = {
                k: v for k, v in asdict(snap).items()
                if not k.startswith("_")
            }
        return DetecticEvent(
            event_id=str(uuid.uuid4()),
            sensor_id="",  # filled by caller
            event_type=event_type,
            event_timestamp=timestamp,
            device_id=device_id,
            snapshot=snap_dict,
        )

    def present_device_ids(self) -> List[str]:
        return [d for d, t in self._trackers.items() if t.present]

    def all_trackers(self) -> Dict[str, DeviceTracker]:
        return dict(self._trackers)


# ---------------------------------------------------------------------------
# 14.9.2 — Normalizer (raw ASSOCDEV -> DeviceSnapshot)
# ---------------------------------------------------------------------------

# Known radio MACs from the live EX520 (from AGENTS.md):
#   3c:6a:d2:5f:ab:c1 — primary radio (2.4GHz)
#   3c:6a:d2:5f:ab:c3 — secondary radio (5GHz)
# Band is derived from X_TP_RadioMac.  We use a heuristic: if the
# radio MAC ends in :c1 (or matches the known 2.4GHz MAC), it's 2.4GHz;
# if it ends in :c3 (or matches the known 5GHz MAC), it's 5GHz.
# Otherwise, band is UNKNOWN.

KNOWN_RADIO_BANDS: Dict[str, str] = {
    "3c:6a:d2:5f:ab:c1": "2.4GHz",
    "3c:6a:d2:5f:ab:c3": "5GHz",
}


def _to_int(val: Any) -> Optional[int]:
    if val is None or val == "":
        return None
    try:
        return int(val)
    except (ValueError, TypeError):
        return None


def _to_str(val: Any) -> Optional[str]:
    if val is None or val == "":
        return None
    return str(val)


def derive_band(radio_mac: Optional[str]) -> Optional[str]:
    if not radio_mac:
        return None
    mac_lower = radio_mac.lower()
    if mac_lower in KNOWN_RADIO_BANDS:
        return KNOWN_RADIO_BANDS[mac_lower]
    # Heuristic: last octet odd → 2.4GHz, even → 5GHz (EX520 convention)
    # This is a fallback; the known map should be authoritative.
    try:
        last_octet = int(mac_lower.split(":")[-1], 16)
        return "2.4GHz" if last_octet % 2 == 1 else "5GHz"
    except (ValueError, IndexError):
        return None


def pseudonymize(secret: bytes, identifier: str) -> str:
    """HMAC-SHA256 pseudonymization. Deterministic, non-reversible."""
    return hmac.new(secret, identifier.encode(), hashlib.sha256).hexdigest()


def normalize_assocdev(
    raw_devices: List[Dict[str, Any]],
    secret: bytes,
    sensor_id: str,
    captured_at: int,
) -> List[DeviceSnapshot]:
    """Convert raw ASSOCDEV device dicts to normalized DeviceSnapshots.

    Pseudonymizes device MAC and radio MAC.  Raw MAC/hostname/IP are
    kept internally on the snapshot for diffing but are never included
    in events sent to the backend.
    """
    snapshots: List[DeviceSnapshot] = []
    for d in raw_devices:
        raw_mac = _to_str(d.get("MACAddress"))
        radio_mac = _to_str(d.get("X_TP_RadioMac"))
        hostname = _to_str(d.get("X_TP_HostName"))

        # Pseudonymize device identity (MAC preferred, then IP, then hostname)
        identity = raw_mac or _to_str(d.get("X_TP_IPAddress")) or hostname or ""
        device_id = pseudonymize(secret, identity or json.dumps(d, sort_keys=True))
        radio_id = pseudonymize(secret, radio_mac) if radio_mac else None

        snap = DeviceSnapshot(
            device_id=device_id,
            observed_at=captured_at,
            associated=d.get("active", "1") == "1",
            signal_strength=_to_int(d.get("signalStrength")),
            signal_level=_to_int(d.get("X_TP_SignalStrengthLevel")),
            noise=_to_int(d.get("noise")),
            operating_standard=_to_str(d.get("operatingStandard")),
            radio_id=radio_id,
            tx_rate_kbps=_to_int(d.get("lastDataDownlinkRate")),
            rx_rate_kbps=_to_int(d.get("lastDataUplinkRate")),
            max_link_rate_kbps=_to_int(d.get("X_TP_MaxLinkRate")),
            band=derive_band(radio_mac),
            _hostname=hostname,
            _raw_mac=raw_mac,
        )
        snapshots.append(snap)
    return snapshots


def parse_assocdev_response(raw_json: str) -> Tuple[bool, List[Dict], Optional[int]]:
    """Parse the decrypted GTPR response.

    Returns:
        (success, device_list, errorcode)
    """
    try:
        parsed = json.loads(raw_json)
    except json.JSONDecodeError as e:
        return False, [], None  # malformed
    if parsed.get("success") is False:
        return False, [], parsed.get("errorcode")
    data = parsed.get("data")
    if isinstance(data, list):
        return True, data, None
    if isinstance(data, dict):
        # Some firmware wraps the list in a key
        for key in ("ASSOCDEV", "devices", "list"):
            if key in data and isinstance(data[key], list):
                return True, data[key], None
        return True, [], None  # object but no device list
    return True, [], None


# ---------------------------------------------------------------------------
# 14.9.6 — Durable Local Event Store (SQLite queue)
# ---------------------------------------------------------------------------

class EventStore:
    """Durable, bounded SQLite queue for events awaiting upload.

    Events are inserted on emission and deleted only after the backend
    acknowledges successful delivery.  Survives process restart and
    temporary Internet loss.
    """

    DDL = """
    CREATE TABLE IF NOT EXISTS event_queue (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        event_id    TEXT NOT NULL UNIQUE,
        event_json  TEXT NOT NULL,
        created_at  INTEGER NOT NULL,
        attempts    INTEGER NOT NULL DEFAULT 0,
        last_error  TEXT,
        uploaded    INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX IF NOT EXISTS idx_queue_pending
        ON event_queue(uploaded, id);
    CREATE TABLE IF NOT EXISTS sensor_health (
        key         TEXT PRIMARY KEY,
        value       TEXT NOT NULL,
        updated_at  INTEGER NOT NULL
    );
    """

    def __init__(self, db_path: str, max_events: int = 65536):
        self.db_path = db_path
        self.max_events = max_events
        self._lock = threading.Lock()
        self._conn = sqlite3.connect(db_path, check_same_thread=False)
        self._conn.execute("PRAGMA journal_mode=WAL")
        self._conn.executescript(self.DDL)
        self._conn.commit()

    def enqueue(self, event: DetecticEvent) -> bool:
        """Add an event to the queue. Returns False if queue is full."""
        with self._lock:
            count = self._conn.execute(
                "SELECT COUNT(*) FROM event_queue WHERE uploaded=0"
            ).fetchone()[0]
            if count >= self.max_events:
                logger.warning("event queue full (%d), dropping oldest", count)
                self._conn.execute(
                    "DELETE FROM event_queue WHERE id IN "
                    "(SELECT id FROM event_queue WHERE uploaded=0 "
                    "ORDER BY id LIMIT 100)"
                )
            try:
                self._conn.execute(
                    "INSERT OR IGNORE INTO event_queue "
                    "(event_id, event_json, created_at) VALUES (?,?,?)",
                    (event.event_id, event.to_json(), int(time.time())),
                )
                self._conn.commit()
                return True
            except sqlite3.IntegrityError:
                # Duplicate event_id — idempotent enqueue
                return True

    def pending(self, limit: int = 100) -> List[Tuple[int, str]]:
        """Get pending events (id, json) ordered by insertion."""
        with self._lock:
            rows = self._conn.execute(
                "SELECT id, event_json FROM event_queue "
                "WHERE uploaded=0 ORDER BY id LIMIT ?",
                (limit,),
            ).fetchall()
            return rows

    def mark_uploaded(self, event_ids: List[int]):
        """Mark events as successfully delivered."""
        if not event_ids:
            return
        with self._lock:
            placeholders = ",".join("?" * len(event_ids))
            self._conn.execute(
                f"DELETE FROM event_queue WHERE id IN ({placeholders})",
                event_ids,
            )
            self._conn.commit()

    def mark_failed(self, event_ids: List[int], error: str):
        """Increment attempt count and record error."""
        if not event_ids:
            return
        with self._lock:
            placeholders = ",".join("?" * len(event_ids))
            self._conn.execute(
                f"UPDATE event_queue SET attempts=attempts+1, "
                f"last_error=? WHERE id IN ({placeholders})",
                [error] + event_ids,
            )
            self._conn.commit()

    def depth(self) -> int:
        with self._lock:
            return self._conn.execute(
                "SELECT COUNT(*) FROM event_queue WHERE uploaded=0"
            ).fetchone()[0]

    def set_health(self, key: str, value: Any):
        with self._lock:
            self._conn.execute(
                "INSERT OR REPLACE INTO sensor_health (key, value, updated_at) "
                "VALUES (?,?,?)",
                (key, json.dumps(value), int(time.time())),
            )
            self._conn.commit()

    def get_health(self) -> Dict[str, Any]:
        with self._lock:
            rows = self._conn.execute(
                "SELECT key, value FROM sensor_health"
            ).fetchall()
            return {k: json.loads(v) for k, v in rows}

    def close(self):
        with self._lock:
            self._conn.close()


# ---------------------------------------------------------------------------
# 14.9.2 — Polling Engine
# ---------------------------------------------------------------------------

class PollingEngine:
    """Polls the EX520 via GTPR, normalizes, runs presence, enqueues events.

    Handles authentication, retry, exponential backoff, session
    re-authentication, malformed responses, and router-unreachable.
    """

    def __init__(
        self,
        client: GtprClient,
        secret: bytes,
        sensor_id: str,
        store: EventStore,
        interval: int = 30,
        timeout: int = 15,
        max_retries: int = 3,
        backoff_base: float = 2.0,
        backoff_max: float = 300.0,
        absence_threshold: int = 3,
    ):
        self.client = client
        self.secret = secret
        self.sensor_id = sensor_id
        self.store = store
        self.interval = interval
        self.timeout = timeout
        self.max_retries = max_retries
        self.backoff_base = backoff_base
        self.backoff_max = backoff_max
        self.presence = PresenceEngine(absence_threshold=absence_threshold)

        # Health state (14.9.8)
        self._health = {
            "sensor_status": "stopped",
            "last_successful_poll": 0,
            "last_successful_upload": 0,
            "poll_latency_ms": 0,
            "router_errors": 0,
            "auth_errors": 0,
            "backend_errors": 0,
            "queue_depth": 0,
            "events_generated": 0,
            "events_uploaded": 0,
            "poll_count": 0,
        }
        self._running = False
        self._stop_event = threading.Event()

    def _update_health(self, **kwargs):
        self._health.update(kwargs)
        for k, v in kwargs.items():
            self.store.set_health(k, v)
        self.store.set_health("sensor_status", self._health["sensor_status"])
        self.store.set_health("updated_at", int(time.time()))

    def _backoff(self, attempt: int) -> float:
        delay = min(self.backoff_base ** attempt, self.backoff_max)
        return delay

    def _authenticate(self) -> bool:
        """Authenticate (or re-authenticate) to the router."""
        try:
            self.client.connect()
            self._health["auth_errors"] = self._health.get("auth_errors", 0)
            return True
        except Exception as e:
            self._health["auth_errors"] = self._health.get("auth_errors", 0) + 1
            logger.error("authentication failed: %s", e)
            self._update_health(
                auth_errors=self._health["auth_errors"],
                sensor_status="auth_failure",
            )
            return False

    def _poll_once(self) -> Tuple[bool, List[DeviceSnapshot], int]:
        """Execute a single poll. Returns (success, snapshots, latency_ms)."""
        start = time.monotonic()
        try:
            raw = self.client.gl("DEV2_WIFI_APDEV_ASSOCDEV")
            latency = int((time.monotonic() - start) * 1000)
            success, devices, errcode = parse_assocdev_response(raw)
            if not success:
                logger.warning("GTPR error: errorcode=%s", errcode)
                self._health["router_errors"] = self._health.get("router_errors", 0) + 1
                return False, [], latency
            captured_at = int(time.time())
            snapshots = normalize_assocdev(
                devices, self.secret, self.sensor_id, captured_at
            )
            return True, snapshots, latency
        except Exception as e:
            latency = int((time.monotonic() - start) * 1000)
            logger.error("poll failed: %s", e)
            self._health["router_errors"] = self._health.get("router_errors", 0) + 1
            return False, [], latency

    def _process_poll(
        self,
        success: bool,
        snapshots: List[DeviceSnapshot],
        timestamp: int,
    ) -> List[DetecticEvent]:
        """Run presence engine and generate events."""
        events: List[DetecticEvent] = []

        if success:
            events.append(DetecticEvent(
                event_id=str(uuid.uuid4()),
                sensor_id=self.sensor_id,
                event_type=EVT_SENSOR_ONLINE,
                event_timestamp=timestamp,
            ))
            presence_events = self.presence.update(
                snapshots, timestamp, sensor_online=True
            )
            for evt in presence_events:
                evt.sensor_id = self.sensor_id
                events.append(evt)
        else:
            events.append(DetecticEvent(
                event_id=str(uuid.uuid4()),
                sensor_id=self.sensor_id,
                event_type=EVT_SENSOR_OFFLINE,
                event_timestamp=timestamp,
            ))
            # On router offline, mark all present as missing (increment counter)
            presence_events = self.presence.update(
                [], timestamp, sensor_online=False
            )
            for evt in presence_events:
                evt.sensor_id = self.sensor_id
                events.append(evt)

        return events

    def poll_cycle(self) -> Tuple[bool, List[DetecticEvent]]:
        """Run one complete poll cycle with retry/backoff."""
        timestamp = int(time.time())
        self._health["poll_count"] = self._health.get("poll_count", 0) + 1

        for attempt in range(self.max_retries):
            success, snapshots, latency = self._poll_once()
            self._update_health(poll_latency_ms=latency)

            if success:
                self._health["last_successful_poll"] = timestamp
                self._health["router_errors"] = 0
                events = self._process_poll(success, snapshots, timestamp)
                self._health["events_generated"] = (
                    self._health.get("events_generated", 0) + len(events)
                )
                self._update_health(
                    last_successful_poll=timestamp,
                    router_errors=0,
                    events_generated=self._health["events_generated"],
                    sensor_status="online",
                    queue_depth=self.store.depth(),
                )
                return True, events

            # On failure, try re-authentication on last attempt
            if attempt < self.max_retries - 1:
                delay = self._backoff(attempt + 1)
                logger.info(
                    "poll attempt %d failed, retrying in %.1fs",
                    attempt + 1, delay,
                )
                self._stop_event.wait(delay)
                if self._stop_event.is_set():
                    return False, []
                # Re-authenticate if we suspect session expiry
                if attempt == 0:
                    self._authenticate()
            else:
                events = self._process_poll(False, [], timestamp)
                self._health["events_generated"] = (
                    self._health.get("events_generated", 0) + len(events)
                )
                self._update_health(
                    sensor_status="router_unreachable",
                    events_generated=self._health["events_generated"],
                    queue_depth=self.store.depth(),
                )
                return False, events

        return False, []

    def run(self):
        """Main polling loop. Runs until stopped via signal or stop()."""
        self._running = True
        self._stop_event.clear()
        self._update_health(sensor_status="starting")

        # Initial authentication
        if not self._authenticate():
            self._update_health(sensor_status="auth_failure")
            logger.error("initial authentication failed, will retry")
        else:
            logger.info("authenticated to %s", self.client.base)

        logger.info(
            "polling started: interval=%ds, absence_threshold=%d",
            self.interval, self.presence.absence_threshold,
        )

        while not self._stop_event.is_set():
            try:
                success, events = self.poll_cycle()
                for evt in events:
                    self.store.enqueue(evt)
                    logger.debug(
                        "enqueued: %s device=%s",
                        evt.event_type, evt.device_id or "-",
                    )
            except Exception as e:
                logger.error("poll cycle exception: %s", e, exc_info=True)
                self._update_health(sensor_status="error")

            # Wait for next interval (interruptible)
            self._stop_event.wait(self.interval)

        self._running = False
        self._update_health(sensor_status="stopped")
        logger.info("polling stopped")

    def stop(self):
        self._stop_event.set()

    @property
    def health(self) -> Dict[str, Any]:
        h = dict(self._health)
        h["queue_depth"] = self.store.depth()
        h["present_devices"] = len(self.presence.present_device_ids())
        return h


# ---------------------------------------------------------------------------
# 14.9.7 — Uploader (backend HTTPS ingestion)
# ---------------------------------------------------------------------------

class Uploader:
    """Uploads events from the EventStore to the Detectic backend.

    Uses HMAC-SHA256 authentication and idempotency keys so retries
    do not create duplicates.  Events are deleted from the store only
    after the backend acknowledges successful delivery.
    """

    def __init__(
        self,
        store: EventStore,
        backend_url: str,
        sensor_id: str,
        sensor_secret: str,
        batch_size: int = 50,
        timeout: int = 15,
        max_retries: int = 5,
        backoff_base: float = 2.0,
        backoff_max: float = 600.0,
    ):
        self.store = store
        self.backend_url = backend_url.rstrip("/")
        self.sensor_id = sensor_id
        self.sensor_secret = sensor_secret
        self.batch_size = batch_size
        self.timeout = timeout
        self.max_retries = max_retries
        self.backoff_base = backoff_base
        self.backoff_max = backoff_max
        self._stop_event = threading.Event()

    def _sign(self, body: bytes) -> str:
        return hmac.new(
            self.sensor_secret.encode(), body, hashlib.sha256
        ).hexdigest()

    def upload_batch(self) -> Tuple[int, int]:
        """Upload one batch of pending events.

        Returns:
            (uploaded_count, failed_count)
        """
        import requests as req_lib

        pending = self.store.pending(self.batch_size)
        if not pending:
            return 0, 0

        events = []
        row_ids = []
        for row_id, event_json in pending:
            events.append(json.loads(event_json))
            row_ids.append(row_id)

        payload = json.dumps({
            "sensor_id": self.sensor_id,
            "events": events,
        }).encode()

        signature = self._sign(payload)

        for attempt in range(self.max_retries):
            try:
                resp = req_lib.post(
                    f"{self.backend_url}/api/v1/events",
                    data=payload,
                    headers={
                        "Content-Type": "application/json",
                        "X-Detectic-Sensor": self.sensor_id,
                        "X-Detectic-Signature": signature,
                    },
                    timeout=self.timeout,
                )
                if resp.status_code in (200, 201, 202):
                    self.store.mark_uploaded(row_ids)
                    self.store.set_health(
                        "last_successful_upload", int(time.time())
                    )
                    logger.info("uploaded %d events", len(row_ids))
                    return len(row_ids), 0
                elif resp.status_code == 409:
                    # Conflict — backend already has these (idempotent)
                    self.store.mark_uploaded(row_ids)
                    logger.info("uploaded %d events (idempotent ack)", len(row_ids))
                    return len(row_ids), 0
                else:
                    error = f"HTTP {resp.status_code}: {resp.text[:200]}"
                    logger.warning("upload failed: %s", error)
                    self.store.mark_failed(row_ids, error)
                    delay = min(self.backoff_base ** attempt, self.backoff_max)
                    self._stop_event.wait(delay)
            except Exception as e:
                error = f"{type(e).__name__}: {e}"
                logger.warning("upload error: %s", error)
                self.store.mark_failed(row_ids, error)
                delay = min(self.backoff_base ** attempt, self.backoff_max)
                self._stop_event.wait(delay)

        logger.error("upload batch exhausted retries: %d events", len(row_ids))
        return 0, len(row_ids)

    def run(self, poll_interval: int = 5):
        """Continuous upload loop."""
        while not self._stop_event.is_set():
            try:
                uploaded, failed = self.upload_batch()
                if uploaded == 0 and failed == 0:
                    self._stop_event.wait(poll_interval)
            except Exception as e:
                logger.error("uploader loop error: %s", e)
                self._stop_event.wait(poll_interval)

    def stop(self):
        self._stop_event.set()


# ---------------------------------------------------------------------------
# 14.9.8 — Sensor Health
# ---------------------------------------------------------------------------

def print_health(engine: PollingEngine, store: EventStore):
    h = engine.health
    stored = store.get_health()
    print("=== Detectic Sensor Health ===")
    print(f"  status:              {h['sensor_status']}")
    print(f"  last successful poll: {h['last_successful_poll']}")
    print(f"  poll latency:        {h['poll_latency_ms']}ms")
    print(f"  poll count:          {h['poll_count']}")
    print(f"  router errors:       {h['router_errors']}")
    print(f"  auth errors:         {h['auth_errors']}")
    print(f"  events generated:    {h['events_generated']}")
    print(f"  events uploaded:     {h['events_uploaded']}")
    print(f"  queue depth:         {h['queue_depth']}")
    print(f"  present devices:     {h['present_devices']}")
    print(f"  stored health keys:  {list(stored.keys())}")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def env_or(key: str, default: str = "") -> str:
    return os.environ.get(key, default)


def env_int(key: str, default: int) -> int:
    try:
        return int(os.environ.get(key, default))
    except (ValueError, TypeError):
        return default


def main():
    ap = argparse.ArgumentParser(
        description="Detectic external sensor for TP-Link EX520"
    )
    ap.add_argument(
        "--url", default=env_or("DETECTIC_URL"),
        help="Router URL (default: DETECTIC_URL env)",
    )
    ap.add_argument(
        "--user", default=env_or("DETECTIC_USER", "admin"),
        help="Router username (default: admin)",
    )
    ap.add_argument(
        "--password", default=env_or("DETECTIC_PASSWORD"),
        help="Router password (default: DETECTIC_PASSWORD env)",
    )
    ap.add_argument(
        "--dialect",
        choices=[Dialect.GDPR_JSON, Dialect.GDPR_TEXT],
        default=env_or("DETECTIC_DIALECT", Dialect.GDPR_JSON),
    )
    ap.add_argument(
        "--sensor-id", default=env_or("DETECTIC_SENSOR_ID", "home-001"),
        help="Sensor identifier (default: home-001)",
    )
    ap.add_argument(
        "--secret", default=env_or("DETECTIC_SECRET"),
        help="Hex secret for pseudonymization (DETECTIC_SECRET env)",
    )
    ap.add_argument(
        "--db", default=env_or("DETECTIC_DB", "detectic_sensor.db"),
        help="SQLite path for event queue (default: detectic_sensor.db)",
    )
    ap.add_argument(
        "--interval", type=int, default=env_int("DETECTIC_INTERVAL", 30),
        help="Polling interval seconds (default: 30)",
    )
    ap.add_argument(
        "--absence-threshold", type=int,
        default=env_int("DETECTIC_ABSENCE_THRESHOLD", 3),
        help="Consecutive missing polls before declaring absent (default: 3)",
    )
    ap.add_argument(
        "--backend-url", default=env_or("DETECTIC_BACKEND_URL"),
        help="Backend URL for event upload (default: DETECTIC_BACKEND_URL env)",
    )
    ap.add_argument(
        "--backend-token", default=env_or("DETECTIC_BACKEND_TOKEN"),
        help="Sensor secret for HMAC auth (DETECTIC_BACKEND_TOKEN env)",
    )
    ap.add_argument(
        "--buffer-max", type=int,
        default=env_int("DETECTIC_BUFFER_MAX", 65536),
        help="Max events in buffer (default: 65536)",
    )
    ap.add_argument(
        "--log-level", default=env_or("DETECTIC_LOG_LEVEL", "info"),
        choices=["debug", "info", "warning", "error"],
    )

    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("run", help="Run continuous polling + upload")
    sub.add_parser("capture", help="One-shot poll, print events, no upload")
    sub.add_parser("health", help="Print stored health and exit")
    sub.add_parser("drain", help="Upload all pending events and exit")
    sub.add_parser("contract", help="Print raw GTPR field contract")

    args = ap.parse_args()

    logging.basicConfig(
        level=getattr(logging, args.log_level.upper()),
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )

    if not args.password:
        ap.error("--password or DETECTIC_PASSWORD env is required")
    if not args.secret:
        ap.error("--secret or DETECTIC_SECRET env is required")

    secret = bytes.fromhex(args.secret) if args.secret else b"\x00" * 32

    if args.cmd == "contract":
        print("=== DEV2_WIFI_APDEV_ASSOCDEV Raw Field Contract ===")
        for field, classification in sorted(ASSOCDEV_FIELD_CONTRACT.items()):
            print(f"  {field:<35} {classification}")
        print()
        print("Response envelope:")
        print('  {"data": [device_dict, ...], "operation": "gl",')
        print('   "oid": "DEV2_WIFI_APDEV_ASSOCDEV", "success": true}')
        print()
        print("Error envelope:")
        print('  {"success": false, "errorcode": <int>}')
        print("  9003 = permission denied")
        print("  9804 = OID not supported")
        return

    store = EventStore(args.db, args.buffer_max)

    if args.cmd == "health":
        h = store.get_health()
        print("=== Stored Sensor Health ===")
        for k, v in sorted(h.items()):
            print(f"  {k}: {v}")
        print(f"  queue_depth: {store.depth()}")
        store.close()
        return

    if args.cmd == "drain":
        if not args.backend_url or not args.backend_token:
            ap.error("--backend-url and --backend-token required for drain")
        uploader = Uploader(
            store, args.backend_url, args.sensor_id, args.backend_token,
        )
        total = 0
        while True:
            uploaded, failed = uploader.upload_batch()
            total += uploaded
            if uploaded == 0:
                break
        print(f"drained {total} events")
        store.close()
        return

    # capture or run — need a client
    client = GtprClient(args.url, args.user, args.password, args.dialect)
    engine = PollingEngine(
        client, secret, args.sensor_id, store,
        interval=args.interval,
        absence_threshold=args.absence_threshold,
    )

    if args.cmd == "capture":
        # One-shot: authenticate, poll once, print events
        if not engine._authenticate():
            print("authentication failed", file=sys.stderr)
            store.close()
            sys.exit(1)
        success, events = engine.poll_cycle()
        print(f"poll success: {success}")
        print(f"events generated: {len(events)}")
        for evt in events:
            print(json.dumps(asdict(evt), indent=2))
        print_health(engine, store)
        store.close()
        return

    # run — continuous polling + upload
    uploader: Optional[Uploader] = None
    upload_thread: Optional[threading.Thread] = None

    if args.backend_url and args.backend_token:
        uploader = Uploader(
            store, args.backend_url, args.sensor_id, args.backend_token,
        )
        upload_thread = threading.Thread(
            target=uploader.run, daemon=True, name="uploader"
        )
        upload_thread.start()
        logger.info("uploader started → %s", args.backend_url)
    else:
        logger.warning("no backend configured — events will buffer locally only")

    # Signal handling for graceful shutdown
    def handle_signal(signum, frame):
        logger.info("signal %d received, shutting down", signum)
        engine.stop()
        if uploader:
            uploader.stop()

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)

    # Health reporting thread
    def health_reporter():
        while not engine._stop_event.is_set():
            engine._stop_event.wait(60)
            if engine._stop_event.is_set():
                break
            print_health(engine, store)

    health_thread = threading.Thread(
        target=health_reporter, daemon=True, name="health"
    )
    health_thread.start()

    # Main polling loop
    engine.run()

    if upload_thread:
        uploader.stop()
        upload_thread.join(timeout=10)

    print_health(engine, store)
    store.close()


if __name__ == "__main__":
    main()
