#!/usr/bin/env python3
"""Detectic Autonomous EX520 Collector — one-shot 5-minute job.

The autonomous reporting loop:

    cron (every 5 min, 5-minute aligned)
      |
      v
    run.sh (flock, env)
      |
      v
    collector.py run
      |
      +-- live EX520 GTPR poll (auth_latency, api_latency)
      |     |
      |     v
      |   normalize + pseudonymize (no raw MAC persisted)
      |     |
      |     v
      |   persist capture + device observations  (SQLite, deterministic capture_id)
      |     |
      |     v
      |   generate report
      |     |
      |     v
      |   SMTP delivery (state machine, idempotent, bounded retries)
      |
      v
    verify.py  (external evidence, independent of email)

Safety:
  * EX520 is read-only: only `gl` operations are performed.
  * No raw MAC addresses are persisted or emailed.
  * Credentials come from environment / .env, never from source.
  * Every capture has a deterministic identity (sensor_id + scheduled_at)
    so retries can never produce duplicate captures or duplicate reports.
  * One job at a time (enforced by run.sh flock).

Exit codes:
  0  run completed (captured and/or delivered, or duplicate slot skipped)
  2  already running (flock held) — normal, not an error
  3  capture failed (EX520 unreachable / auth failed)
  4  report generation failed
  5  delivery failed (capture persisted, will be retried)
"""

from __future__ import annotations

import argparse
import hashlib
import hmac
import io
import json
import os
import smtplib
import sqlite3
import sys
import time
import uuid
from contextlib import redirect_stdout
from dataclasses import dataclass, field
from datetime import datetime, timezone, timedelta
from email.mime.multipart import MIMEMultipart
from email.mime.text import MIMEText
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

from identity import (
    AssociationState,
    DeviceIdentityEngine,
    EntityType,
    Observation,
)
from identity.repository import JsonFileRepositories
from identity.stable_id import stable_fingerprint
from identity.classifier import infer_device_class
from identity.mac import classify_mac
from identity.oui import manufacturer as oui_manufacturer

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

DEFAULT_URL = "http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]"
SLOT_SECONDS = 300          # 5 minutes
TZ = timezone(timedelta(hours=-3), name="BRT")   # America/Sao_Paulo
RETRY_ATTEMPTS = 3          # SMTP attempts within one run
RETRY_BACKOFF = [0, 10, 30]  # seconds to wait before attempt 1..N
MAX_RETRY_CATCHUP = 3       # max pending deliveries retried per run
RETRY_AGE_LIMIT = 48 * 3600 # only retry deliveries younger than this
MAX_REPORT_DEVICES = 40     # cap device rows in report


def load_dotenv(path: str) -> None:
    """Load .env into os.environ (only if key not already set)."""
    if not os.path.exists(path):
        return
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, _, value = line.partition("=")
            key = key.strip()
            value = value.strip().strip('"').strip("'")
            if key and key not in os.environ:
                os.environ[key] = value


def env(key: str, default: str = "", alt: str = "") -> str:
    """Read env with alternate key fallback (AUTONOMOUS_* then DETECTIC_*)."""
    v = os.environ.get(key)
    if v is not None and v != "":
        return v
    if alt:
        v = os.environ.get(alt)
        if v is not None and v != "":
            return v
    return default


def env_int(key: str, default: int, alt: str = "") -> int:
    try:
        return int(env(key, str(default), alt))
    except (ValueError, TypeError):
        return default


@dataclass
class Config:
    db_path: str
    sensor_id: str
    url: str
    user: str
    password: str
    secret: bytes
    dialect: str
    smtp_host: str
    smtp_port: int
    smtp_user: str
    smtp_password: str
    smtp_from: str
    smtp_to: List[str]
    smtp_tls: str
    email_enabled: bool
    log_path: str
    identity_path: str = ""


def load_config() -> Config:
    here = Path(__file__).resolve().parent
    repo = here.parent
    load_dotenv(str(repo / ".env"))

    db = env("AUTONOMOUS_DB", "")
    sensor = env("AUTONOMOUS_SENSOR_ID", "ex520-001")
    url = env("AUTONOMOUS_URL", DEFAULT_URL)
    user = env("AUTONOMOUS_USER", "user", "DETECTIC_USER")
    password = env("AUTONOMOUS_PASSWORD", "", "DETECTIC_PASSWORD")
    secret_hex = env("AUTONOMOUS_SECRET", "", "DETECTIC_SECRET")
    dialect = env("AUTONOMOUS_DIALECT", "gdpr-json", "DETECTIC_DIALECT")
    # Normalize legacy values: the client contract uses "gdpr-json"/"gdpr-text".
    dialect = {"json": "gdpr-json", "text": "gdpr-text"}.get(dialect, dialect)

    # SMTP: AUTONOMOUS_SMTP_* override, fall back to DETECTIC_SMTP_*
    smtp_host = env("AUTONOMOUS_SMTP_HOST", "", "DETECTIC_SMTP_HOST")
    smtp_port = env_int("AUTONOMOUS_SMTP_PORT", 587, "DETECTIC_SMTP_PORT")
    smtp_user = env("AUTONOMOUS_SMTP_USER", "", "DETECTIC_SMTP_USER")
    smtp_password = env("AUTONOMOUS_SMTP_PASSWORD", "", "DETECTIC_SMTP_PASSWORD")
    smtp_from = env("AUTONOMOUS_SMTP_FROM", "", "DETECTIC_SMTP_FROM")
    smtp_to_raw = env("AUTONOMOUS_SMTP_TO", "", "DETECTIC_SMTP_TO")
    smtp_tls = env("AUTONOMOUS_SMTP_TLS", "starttls", "DETECTIC_SMTP_TLS")
    smtp_to = [t.strip() for t in smtp_to_raw.replace(";", ",").split(",") if t.strip()]

    email_enabled = env_int("AUTONOMOUS_EMAIL_ENABLED", 1 if smtp_host and smtp_to else 0)
    log_path = env("AUTONOMOUS_LOG", str(here / "logs" / "collector.log"))

    # Identity/fingerprint state file (cross-run temporal correlation).
    # Co-located with the SQLite DB when present, else under data/.
    if db and db != ":memory:":
        identity_path = str(Path(db).parent / "identity_state.json")
    else:
        identity_path = str(here / "data" / "identity_state.json")

    if not secret_hex:
        if os.environ.get("AUTONOMOUS_ALLOW_DEV_SECRET", "0") != "1":
            raise ValueError(
                "no sensor secret configured: set AUTONOMOUS_SECRET or "
                "DETECTIC_SECRET (development only: AUTONOMOUS_ALLOW_DEV_SECRET=1)"
            )
        print(
            "[collector] WARNING: using development secret "
            "(AUTONOMOUS_ALLOW_DEV_SECRET=1)",
            file=sys.stderr,
        )
        secret = b"detectic-autonomous-dev-secret"
    else:
        # Canonical HMAC contract: UTF-8 bytes of the secret string.
        # This matches the Rust sensor (secret.as_bytes()) and the
        # Cloudflare Worker (TextEncoder().encode(secret)).
        # NEVER hex-decode — that produces a different key than the other
        # components and causes HTTP 401.
        secret = secret_hex.encode("utf-8")

    return Config(
        db_path=db, sensor_id=sensor, url=url, user=user, password=password,
        secret=secret, dialect=dialect,
        smtp_host=smtp_host, smtp_port=smtp_port, smtp_user=smtp_user,
        smtp_password=smtp_password, smtp_from=smtp_from, smtp_to=smtp_to,
        smtp_tls=smtp_tls, email_enabled=bool(email_enabled), log_path=log_path,
        identity_path=identity_path,
    )


# ---------------------------------------------------------------------------
# Scheduling
# ---------------------------------------------------------------------------

def align_slot(ts: int, slot: int = SLOT_SECONDS) -> int:
    """Floor timestamp to the 5-minute boundary (HH:00/05/10/...)."""
    return (ts // slot) * slot


def capture_id_for(sensor_id: str, scheduled_at: int) -> str:
    """Deterministic capture identity: sensor_id + scheduled_at."""
    raw = f"{sensor_id}|{scheduled_at}"
    return hashlib.sha1(raw.encode()).hexdigest()[:12]


def run_id_for() -> str:
    return uuid.uuid4().hex[:12]


# ---------------------------------------------------------------------------
# Persistence
# ---------------------------------------------------------------------------

SCHEMA = """
CREATE TABLE IF NOT EXISTS captures (
    capture_id          TEXT PRIMARY KEY,
    run_id              TEXT NOT NULL,
    sensor_id           TEXT NOT NULL,
    scheduled_at        INTEGER NOT NULL,
    started_at          INTEGER NOT NULL,
    completed_at        INTEGER,
    status              TEXT NOT NULL,
    api_latency_ms      INTEGER,
    auth_latency_ms     INTEGER,
    device_count        INTEGER,
    active_device_count INTEGER,
    payload_hash        TEXT,
    created_at          INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_captures_scheduled ON captures(scheduled_at);
CREATE INDEX IF NOT EXISTS idx_captures_status   ON captures(status);

CREATE TABLE IF NOT EXISTS device_observations (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    capture_id        TEXT NOT NULL,
    pseudonym         TEXT NOT NULL,
    fingerprint_id    TEXT,
    fingerprint_method TEXT,
    hostname          TEXT,
    band              TEXT,
    signal_strength   INTEGER,
    signal_level      INTEGER,
    noise             INTEGER,
    operating_standard TEXT,
    tx_rate_kbps      INTEGER,
    rx_rate_kbps      INTEGER,
    status            TEXT,
    identity_json     TEXT,
    FOREIGN KEY(capture_id) REFERENCES captures(capture_id)
);
CREATE INDEX IF NOT EXISTS idx_devobs_capture ON device_observations(capture_id);
-- idx_devobs_fp is created in _migrate after fingerprint_id is guaranteed present.

CREATE TABLE IF NOT EXISTS deliveries (
    delivery_id   TEXT NOT NULL,
    capture_id    TEXT NOT NULL,
    report_id     TEXT NOT NULL,
    attempt_number INTEGER NOT NULL,
    attempted_at  INTEGER NOT NULL,
    error         TEXT,
    final_status  TEXT NOT NULL,
    PRIMARY KEY (delivery_id, attempt_number),
    FOREIGN KEY(capture_id) REFERENCES captures(capture_id)
);

CREATE TABLE IF NOT EXISTS runs (
    run_id        TEXT PRIMARY KEY,
    scheduled_at  INTEGER NOT NULL,
    started_at    INTEGER NOT NULL,
    completed_at  INTEGER,
    status        TEXT NOT NULL,
    duration_ms   INTEGER
);
"""

# Delivery / capture states
CAPTURED = "CAPTURED"
PERSISTED = "PERSISTED"
REPORT_GENERATED = "REPORT_GENERATED"
DELIVERY_PENDING = "DELIVERY_PENDING"
DELIVERED = "DELIVERED"
CAPTURE_FAILED = "CAPTURE_FAILED"
REPORT_FAILED = "REPORT_FAILED"
DELIVERY_FAILED = "DELIVERY_FAILED"


class Store:
    def __init__(self, db_path: str):
        self.path = db_path
        Path(db_path).parent.mkdir(parents=True, exist_ok=True)
        self.conn = sqlite3.connect(db_path)
        self.conn.executescript(SCHEMA)
        self.conn.commit()
        # Migrate existing DBs: add fingerprint columns if absent.
        self._migrate()

    def _migrate(self) -> None:
        cols = {r[1] for r in self.conn.execute("PRAGMA table_info(device_observations)")}
        for col, decl in [("fingerprint_id", "TEXT"), ("fingerprint_method", "TEXT")]:
            if col not in cols:
                try:
                    self.conn.execute(
                        f"ALTER TABLE device_observations ADD COLUMN {col} {decl}"
                    )
                except sqlite3.OperationalError as e:
                    # A pre-existing column is benign; anything else is a real
                    # migration failure and must not be swallowed.
                    if "duplicate column" not in str(e).lower():
                        raise
        try:
            self.conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_devobs_fp ON device_observations(fingerprint_id)"
            )
        except sqlite3.OperationalError as e:
            if "duplicate column" not in str(e).lower():
                raise
        self.conn.commit()

    # ---- runs ----
    def start_run(self, run_id: str, scheduled_at: int) -> None:
        self.conn.execute(
            "INSERT OR REPLACE INTO runs (run_id, scheduled_at, started_at, status) "
            "VALUES (?,?,?,?)",
            (run_id, scheduled_at, int(time.time()), "RUNNING"),
        )
        self.conn.commit()

    def finish_run(self, run_id: str, status: str) -> None:
        started = self.conn.execute(
            "SELECT started_at FROM runs WHERE run_id=?", (run_id,)
        ).fetchone()
        started = started[0] if started else int(time.time())
        dur = int((time.time() - started) * 1000)
        self.conn.execute(
            "UPDATE runs SET completed_at=?, status=?, duration_ms=? WHERE run_id=?",
            (int(time.time()), status, dur, run_id),
        )
        self.conn.commit()

    # ---- captures ----
    def get_capture(self, capture_id: str) -> Optional[Dict[str, Any]]:
        row = self.conn.execute(
            "SELECT * FROM captures WHERE capture_id=?", (capture_id,)
        ).fetchone()
        if not row:
            return None
        cols = [d[0] for d in self.conn.execute("SELECT * FROM captures").description]
        return dict(zip(cols, row))

    def insert_capture(
        self,
        capture_id: str, run_id: str, sensor_id: str,
        scheduled_at: int, started_at: int, status: str,
        api_latency_ms: Optional[int], auth_latency_ms: Optional[int],
        device_count: Optional[int], active_count: Optional[int],
        payload_hash: Optional[str],
    ) -> None:
        self.conn.execute(
            "INSERT OR IGNORE INTO captures "
            "(capture_id, run_id, sensor_id, scheduled_at, started_at, status, "
            " api_latency_ms, auth_latency_ms, device_count, active_device_count, "
            " payload_hash, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
            (capture_id, run_id, sensor_id, scheduled_at, started_at, status,
             api_latency_ms, auth_latency_ms, device_count, active_count,
             payload_hash, int(time.time())),
        )
        self.conn.commit()

    def update_capture(self, capture_id: str, **fields) -> None:
        if not fields:
            return
        keys = ",".join(f"{k}=?" for k in fields)
        self.conn.execute(
            f"UPDATE captures SET {keys} WHERE capture_id=?",
            list(fields.values()) + [capture_id],
        )
        self.conn.commit()

    def insert_devices(self, capture_id: str, devices: List[Dict[str, Any]]) -> None:
        for d in devices:
            id_json = json.dumps(d.get("identity")) if d.get("identity") is not None else None
            self.conn.execute(
                "INSERT OR IGNORE INTO device_observations "
                "(capture_id, pseudonym, fingerprint_id, fingerprint_method, hostname, "
                " band, signal_strength, signal_level, noise, operating_standard, "
                " tx_rate_kbps, rx_rate_kbps, status, identity_json) "
                "VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                (capture_id, d.get("pseudonym"), d.get("fingerprint_id"),
                 d.get("fingerprint_method"), d.get("hostname"), d.get("band"),
                 d.get("signal_strength"), d.get("signal_level"), d.get("noise"),
                 d.get("operating_standard"), d.get("tx_rate_kbps"),
                 d.get("rx_rate_kbps"), d.get("status"), id_json),
            )
        self.conn.commit()

    def devices_for(self, capture_id: str) -> List[Dict[str, Any]]:
        rows = self.conn.execute(
            "SELECT pseudonym, fingerprint_id, fingerprint_method, hostname, band, "
            "       signal_strength, signal_level, noise, operating_standard, "
            "       tx_rate_kbps, rx_rate_kbps, status, identity_json "
            "FROM device_observations WHERE capture_id=? ORDER BY id",
            (capture_id,),
        ).fetchall()
        cols = ["pseudonym", "fingerprint_id", "fingerprint_method", "hostname", "band",
                "signal_strength", "signal_level", "noise", "operating_standard",
                "tx_rate_kbps", "rx_rate_kbps", "status", "identity_json"]
        out = []
        for r in rows:
            d = dict(zip(cols, r))
            if d.get("identity_json"):
                try:
                    d["identity"] = json.loads(d["identity_json"])
                except (ValueError, TypeError):
                    d["identity"] = None
            out.append(d)
        return out

    # ---- deliveries ----
    def next_attempt_number(self, delivery_id: str) -> int:
        row = self.conn.execute(
            "SELECT COALESCE(MAX(attempt_number), 0) FROM deliveries "
            "WHERE delivery_id=?", (delivery_id,),
        ).fetchone()
        return (row[0] or 0) + 1

    def insert_delivery(
        self, delivery_id: str, capture_id: str, report_id: str,
        attempt: int, status: str, error: Optional[str] = None,
    ) -> None:
        self.conn.execute(
            "INSERT OR IGNORE INTO deliveries "
            "(delivery_id, capture_id, report_id, attempt_number, attempted_at, "
            " error, final_status) VALUES (?,?,?,?,?,?,?)",
            (delivery_id, capture_id, report_id, attempt, int(time.time()),
             error, status),
        )
        self.conn.commit()

    def delivery_for(self, capture_id: str) -> Optional[Dict[str, Any]]:
        row = self.conn.execute(
            "SELECT * FROM deliveries WHERE capture_id=? ORDER BY attempt_number DESC LIMIT 1",
            (capture_id,),
        ).fetchone()
        if not row:
            return None
        cols = [d[0] for d in self.conn.execute("SELECT * FROM deliveries").description]
        return dict(zip(cols, row))

    def pending_deliveries(self, limit: int = MAX_RETRY_CATCHUP) -> List[Dict[str, Any]]:
        """Captures that are captured/pending/failed but not delivered, oldest first."""
        rows = self.conn.execute(
            "SELECT c.capture_id, c.scheduled_at, c.status, c.sensor_id "
            "FROM captures c WHERE c.status != ? AND c.status != ? "
            "AND c.scheduled_at > ? "
            "ORDER BY c.scheduled_at ASC LIMIT ?",
            (DELIVERED, CAPTURE_FAILED, int(time.time()) - RETRY_AGE_LIMIT, limit),
        ).fetchall()
        cols = ["capture_id", "scheduled_at", "status", "sensor_id"]
        return [dict(zip(cols, r)) for r in rows]

    # ---- health / summary ----
    def health(self) -> Dict[str, Any]:
        h: Dict[str, Any] = {}
        r = self.conn.execute(
            "SELECT MAX(completed_at) FROM captures WHERE status=?", (DELIVERED,)
        ).fetchone()
        h["last_delivered_capture_at"] = r[0]
        r = self.conn.execute(
            "SELECT MAX(completed_at) FROM captures WHERE status IN (?,?,?)",
            (CAPTURED, PERSISTED, DELIVERED),
        ).fetchone()
        h["last_successful_capture_at"] = r[0]
        r = self.conn.execute(
            "SELECT MAX(completed_at) FROM captures WHERE status=?", (CAPTURE_FAILED,)
        ).fetchone()
        h["last_failed_capture_at"] = r[0]
        r = self.conn.execute("SELECT COUNT(*) FROM captures").fetchone()
        h["total_captures"] = r[0]
        r = self.conn.execute(
            "SELECT COUNT(*) FROM captures WHERE status IN (?,?,?,?,?)",
            (CAPTURED, PERSISTED, REPORT_GENERATED, DELIVERY_PENDING, DELIVERED),
        ).fetchone()
        h["successful_captures"] = r[0]
        r = self.conn.execute(
            "SELECT COUNT(*) FROM captures WHERE status=?", (DELIVERED,)
        ).fetchone()
        h["delivered_captures"] = r[0]
        r = self.conn.execute(
            "SELECT COUNT(*) FROM captures WHERE status=?", (CAPTURE_FAILED,)
        ).fetchone()
        h["failed_captures"] = r[0]
        r = self.conn.execute("SELECT COUNT(*) FROM deliveries").fetchone()
        h["total_deliveries"] = r[0]
        h["pending_deliveries"] = len(self.pending_deliveries(100))
        return h

    def recent(self, n: int = 20) -> List[Dict[str, Any]]:
        rows = self.conn.execute(
            "SELECT * FROM captures ORDER BY scheduled_at DESC LIMIT ?", (n,)
        ).fetchall()
        if not rows:
            return []
        cols = [d[0] for d in self.conn.execute("SELECT * FROM captures").description]
        out = []
        for r in rows:
            d = dict(zip(cols, r))
            dlv = self.delivery_for(d["capture_id"])
            d["delivery_status"] = dlv["final_status"] if dlv else "-"
            d["delivery_attempts"] = dlv["attempt_number"] if dlv else 0
            out.append(d)
        return out

    def close(self) -> None:
        self.conn.close()


class NullStore:
    """In-memory, no-disk store for space-constrained routers (EX520).

    Keeps just enough state for a single run: dedup within the run,
    retry accounting, and live report generation.  No SQLite file is
    created and no persistence survives process exit.
    """

    def __init__(self, db_path: str = ""):
        self._captures: Dict[str, Dict[str, Any]] = {}
        self._devices: Dict[str, List[Dict[str, Any]]] = {}
        self._deliveries: Dict[str, List[Dict[str, Any]]] = {}
        self._runs: Dict[str, Dict[str, Any]] = {}

    def _now(self) -> int:
        return int(time.time())

    def start_run(self, run_id: str, scheduled_at: int) -> None:
        self._runs[run_id] = {"started_at": self._now(), "status": "RUNNING"}

    def finish_run(self, run_id: str, status: str) -> None:
        started = self._runs.get(run_id, {}).get("started_at", self._now())
        self._runs[run_id] = {
            "completed_at": self._now(),
            "status": status,
            "duration_ms": int((time.time() - started) * 1000),
        }

    def get_capture(self, capture_id: str) -> Optional[Dict[str, Any]]:
        return self._captures.get(capture_id)

    def insert_capture(
        self,
        capture_id: str, run_id: str, sensor_id: str,
        scheduled_at: int, started_at: int, status: str,
        api_latency_ms: Optional[int], auth_latency_ms: Optional[int],
        device_count: Optional[int], active_count: Optional[int],
        payload_hash: Optional[str],
    ) -> None:
        self._captures[capture_id] = {
            "capture_id": capture_id,
            "run_id": run_id,
            "sensor_id": sensor_id,
            "scheduled_at": scheduled_at,
            "started_at": started_at,
            "completed_at": self._now(),
            "status": status,
            "api_latency_ms": api_latency_ms,
            "auth_latency_ms": auth_latency_ms,
            "device_count": device_count,
            "active_device_count": active_count,
            "payload_hash": payload_hash,
            "created_at": self._now(),
        }

    def update_capture(self, capture_id: str, **fields) -> None:
        cap = self._captures.get(capture_id)
        if cap:
            cap.update(fields)

    def insert_devices(self, capture_id: str, devices: List[Dict[str, Any]]) -> None:
        self._devices[capture_id] = list(devices)

    def devices_for(self, capture_id: str) -> List[Dict[str, Any]]:
        return list(self._devices.get(capture_id, []))

    def next_attempt_number(self, delivery_id: str) -> int:
        return len(self._deliveries.get(delivery_id, [])) + 1

    def insert_delivery(
        self, delivery_id: str, capture_id: str, report_id: str,
        attempt: int, status: str, error: Optional[str] = None,
    ) -> None:
        self._deliveries.setdefault(delivery_id, []).append({
            "delivery_id": delivery_id,
            "capture_id": capture_id,
            "report_id": report_id,
            "attempt_number": attempt,
            "attempted_at": self._now(),
            "error": error,
            "final_status": status,
        })

    def delivery_for(self, capture_id: str) -> Optional[Dict[str, Any]]:
        latest: Optional[Dict[str, Any]] = None
        for dlist in self._deliveries.values():
            for d in dlist:
                if d["capture_id"] == capture_id:
                    if latest is None or d["attempt_number"] > latest["attempt_number"]:
                        latest = d
        return latest

    def pending_deliveries(self, limit: int = MAX_RETRY_CATCHUP) -> List[Dict[str, Any]]:
        return []

    def health(self) -> Dict[str, Any]:
        return {
            "last_delivered_capture_at": None,
            "last_successful_capture_at": None,
            "last_failed_capture_at": None,
            "total_captures": len(self._captures),
            "successful_captures": 0,
            "delivered_captures": 0,
            "failed_captures": 0,
            "total_deliveries": sum(len(v) for v in self._deliveries.values()),
            "pending_deliveries": 0,
        }

    def recent(self, n: int = 20) -> List[Dict[str, Any]]:
        rows = sorted(
            self._captures.values(),
            key=lambda r: r.get("scheduled_at", 0),
            reverse=True,
        )[:n]
        out = []
        for r in rows:
            d = dict(r)
            dlv = self.delivery_for(d["capture_id"])
            d["delivery_status"] = dlv["final_status"] if dlv else "-"
            d["delivery_attempts"] = dlv["attempt_number"] if dlv else 0
            out.append(d)
        return out

    def close(self) -> None:
        pass


# ---------------------------------------------------------------------------
# Pseudonymization / normalization
# ---------------------------------------------------------------------------

def pseudonymize(secret: bytes, identifier: str) -> str:
    return hmac.new(secret, identifier.encode(), hashlib.sha256).hexdigest()[:16]


# Known radio MACs from the live EX520 (AGENTS.md §10.A):
#   3c:6a:d2:5f:ab:c1 — primary radio (2.4GHz)
#   3c:6a:d2:5f:ab:c3 — secondary radio (5GHz)
KNOWN_RADIO_BANDS = {
    "3c:6a:d2:5f:ab:c1": "2.4GHz",
    "3c:6a:d2:5f:ab:c3": "5GHz",
}


def _ssid_band_heuristic(ssid: str) -> str:
    """Fallback band guess from SSID name. Only use when radio data is unavailable."""
    s = ssid.lower().rstrip()
    if s.endswith("_5g") or s.endswith("-5g") or s.endswith(" 5g"):
        return "5GHz"
    if s.endswith("_2.4g") or s.endswith("-2.4g"):
        return "2.4GHz"
    return ""


def derive_band(standard: Optional[str], radio_mac: Optional[str],
                radio_band_map: Optional[Dict[str, str]] = None) -> str:
    if radio_mac:
        mac_lower = radio_mac.lower()
        if mac_lower in KNOWN_RADIO_BANDS:
            return KNOWN_RADIO_BANDS[mac_lower]
        if radio_band_map and mac_lower in radio_band_map:
            return radio_band_map[mac_lower]
    if standard == "ac":
        return "5GHz"
    if standard in ("n", "ax", "b", "g"):
        return "2.4GHz"
    return "unknown"


def to_int(v: Any) -> Optional[int]:
    if v is None or v == "":
        return None
    try:
        return int(v)
    except (ValueError, TypeError):
        return None


def _stable_fingerprint_for(secret: bytes, mac: str, hostname: Optional[str],
                           protocol: Optional[str], band: Optional[str]) -> Dict[str, Any]:
    """Compute the stable fingerprint_id (huella) for a raw device entry."""
    mac_type = classify_mac(mac)
    randomized = mac_type in ("LOCAL_RANDOMIZED", "LOCAL_ADMINISTERED", "INVALID")
    mfr = None
    if not randomized:
        mfr = oui_manufacturer(mac)
    device_class, _ = infer_device_class(hostname, mfr, protocol, band, mac)
    fp = stable_fingerprint(secret, hostname, mfr, device_class, mac, mac_type)
    return {
        "fingerprint_id": fp.fingerprint_id,
        "fingerprint_method": fp.method,
        "fingerprint_confidence": fp.confidence,
        "manufacturer": mfr,
        "device_class": device_class.value if hasattr(device_class, "value") else str(device_class),
    }


def normalize_devices(raw_devices: List[Dict], secret: bytes) -> List[Dict[str, Any]]:
    """Pseudonymize raw ASSOCDEV entries. Raw MAC is never persisted.

    Computes both the MAC pseudonym (per-band, may rotate) and the stable
    fingerprint_id (huella, stable across reconnects/bands).
    """
    out = []
    for d in raw_devices:
        if d.get("_meta"):
            continue  # skip metadata entries
        mac = str(d.get("MACAddress") or "").strip()
        radio_mac = str(d.get("X_TP_RadioMac") or "").strip()
        identity = mac or str(d.get("X_TP_IPAddress") or "") or \
                   str(d.get("X_TP_HostName") or "") or "unknown"
        pseudo = pseudonymize(secret, identity)
        standard = str(d.get("operatingStandard") or "").strip() or None
        hostname = str(d.get("X_TP_HostName") or "").strip() or None
        band = derive_band(standard, radio_mac)
        fp = _stable_fingerprint_for(secret, mac, hostname, standard, band)
        out.append({
            "pseudonym": pseudo,
            "fingerprint_id": fp["fingerprint_id"],
            "fingerprint_method": fp["fingerprint_method"],
            "fingerprint_confidence": fp["fingerprint_confidence"],
            "manufacturer": fp["manufacturer"],
            "device_class": fp["device_class"],
            "hostname": hostname,
            "band": band,
            "signal_strength": to_int(d.get("signalStrength")),
            "signal_level": to_int(d.get("X_TP_SignalStrengthLevel")),
            "noise": to_int(d.get("noise")),
            "operating_standard": standard,
            "tx_rate_kbps": to_int(d.get("lastDataDownlinkRate")),
            "rx_rate_kbps": to_int(d.get("lastDataUplinkRate")),
            "status": "active" if str(d.get("active") or "0") == "1" else "inactive",
            "source": d.get("source", "associated"),
            "bssid_pseudonym": pseudonymize(secret, str(d.get("X_TP_BssMac") or "").strip() or "no-bssid"),
        })
    return out


def enrich_identity(devices: List[Dict[str, Any]], raw_devices: List[Dict],
                     secret: bytes, sensor_id: str, identity_path: str) -> List[Dict[str, Any]]:
    """Attach a privacy-safe identity dict to each device (no raw MAC).

    Uses the DeviceIdentityEngine. Cross-run temporal correlation is persisted
    to `identity_path`. Failures never break the observation pipeline.
    """
    try:
        repos = JsonFileRepositories(identity_path)
        engine = DeviceIdentityEngine(sensor_id=sensor_id, repos=repos)
        now = int(time.time())
        # Map pseudonym -> raw entry (raw may contain the real MAC).
        pseudo_to_raw: Dict[str, Dict] = {}
        for raw in raw_devices:
            if raw.get("_meta"):
                continue
            key = str(raw.get("MACAddress") or "").strip() or \
                str(raw.get("X_TP_IPAddress") or "") or \
                str(raw.get("X_TP_HostName") or "") or "unknown"
            pseudo_to_raw.setdefault(pseudonymize(secret, key), raw)
        for d in devices:
            if d.get("_meta"):
                continue
            raw = pseudo_to_raw.get(d["pseudonym"])
            if not raw:
                continue
            obs = Observation(
                mac=str(raw.get("MACAddress") or "").strip(),
                hostname=str(raw.get("X_TP_HostName") or "").strip() or None,
                ssid=None,
                bssid=str(raw.get("X_TP_BssMac") or "").strip() or None,
                band=d.get("band"),
                channel=None,
                rssi=d.get("signal_strength"),
                protocol=d.get("operating_standard"),
                associated=(d.get("status") == "active"),
                timestamp=now,
            )
            ident = engine.identify(obs, persist=True)
            d["identity"] = ident.to_dict()
            # Attach AP (BSSID) fingerprint when available.
            if d.get("bssid_pseudonym") and d["bssid_pseudonym"] != pseudonymize(secret, "no-bssid"):
                network = engine.identify_network(
                    bssid=str(raw.get("X_TP_BssMac") or "").strip(),
                    ssid=None, sensor_id="", timestamp=now,
                )
                d["ap_identity"] = network.get("bssid_manufacturer")
    except Exception:
        # Identity is best-effort; never fail the capture because of it.
        pass
    return devices


def payload_hash(devices: List[Dict[str, Any]]) -> str:
    """Deterministic hash over the normalized device list.

    Excludes the evolving identity enrichment so that cross-run temporal drift
    (observation counts, history) never defeats duplicate-content detection.
    """
    stable = [
        {k: v for k, v in d.items() if k not in ("identity", "ap_identity")}
        for d in devices
    ]
    compact = json.dumps(stable, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(compact.encode()).hexdigest()[:16]


# ---------------------------------------------------------------------------
# Live EX520 capture
# ---------------------------------------------------------------------------

CAPTURE_RETRIES = 3
CAPTURE_RETRY_BACKOFF = [0, 3, 8]  # seconds before attempt 1..N


def live_capture(cfg: Config, started_at: int) -> Tuple[bool, List[Dict], int, int, str]:
    """Poll the EX520 once (with transient-failure retries).

    Returns (success, devices, auth_ms, api_ms, detail). Never raises for
    router/network failures — returns success=False instead so the job can
    record CAPTURE_FAILED durably. Retries only transient auth/transport
    failures; a genuinely unreachable router is reported as FAIL, never
    fabricated.
    """
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "python"))
    from detectic_client import GtprClient  # local import: heavy deps

    last_err = "no attempt"
    for attempt in range(1, CAPTURE_RETRIES + 1):
        if attempt > 1 and CAPTURE_RETRY_BACKOFF[attempt - 1]:
            time.sleep(CAPTURE_RETRY_BACKOFF[attempt - 1])
        ok, devs, auth_ms, api_ms, detail = _live_capture_once(cfg, GtprClient, attempt)
        if ok:
            return True, devs, auth_ms, api_ms, detail
        last_err = detail
    return False, [], 0, 0, f"after_{CAPTURE_RETRIES}_attempts:{last_err}"


REQUEST_TIMEOUT = 15  # seconds; GtprClient has no timeout, so enforce one


def _gl_json(client, oid: str) -> Optional[Dict]:
    """Safe gl() call, returns parsed JSON or None."""
    try:
        with redirect_stdout(io.StringIO()):
            raw = client.gl(oid)
        parsed = json.loads(raw)
        if parsed.get("success") is False:
            return None
        data = parsed.get("data", [])
        if isinstance(data, str):
            try:
                data = json.loads(data)
            except Exception:
                data = []
        return data
    except Exception:
        return None


def _extract_list(data, *keys) -> List[Dict]:
    """Extract list from nested GTPR response."""
    if isinstance(data, list):
        return data
    if isinstance(data, dict):
        for k in keys:
            if k in data and isinstance(data[k], list):
                return data[k]
    return []


def _live_capture_once(cfg: Config, gtpr_cls, attempt: int) -> Tuple[bool, List[Dict], int, int, str]:
    client = gtpr_cls(cfg.url, cfg.user, cfg.password, cfg.dialect)
    # Enforce a bounded request timeout on every HTTP call to the router.
    _orig_post = client.session.post

    def _post_with_timeout(*args, **kwargs):
        kwargs.setdefault("timeout", REQUEST_TIMEOUT)
        return _orig_post(*args, **kwargs)

    client.session.post = _post_with_timeout
    try:
        t0 = time.monotonic()
        with redirect_stdout(io.StringIO()):  # silence GtprClient DEBUG prints
            client.connect()
        auth_ms = int((time.monotonic() - t0) * 1000)
    except Exception as e:
        return False, [], 0, 0, f"auth_failed(attempt {attempt}):{type(e).__name__}:{e}"

    # --- Primary: associated devices ---
    assoc_data = _gl_json(client, "DEV2_WIFI_APDEV_ASSOCDEV")
    if assoc_data is None:
        return False, [], auth_ms, 0, "assocdev_failed"

    assoc_list = _extract_list(assoc_data, "ASSOCDEV", "devices", "list")
    devices = normalize_devices(assoc_list, cfg.secret)
    for d in devices:
        d["source"] = "associated"

    # --- Best-effort: unassociated stations (may return error 9003) ---
    unassoc_data = _gl_json(client, "DEV2_WIFI_DE_UNASSOCSTA")
    unassoc_list = _extract_list(unassoc_data, "UNASSOCSTA", "devices", "list") if unassoc_data else []
    if unassoc_list:
        unassoc_devs = normalize_devices(unassoc_list, cfg.secret)
        for d in unassoc_devs:
            d["source"] = "unassociated"
            # avoid duplicates with associated
            if not any(x["pseudonym"] == d["pseudonym"] for x in devices):
                devices.append(d)

    # --- Best-effort: nearby APs / radio info ---
    # First fetch radio config to build a radio→band lookup map.
    radio_data = _gl_json(client, "DEV2_WIFI_RADIO")
    radio_list = _extract_list(radio_data, "RADIO", "list") if radio_data else []
    radio_band_map: Dict[str, str] = {}
    for r in radio_list:
        r_mac = (r.get("X_TP_RadioMac") or r.get("RadioMac") or "").lower()
        r_band = r.get("X_TP_Band") or r.get("Band") or ""
        if r_mac and r_band:
            radio_band_map[r_mac] = r_band

    nearby_aps = []
    ap_data = _gl_json(client, "DEV2_WIFI_APDEV")
    ap_list = _extract_list(ap_data, "APDEV", "list") if ap_data else []
    for ap in ap_list:
        ssid = ap.get("X_TP_SSID") or ap.get("SSID") or ""
        radio_mac = ap.get("X_TP_RadioMac") or ap.get("RadioMac") or ""
        band = derive_band(ap.get("operatingStandard"), radio_mac, radio_band_map)
        if ssid:
            nearby_aps.append({"ssid": ssid, "band": band})

    ssid_data = _gl_json(client, "DEV2_WIFI_SSID")
    ssid_list = _extract_list(ssid_data, "SSID", "list") if ssid_data else []
    for s in ssid_list:
        ssid = s.get("X_TP_SSID") or s.get("SSID") or ""
        radio_mac = s.get("X_TP_RadioMac") or ""
        band = derive_band(s.get("operatingStandard"), radio_mac, radio_band_map)
        if band == "unknown":
            band = _ssid_band_heuristic(ssid) or band
        if ssid and not any(x["ssid"] == ssid for x in nearby_aps):
            nearby_aps.append({"ssid": ssid, "band": band})

    # Store extra data separately (will be returned as metadata)
    extra = {"nearby_aps": nearby_aps, "radios": radio_list}
    devices.append({"_extra": extra, "_meta": True})

    # Attach privacy-safe identity / fingerprint to each connected device.
    raw_all = list(assoc_list) + list(unassoc_list)
    enrich_identity(devices, raw_all, cfg.secret, cfg.sensor_id, cfg.identity_path)

    api_ms = int((time.monotonic() - t0) * 1000) - auth_ms
    return True, devices, auth_ms, api_ms, str(len(assoc_list))


# ---------------------------------------------------------------------------
# Report generation
# ---------------------------------------------------------------------------

def fmt_ts(ts: int) -> str:
    return datetime.fromtimestamp(ts, TZ).strftime("%Y-%m-%d %H:%M:%S %Z")


def fmt_slot(ts: int) -> str:
    return datetime.fromtimestamp(ts, TZ).strftime("%H:%M")


def rate_str(kbps: Optional[int]) -> str:
    if not kbps:
        return "N/A"
    if kbps >= 1000:
        return f"{kbps / 1000:.1f} Mbps"
    return f"{kbps} Kbps"


def _signal_quality(ss: Optional[int], level: Optional[int]) -> str:
    """Human-readable signal quality in Spanish."""
    if level is not None:
        labels = {0: "Sin señal", 1: "Débil", 2: "Regular", 3: "Buena", 4: "Excelente"}
        return labels.get(level, "Desconocido")
    if ss is not None and ss > 0:
        if ss >= 100:
            return "Excelente"
        if ss >= 80:
            return "Buena"
        if ss >= 60:
            return "Regular"
        if ss >= 40:
            return "Débil"
        return "Muy débil"
    return "N/D"


def _signal_bar(level: Optional[int]) -> str:
    """ASCII signal bar."""
    if level is None:
        return "[-----]"
    bars = ["[-----]", "[*----]", "[**---]", "[***--]", "[****-]", "[*****]"]
    return bars[min(level, 5)]


def _signal_emoji(lvl: Optional[int]) -> str:
    """Emoji signal bar (no CSS colors needed)."""
    if lvl is None:
        lvl = 0
    lvl = max(0, min(lvl, 4))
    return "\U0001f7e2" * lvl + "\u26aa" * (4 - lvl)


def _distance_hint(ss: Optional[int]) -> str:
    """Rough proximity hint (not exact distance)."""
    if ss is None or ss <= 0:
        return ""
    if ss >= 100:
        return "muy cerca"
    if ss >= 80:
        return "cerca"
    if ss >= 60:
        return "a cierta distancia"
    if ss >= 40:
        return "lejos"
    return "muy lejos"


def _identity_desc(idn: Dict[str, Any]) -> str:
    """Compact privacy-safe identity description for reports/email."""
    if not idn:
        return ""
    parts = []
    if idn.get("manufacturer"):
        parts.append(str(idn["manufacturer"]))
    if idn.get("brand") and idn.get("brand") != idn.get("manufacturer"):
        parts.append(str(idn["brand"]))
    if idn.get("model_guess"):
        parts.append(f"{idn['model_guess']}")
    dev_class = (idn.get("device_class") or "UNKNOWN").replace("_", " ").title()
    if dev_class and dev_class.upper() not in ("Unknown", "Generic"):
        parts.append(f"({dev_class})")
    label = idn.get("confidence_label") or "unknown"
    word = idn.get("confidence_word") or "Unknown"
    if parts:
        return f"{' '.join(parts)} — {word} ({label})"
    # Fallback: only class + confidence
    if dev_class and dev_class.upper() not in ("Unknown", "Generic"):
        return f"{dev_class} — {word} ({label})"
    return f"Dispositivo desconocido — {word} ({label})"


def _identity_html(idn: Dict[str, Any]) -> str:
    """HTML identity line for the connected-device cards."""
    if not idn:
        return ""
    desc = _identity_desc(idn)
    if not desc:
        return ""
    return f"Identidad: {desc}<br>"


def build_report(cfg: Config, cap: Dict[str, Any], devices: List[Dict[str, Any]],
                 extra: Optional[Dict[str, Any]] = None) -> Tuple[str, str, str]:
    """Build (text, html) report in Spanish. Sanitized: pseudonyms only, no MACs."""
    report_id = (
        f"detectic-ex520-"
        f"{datetime.fromtimestamp(cap['scheduled_at'], TZ).strftime('%Y%m%dT%H%M%S')}"
        f"-{cap['capture_id']}"
    )
    scheduled = fmt_ts(cap["scheduled_at"])
    started = fmt_ts(cap["started_at"])
    completed = fmt_ts(cap["completed_at"] or int(time.time()))
    api_ms = cap["api_latency_ms"]
    auth_ms = cap["auth_latency_ms"]
    active = [d for d in devices if d["status"] == "active"]
    inactive = [d for d in devices if d["status"] != "active"]

    # Derive observed bands/protocols from actual device data
    observed_bands = sorted({d["band"] for d in devices if d.get("band") and d["band"] != "unknown"})
    observed_stds = sorted({d["operating_standard"] for d in devices if d.get("operating_standard")})

    # Extract nearby APs from extra metadata
    nearby_aps = (extra or {}).get("nearby_aps", [])
    # Filter out our own APs (same SSID seen on different bands is still us)
    own_ssid = cfg.sensor_id.split("-")[-1] if "-" in cfg.sensor_id else ""

    # --- Plain text version ---
    lines = [
        "=" * 56,
        "  DETECTIC — Informe de Observación Autónoma",
        "  TP-Link EX520 | Sensor Wi-Fi",
        "=" * 56,
        "",
        f"Sensor:            {cfg.sensor_id}",
        f"Dispositivo:       TP-Link EX520V (solo lectura)",
        f"ID del informe:    {report_id}",
        "",
        f"Programado:        {scheduled}",
        f"Inicio captura:    {started}",
        f"Fin captura:       {completed}",
        f"Estado:            {cap['status']}",
        f"Dispositivos:      {cap['device_count']} total ({cap['active_device_count']} conectados)",
        f"Latencia auth:     {auth_ms} ms",
        f"Latencia API:      {api_ms} ms",
        "",
    ]

    if active:
        lines.append(f"--- Dispositivos Conectados ({len(active)}) ---")
        lines.append("  Estos dispositivos están conectados al router:")
        lines.append("")
        for d in active[:MAX_REPORT_DEVICES]:
            ss = d["signal_strength"] if d["signal_strength"] and d["signal_strength"] > 0 else None
            lvl = d["signal_level"]
            quality = _signal_quality(ss, lvl)
            bar = _signal_bar(lvl)
            prox = _distance_hint(ss)
            hostname = d["hostname"] or "sin nombre"
            band = d["band"] or "?"
            std = d["operating_standard"] or "?"
            tx = rate_str(d["tx_rate_kbps"])
            rx = rate_str(d["rx_rate_kbps"])
            lines.append(f"  {hostname}")
            lines.append(f"    Señal:    {bar} {quality} (nivel {lvl or '?'}/4)")
            lines.append(f"    Red:      {band} | {std}")
            lines.append(f"    Velocidad: ↓{tx}  ↑{rx}")
            idn = d.get("identity")
            if idn:
                idline = _identity_desc(idn)
                if idline:
                    lines.append(f"    Identidad: {idline}")
            if prox:
                lines.append(f"    Distancia: ~{prox}")
            lines.append("")

    if inactive:
        lines.append(f"--- Dispositivos Fuera de Rango ({len(inactive)}) ---")
        lines.append("  Estos dispositivos fueron vistos antes pero ya no están conectados:")
        lines.append("")
        for d in inactive[:MAX_REPORT_DEVICES]:
            hostname = d["hostname"] or "sin nombre"
            band = d["band"] or "?"
            lines.append(f"  {hostname} ({band})")
        lines.append("")

    if not devices:
        lines.append("--- Sin dispositivos detectados ---")
        lines.append("  No se encontraron dispositivos Wi-Fi en esta captura.")
        lines.append("")

    lines += [
        "--- Leyenda de Señal ---",
        "  [*****] Excelente  (nivel 4) — dispositivo muy cerca del sensor",
        "  [****-] Buena      (nivel 3) — dispositivo cerca",
        "  [***--] Regular    (nivel 2) — señal aceptable",
        "  [*----] Débil      (nivel 1) — señal débil, puede perderse",
        "  [-----] Sin señal  (nivel 0) — sin conexión",
        "",
        "  Nota: La 'distancia' es una estimación basada en la señal.",
        "  La señal varía según paredes, muebles y obstáculos.",
        "",
        "--- Redes Observadas ---",
        f"  Bandas detectadas:  {', '.join(observed_bands) if observed_bands else 'N/D'}",
        f"  Protocolos:         {', '.join(observed_stds) if observed_stds else 'N/D'}",
        f"  Sensor:             TP-Link EX520V — solo lectura, sin modificaciones",
    ]

    if nearby_aps:
        lines.append("")
        lines.append(f"--- Redes Wi-Fi Detectadas ({len(nearby_aps)}) ---")
        lines.append("  Redes visibles desde el sensor:")
        lines.append("")
        for ap in nearby_aps[:20]:
            lines.append(f"  {ap['ssid']:<30} {ap['band']}")
    else:
        lines.append("")
        lines.append("--- Redes Wi-Fi Detectadas ---")
        lines.append("  No se pudieron obtener redes cercanas (firmware limitado).")

    lines += [
        "",
        "--- Privacidad ---",
        "  Todos los identificadores de dispositivos son pseudónimos HMAC-SHA256.",
        "  No se envían direcciones MAC ni credenciales.",
        "  El router no fue modificado (API de solo lectura).",
    ]
    text = "\n".join(lines)

    # --- HTML version -------------------------------------------------------
    # Responsive email design:
    #   * no tables (block layout adapts to any width)
    #   * no colors (default text only; emoji provide the visual cues)
    #   * inline styles only (email clients strip <style> blocks)
    #   * viewport meta + max-width container for phones through desktops

    def card(title: str, rows: list) -> str:
        body = "<br>".join(rows)
        return (
            "<div style=\"border-top:1px solid currentColor;padding:12px 0;"
            "word-break:break-word;overflow-wrap:anywhere;\">"
            "<div style=\"font-size:15px;line-height:1.5;\"><b>" + title + "</b></div>"
            "<div style=\"font-size:14px;line-height:1.7;\">" + body + "</div>"
            "</div>"
        )

    connected_html = ""
    for d in active[:MAX_REPORT_DEVICES]:
        ss = d["signal_strength"] if d["signal_strength"] and d["signal_strength"] > 0 else None
        lvl = d["signal_level"]
        quality = _signal_quality(ss, lvl)
        prox = _distance_hint(ss)
        hostname = d["hostname"] or "sin nombre"
        rows = [
            f"\U0001f4f6 {_signal_emoji(lvl)} {quality} (nivel {lvl if lvl is not None else '?'}/4)",
            f"\U0001f4e1 {d['band'] or '?'} \u00b7 {d['operating_standard'] or '?'}",
            f"\u2b07\ufe0f {rate_str(d['tx_rate_kbps'])} \u00a0 \u2b06\ufe0f {rate_str(d['rx_rate_kbps'])}",
        ]
        id_html = _identity_html(d.get("identity"))
        if id_html:
            rows.append("\U0001f511 " + id_html.strip())
        if prox:
            rows.append(f"\U0001f4cd ~{prox}")
        connected_html += card(hostname, rows)

    out_of_range_html = ""
    for d in inactive[:MAX_REPORT_DEVICES]:
        hostname = d["hostname"] or "sin nombre"
        out_of_range_html += card(
            hostname,
            [f"\U0001f4e1 {d['band'] or '?'} \u00b7 \U0001f4a4 desconectado"],
        )

    bands_str = ", ".join(observed_bands) if observed_bands else "N/D"
    stds_str = ", ".join(observed_stds) if observed_stds else "N/D"

    nearby_html = ""
    for ap in nearby_aps[:20]:
        nearby_html += card(ap["ssid"], [f"\U0001f4e1 {ap['band']}"])
    if not nearby_html:
        nearby_html = (
            "<p style=\"margin:0;font-size:14px;\">No se pudieron obtener redes cercanas"
            " (firmware limitado).</p>"
        )

    legend_items = [
        ("\U0001f7e2\U0001f7e2\U0001f7e2\U0001f7e2", "Excelente", "nivel 4 \u2014 dispositivo muy cerca del sensor"),
        ("\U0001f7e2\U0001f7e2\U0001f7e2\u26aa", "Buena", "nivel 3 \u2014 dispositivo cerca"),
        ("\U0001f7e2\U0001f7e2\u26aa\u26aa", "Regular", "nivel 2 \u2014 se\u00f1al aceptable"),
        ("\U0001f7e2\u26aa\u26aa\u26aa", "D\u00e9bil", "nivel 1 \u2014 puede perderse"),
        ("\u26aa\u26aa\u26aa\u26aa", "Sin se\u00f1al", "nivel 0 \u2014 sin conexi\u00f3n estable"),
    ]
    legend_html = "".join(
        "<div style=\"padding:6px 0;font-size:14px;line-height:1.6;\">"
        f"{dots} <b>{name}</b> ({desc})</div>"
        for dots, name, desc in legend_items
    )

    html = f"""<!DOCTYPE html>
<html lang="es"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>DETECTIC — Informe de Observación Autónoma</title></head>
<body style="margin:0;padding:0;">
<div style="max-width:600px;width:100%;margin:0 auto;padding:24px 16px;
  font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;
  line-height:1.5;">

  <h1 style="margin:0 0 4px 0;font-size:19px;">🛰️ DETECTIC — Informe de Observación Autónoma</h1>
  <p style="margin:0 0 4px 0;font-size:14px;">Sensor: <b>{cfg.sensor_id}</b> · TP-Link EX520V (solo lectura)</p>
  <p style="margin:0 0 20px 0;font-size:13px;">🕐 Programado: {scheduled}<br>
     Captura: {started} → {completed}</p>

  <div style="border:1px solid currentColor;padding:14px 16px;margin-bottom:20px;
    font-size:14px;line-height:1.7;">
    📊 <b>Resumen:</b> {cap['device_count']} dispositivos detectados,<br>
    ✅ <b>{cap['active_device_count']} conectados</b> ·
    😴 {len(inactive)} fuera de rango<br>
    ⚡ Estado: {cap['status']} · Auth: {auth_ms} ms · API: {api_ms} ms
  </div>

  <h2 style="margin:0 0 8px 0;font-size:16px;">📱 Dispositivos Conectados</h2>
  {connected_html if connected_html else '<p style="font-size:14px;">No hay dispositivos conectados.</p>'}

  <h2 style="margin:24px 0 8px 0;font-size:16px;">😴 Dispositivos Fuera de Rango</h2>
  {out_of_range_html if out_of_range_html else '<p style="font-size:14px;">No hay dispositivos fuera de rango.</p>'}

  <h2 style="margin:24px 0 8px 0;font-size:16px;">📶 Leyenda de Señal</h2>
  {legend_html}
  <p style="margin:8px 0 0 0;font-size:13px;line-height:1.6;">
    <b>Nota:</b> La distancia es una estimación basada en la señal RF.
    La señal varía según paredes, muebles, personas y obstáculos.</p>

  <h2 style="margin:24px 0 8px 0;font-size:16px;">🌐 Redes Wi-Fi Detectadas</h2>
  {nearby_html}

  <h2 style="margin:24px 0 8px 0;font-size:16px;">🖥️ Redes Observadas</h2>
  <div style="font-size:14px;line-height:1.8;">
    📡 Bandas detectadas: {bands_str}<br>
    📟 Protocolos: {stds_str}<br>
    🔌 Sensor: TP-Link EX520V — solo lectura, sin modificaciones
  </div>

  <div style="margin-top:28px;border-top:1px solid currentColor;padding-top:10px;
    font-size:12px;line-height:1.6;">
    🔒 Privacidad: identificadores pseudónimos HMAC-SHA256.
    Sin direcciones MAC. Router sin modificaciones.<br>
    ID: {report_id}
  </div>
</div>
</body></html>"""
    return text, html, report_id


# ---------------------------------------------------------------------------
# Email delivery
# ---------------------------------------------------------------------------

def send_email(cfg: Config, subject: str, text: str, html: str) -> None:
    msg = MIMEMultipart("alternative")
    msg["From"] = cfg.smtp_from
    msg["To"] = ", ".join(cfg.smtp_to)
    msg["Subject"] = subject
    msg.attach(MIMEText(text, "plain", "utf-8"))
    msg.attach(MIMEText(html, "html", "utf-8"))
    with smtplib.SMTP(cfg.smtp_host, cfg.smtp_port, timeout=20) as server:
        server.ehlo()
        if cfg.smtp_tls == "starttls":
            server.starttls()
            server.ehlo()
        if cfg.smtp_user and cfg.smtp_password:
            server.login(cfg.smtp_user, cfg.smtp_password)
        server.sendmail(cfg.smtp_from, cfg.smtp_to, msg.as_string())


def deliver_report(cfg: Config, store: Store, cap: Dict[str, Any],
                   text: str, html: str, report_id: str) -> Tuple[str, int]:
    """Attempt delivery up to RETRY_ATTEMPTS times. Returns (final_status, attempts).

    Attempt numbers are globally incrementing per delivery (across runs), so
    the delivery history shows cumulative retries without duplicates.
    """
    delivery_id = f"dlv-{cap['capture_id']}"
    store.update_capture(cap["capture_id"], status=DELIVERY_PENDING)
    last_error = ""
    final_status = DELIVERY_FAILED
    for _ in range(RETRY_ATTEMPTS):
        attempt = store.next_attempt_number(delivery_id)
        if attempt > 1 and RETRY_BACKOFF[(attempt - 2) % len(RETRY_BACKOFF)]:
            time.sleep(RETRY_BACKOFF[(attempt - 2) % len(RETRY_BACKOFF)])
        subject = (
            f"[DETECTIC] EX520 Autonomous Observation "
            f"{fmt_slot(cap['scheduled_at'])} — {cfg.sensor_id}"
        )
        try:
            send_email(cfg, subject, text, html)
            final_status = DELIVERED
            store.insert_delivery(delivery_id, cap["capture_id"], report_id,
                                  attempt, DELIVERED, None)
            store.update_capture(cap["capture_id"], status=DELIVERED)
            break
        except Exception as e:
            last_error = f"{type(e).__name__}: {e}"
            store.insert_delivery(delivery_id, cap["capture_id"], report_id,
                                  attempt, DELIVERY_FAILED, last_error)
    if final_status != DELIVERED:
        store.update_capture(cap["capture_id"], status=DELIVERY_FAILED)
    return final_status, attempt


# ---------------------------------------------------------------------------
# D1 sync (push capture data to Cloudflare Worker)
# ---------------------------------------------------------------------------

def _get_d1_sync_url() -> str:
    """Get D1 sync URL from environment (lazy, after .env is loaded)."""
    return env("DETECTIC_D1_SYNC_URL", "", "DETECTIC_CALLBACK_BASE")


def deliver_worker_reports(cfg: Config) -> int:
    """Consume pending email reports generated by the Worker and send them via SMTP.

    Returns number of emails delivered.
    """
    worker_url = _get_d1_sync_url()
    if not worker_url or not cfg.email_enabled or not cfg.smtp_host:
        return 0

    queue_url = f"{worker_url}/api/v1/reports/queue?limit=5"
    try:
        import urllib.request
        req = urllib.request.Request(queue_url, headers={
            "User-Agent": "detectic-collector/1.0",
            "Accept": "application/json",
        })
        with urllib.request.urlopen(req, timeout=20) as resp:
            data = json.loads(resp.read().decode())
    except Exception as e:
        print(f"worker report queue fetch failed: {type(e).__name__}: {e}", file=sys.stderr)
        return 0

    queue = data.get("queue", [])
    if not queue:
        return 0

    delivered = 0
    for item in queue:
        report_id = item.get("report_id", "unknown")
        html = item.get("html", "")
        config = json.loads(item.get("config_json") or "{}")
        to_raw = config.get("email_to") or ",".join(cfg.smtp_to)
        to_list = [t.strip() for t in to_raw.replace(";", ",").split(",") if t.strip()]
        if not to_list:
            continue

        subject = config.get("email_subject") or f"[Detectic] Informe {report_id}"
        try:
            msg = MIMEMultipart("alternative")
            msg["From"] = cfg.smtp_from
            msg["To"] = ", ".join(to_list)
            msg["Subject"] = subject
            text = item.get("text") or ""
            if text:
                msg.attach(MIMEText(text, "plain", "utf-8"))
            msg.attach(MIMEText(html, "html", "utf-8"))
            with smtplib.SMTP(cfg.smtp_host, cfg.smtp_port, timeout=20) as server:
                server.ehlo()
                if cfg.smtp_tls == "starttls":
                    server.starttls()
                    server.ehlo()
                if cfg.smtp_user and cfg.smtp_password:
                    server.login(cfg.smtp_user, cfg.smtp_password)
                server.sendmail(cfg.smtp_from, to_list, msg.as_string())
            delivered += 1
            status = "delivered"
            error = None
        except Exception as e:
            status = "failed"
            error = f"{type(e).__name__}: {e}"
            print(f"worker report delivery failed: {report_id}: {error}", file=sys.stderr)

        try:
            ack_url = f"{worker_url}/api/v1/reports/queue/{item['id']}"
            ack_body = json.dumps({"status": status, "error": error}, separators=(",", ":"))
            ack_req = urllib.request.Request(
                ack_url,
                data=ack_body.encode(),
                headers={
                    "Content-Type": "application/json",
                    "User-Agent": "detectic-collector/1.0",
                },
                method="POST",
            )
            with urllib.request.urlopen(ack_req, timeout=10):
                pass
        except Exception as e:
            print(f"worker report ack failed: {report_id}: {e}", file=sys.stderr)

    return delivered


def sync_to_d1(cfg: Config, cap: Dict[str, Any], devices: List[Dict[str, Any]],
               run: Optional[Dict[str, Any]] = None) -> bool:
    """Push capture + devices to Cloudflare D1 via Worker endpoint.

    Returns True on success, False on failure (never raises).
    """
    d1_url = _get_d1_sync_url()
    if not d1_url:
        return False

    url = f"{d1_url}/api/v1/captures/sync"

    # Build HMAC signature
    payload: Dict[str, Any] = {
        "captures": [{
            "capture_id": cap["capture_id"],
            "run_id": cap.get("run_id", ""),
            "sensor_id": cap["sensor_id"],
            "scheduled_at": cap["scheduled_at"],
            "started_at": cap["started_at"],
            "completed_at": cap.get("completed_at"),
            "status": cap["status"],
            "api_latency_ms": cap.get("api_latency_ms"),
            "auth_latency_ms": cap.get("auth_latency_ms"),
            "device_count": cap.get("device_count"),
            "active_device_count": cap.get("active_device_count"),
            "payload_hash": cap.get("payload_hash"),
            "created_at": cap.get("created_at", int(time.time())),
        }],
        "devices": {
            cap["capture_id"]: [{
                "pseudonym": d.get("pseudonym", ""),
                "hostname": d.get("hostname"),
                "band": d.get("band"),
                "signal_strength": d.get("signal_strength"),
                "signal_level": d.get("signal_level"),
                "noise": d.get("noise"),
                "operating_standard": d.get("operating_standard"),
                "tx_rate_kbps": d.get("tx_rate_kbps"),
                "rx_rate_kbps": d.get("rx_rate_kbps"),
                "status": d.get("status"),
                "bssid_pseudonym": d.get("bssid_pseudonym"),
                "identity": d.get("identity"),
            } for d in devices if not d.get("_meta")]
        },
    }

    if run:
        payload["runs"] = [{
            "run_id": run.get("run_id", ""),
            "scheduled_at": run.get("scheduled_at", 0),
            "started_at": run.get("started_at", 0),
            "completed_at": run.get("completed_at"),
            "status": run.get("status", ""),
            "duration_ms": run.get("duration_ms"),
        }]

    body = json.dumps(payload, separators=(",", ":"))
    ts = str(int(time.time()))
    # Canonical HMAC: sign timestamp + "\n" + body to bind the timestamp
    # to the payload (replay protection).
    signed = ts.encode() + b"\n" + body.encode()
    sig = hmac.new(cfg.secret, signed, hashlib.sha256).hexdigest()

    try:
        import urllib.request
        req = urllib.request.Request(
            url,
            data=body.encode(),
            headers={
                "Content-Type": "application/json",
                "X-Detectic-Sensor": cfg.sensor_id,
                "X-Detectic-Signature": sig,
                "X-Detectic-Timestamp": ts,
                "User-Agent": "detectic-collector/1.0",
            },
            method="POST",
        )
        with urllib.request.urlopen(req, timeout=10) as resp:
            return resp.status == 200
    except Exception as e:
        print(f"D1 sync failed: {type(e).__name__}: {e}", file=sys.stderr)
        return False


# ---------------------------------------------------------------------------
# Structured logging
# ---------------------------------------------------------------------------

class JobLog:
    def __init__(self, path: str):
        self.path = path
        if path:
            Path(path).parent.mkdir(parents=True, exist_ok=True)

    def emit(self, event: str, **fields) -> None:
        line = f"{datetime.now(TZ).isoformat()} [{event}] " + " ".join(
            f"{k}={v}" for k, v in fields.items()
        )
        print(line, flush=True)
        if self.path:
            try:
                with open(self.path, "a") as f:
                    f.write(line + "\n")
            except OSError:
                pass


# ---------------------------------------------------------------------------
# Main job
# ---------------------------------------------------------------------------

def run_job(cfg: Config) -> int:
    stateless = not cfg.db_path
    store: Any = NullStore() if stateless else Store(cfg.db_path)
    jlog = JobLog(cfg.log_path)

    started_at = int(time.time())
    scheduled_at = align_slot(started_at)
    run_id = run_id_for()
    capture_id = capture_id_for(cfg.sensor_id, scheduled_at)

    if not stateless:
        # Recover runs interrupted by a previous crash/kill.
        store.conn.execute(
            "UPDATE runs SET status='INTERRUPTED' "
            "WHERE status='RUNNING' AND started_at < ?",
            (started_at - 600,),
        )
        store.conn.commit()

    store.start_run(run_id, scheduled_at)
    jlog.emit("RUN_START", run_id=run_id, scheduled_at=scheduled_at,
              slot=fmt_slot(scheduled_at), capture_id=capture_id,
              sensor=cfg.sensor_id, email_enabled=cfg.email_enabled)

    existing = store.get_capture(capture_id)

    if existing and existing["status"] == DELIVERED:
        # Duplicate slot already captured AND delivered — nothing to do.
        store.finish_run(run_id, "DUPLICATE_DELIVERED")
        jlog.emit("RUN_COMPLETE", run_id=run_id, status="DUPLICATE_DELIVERED",
                  capture_id=capture_id, reason="already delivered")
        store.close()
        return 0

    exit_code = 0
    if existing:
        # Same slot already captured but not delivered → retry delivery only.
        jlog.emit("CAPTURE_EXISTS", run_id=run_id, capture_id=capture_id,
                  status=existing["status"], action="retry_delivery")
        cap = existing
        devices = store.devices_for(capture_id)
        text, html, report_id = build_report(cfg, cap, devices)
        store.update_capture(capture_id, status=REPORT_GENERATED)
        jlog.emit("REPORT_GENERATED", run_id=run_id, capture_id=capture_id,
                  report_id=report_id, devices=len(devices))
        if cfg.email_enabled and cfg.smtp_host:
            final, attempts = deliver_report(cfg, store, cap, text, html, report_id)
            jlog.emit("EMAIL_DELIVERY_ENDED", run_id=run_id, capture_id=capture_id,
                      report_id=report_id, status=final, attempts=attempts)
            if final != DELIVERED:
                exit_code = 5
        else:
            jlog.emit("EMAIL_DISABLED", run_id=run_id, capture_id=capture_id)
    else:
        # Fresh capture for this slot.
        success, raw_devices, auth_ms, api_ms, raw_count = live_capture(cfg, started_at)
        if not success:
            store.insert_capture(capture_id, run_id, cfg.sensor_id, scheduled_at,
                                 started_at, CAPTURE_FAILED, api_ms, auth_ms,
                                 None, None, None)
            store.finish_run(run_id, "CAPTURE_FAILED")
            jlog.emit("EX520_CAPTURE_FAILED", run_id=run_id, capture_id=capture_id,
                      auth_ms=auth_ms, api_ms=api_ms, reason=raw_count)
            jlog.emit("RUN_COMPLETE", run_id=run_id, status="CAPTURE_FAILED",
                      capture_id=capture_id)
            store.close()
            return 3

        # Extract extra metadata (already normalized by _live_capture_once)
        extra = {}
        devices = []
        for item in raw_devices:
            if item.get("_meta") and "_extra" in item:
                extra = item["_extra"]
            elif not item.get("_meta"):
                devices.append(item)
        dev_count = len(devices)
        active_count = sum(1 for d in devices if d["status"] == "active")
        ph = payload_hash(devices)

        # Persist BEFORE reporting success (capture must be durable first).
        store.insert_capture(capture_id, run_id, cfg.sensor_id, scheduled_at,
                             started_at, CAPTURED, api_ms, auth_ms,
                             dev_count, active_count, ph)
        store.insert_devices(capture_id, devices)
        store.update_capture(capture_id, status=PERSISTED)
        jlog.emit("EX520_CAPTURE_SUCCESS", run_id=run_id, capture_id=capture_id,
                  auth_ms=auth_ms, api_ms=api_ms, devices=dev_count,
                  active=active_count, raw_count=raw_count)

        cap = store.get_capture(capture_id)
        text, html, report_id = build_report(cfg, cap, devices, extra)
        store.update_capture(capture_id, status=REPORT_GENERATED)
        jlog.emit("REPORT_GENERATED", run_id=run_id, capture_id=capture_id,
                  report_id=report_id, devices=dev_count)

        if cfg.email_enabled and cfg.smtp_host:
            final, attempts = deliver_report(cfg, store, cap, text, html, report_id)
            jlog.emit("EMAIL_DELIVERY_ENDED", run_id=run_id, capture_id=capture_id,
                      report_id=report_id, status=final, attempts=attempts)
            if final != DELIVERED:
                exit_code = 5
        else:
            jlog.emit("EMAIL_DISABLED", run_id=run_id, capture_id=capture_id,
                      reason="no smtp config")

        # Sync to D1 (best-effort, non-blocking)
        d1_ok = sync_to_d1(cfg, cap, devices, {"run_id": run_id, "scheduled_at": scheduled_at,
                                                 "started_at": started_at, "completed_at": int(time.time()),
                                                 "status": "COMPLETE" if exit_code == 0 else "PARTIAL",
                                                 "duration_ms": int((time.time() - started_at) * 1000)})
        jlog.emit("D1_SYNC", run_id=run_id, capture_id=capture_id, ok=d1_ok)

    # Catch-up: retry other non-delivered captures (idempotent, bounded).
    pending = store.pending_deliveries()
    for p in pending:
        if p["capture_id"] == capture_id:
            continue
        pcap = store.get_capture(p["capture_id"])
        if not pcap or pcap["status"] == DELIVERED:
            continue
        pdevices = store.devices_for(p["capture_id"])
        ptext, phtml, preport = build_report(cfg, pcap, pdevices)
        jlog.emit("CATCHUP_DELIVERY", run_id=run_id, capture_id=p["capture_id"],
                  report_id=preport, prev_status=pcap["status"])
        final, attempts = deliver_report(cfg, store, pcap, ptext, phtml, preport)
        jlog.emit("EMAIL_DELIVERY_ENDED", run_id=run_id, capture_id=p["capture_id"],
                  report_id=preport, status=final, attempts=attempts)

    store.finish_run(run_id, "COMPLETE" if exit_code == 0 else "PARTIAL")
    if stateless:
        run_status = "COMPLETE" if exit_code == 0 else "PARTIAL"
    else:
        run_status = store.conn.execute(
            "SELECT status FROM runs WHERE run_id=?", (run_id,)
        ).fetchone()[0]
    # Deliver any Worker-generated email reports queued in D1.
    worker_delivered = deliver_worker_reports(cfg)
    if worker_delivered:
        jlog.emit("WORKER_REPORTS_DELIVERED", run_id=run_id, count=worker_delivered)

    jlog.emit("RUN_COMPLETE", run_id=run_id, status=run_status,
              capture_id=capture_id)
    store.close()
    return exit_code


def cmd_verify(cfg: Config, n: int = 24, json_out: bool = False) -> int:
    if not cfg.db_path:
        # No local DB — try D1 via Worker
        d1_url = _get_d1_sync_url()
        if d1_url:
            return cmd_verify_d1(cfg, n, json_out)
        msg = {
            "mode": "stateless",
            "note": "No persistent SQLite storage and no D1 sync URL configured.",
        }
        if json_out:
            print(json.dumps(msg))
        else:
            print(json.dumps(msg, indent=2))
        return 0
    store = Store(cfg.db_path)
    h = store.health()
    rows = store.recent(n)
    if json_out:
        out = {"health": h, "captures": rows}
        print(json.dumps(out, indent=2, default=str))
        store.close()
        return 0
    print("=" * 100)
    print("DETECTIC AUTONOMOUS EX520 — VERIFICATION VIEW")
    print("=" * 100)
    print(f"DB: {cfg.db_path}")
    print(f"total_captures={h['total_captures']} successful={h['successful_captures']} "
          f"delivered={h['delivered_captures']} failed={h['failed_captures']} "
          f"pending_deliveries={h['pending_deliveries']}")
    print(f"last_successful_capture={fmt_ts(h['last_successful_capture_at']) if h['last_successful_capture_at'] else '-'}")
    print(f"last_delivered_capture={fmt_ts(h['last_delivered_capture_at']) if h['last_delivered_capture_at'] else '-'}")
    print(f"last_failed_capture={fmt_ts(h['last_failed_capture_at']) if h['last_failed_capture_at'] else '-'}")
    print()
    print(f"{'SCHEDULED':<19} {'STATUS':<18} {'CAPTURE':<14} {'DEV':>4} {'ACT':>4} "
          f"{'API_MS':>7} {'AUTH_MS':>7} {'EMAIL':<16} {'ATT':>3}  DELIVERY")
    print("-" * 100)
    for r in rows:
        sch = fmt_slot(r["scheduled_at"])
        email_status = r["delivery_status"]
        att = r["delivery_attempts"]
        failed = "FAIL" if r["status"] == CAPTURE_FAILED else ""
        flag = " <<<" if r["status"] not in (DELIVERED, CAPTURE_FAILED) else ""
        print(f"{sch:<19} {r['status']:<18} {r['capture_id']:<14} "
              f"{str(r['device_count'] or '-'):>4} {str(r['active_device_count'] or '-'):>4} "
              f"{str(r['api_latency_ms'] or '-'):>7} {str(r['auth_latency_ms'] or '-'):>7} "
              f"{email_status:<16} {att:>3}  {failed}{flag}")
    print("=" * 100)
    store.close()
    return 0


def cmd_verify_d1(cfg: Config, n: int = 24, json_out: bool = False) -> int:
    """Verify via D1 through Cloudflare Worker endpoint."""
    import urllib.request
    d1_url = _get_d1_sync_url()
    url = f"{d1_url}/api/v1/captures/verify?n={n}"
    try:
        req = urllib.request.Request(url, headers={
            "Accept": "application/json",
            "User-Agent": "detectic-collector/1.0",
        })
        with urllib.request.urlopen(req, timeout=15) as resp:
            data = json.loads(resp.read())
    except Exception as e:
        print(f"D1 verify failed: {type(e).__name__}: {e}", file=sys.stderr)
        return 1

    health = data.get("health", {})
    captures = data.get("captures", [])

    if json_out:
        print(json.dumps(data, indent=2, default=str))
        return 0

    print("=" * 100)
    print("DETECTIC AUTONOMOUS EX520 — D1 VERIFICATION VIEW")
    print("=" * 100)
    print(f"D1: {d1_url}")
    print(f"total_captures={health.get('total_captures', 0)} "
          f"delivered={health.get('delivered_captures', 0)} "
          f"failed={health.get('failed_captures', 0)}")
    last_del = health.get('last_delivered_capture_at')
    last_succ = health.get('last_successful_capture_at')
    last_fail = health.get('last_failed_capture_at')
    print(f"last_successful_capture={fmt_ts(last_succ) if last_succ else '-'}")
    print(f"last_delivered_capture={fmt_ts(last_del) if last_del else '-'}")
    print(f"last_failed_capture={fmt_ts(last_fail) if last_fail else '-'}")
    print()
    print(f"{'SCHEDULED':<19} {'STATUS':<18} {'CAPTURE':<14} {'DEV':>4} {'ACT':>4} "
          f"{'API_MS':>7} {'AUTH_MS':>7}")
    print("-" * 80)
    for r in captures:
        sch = fmt_slot(r["scheduled_at"])
        failed = "FAIL" if r["status"] == CAPTURE_FAILED else ""
        print(f"{sch:<19} {r['status']:<18} {r['capture_id']:<14} "
              f"{str(r.get('device_count') or '-'):>4} {str(r.get('active_device_count') or '-'):>4} "
              f"{str(r.get('api_latency_ms') or '-'):>7} {str(r.get('auth_latency_ms') or '-'):>7}  {failed}")
    print("=" * 100)
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="Detectic autonomous EX520 collector")
    ap.add_argument("cmd", choices=["run", "verify"])
    ap.add_argument("--n", type=int, default=24)
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    cfg = load_config()
    if args.cmd == "run":
        try:
            return run_job(cfg)
        except Exception as e:  # unexpected: record and fail, never fabricate
            print(f"UNEXPECTED_ERROR {type(e).__name__}: {e}", file=sys.stderr)
            return 1
    return cmd_verify(cfg, args.n, args.json)


if __name__ == "__main__":
    sys.exit(main())
