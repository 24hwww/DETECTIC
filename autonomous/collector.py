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


def load_config() -> Config:
    here = Path(__file__).resolve().parent
    repo = here.parent
    load_dotenv(str(repo / ".env"))

    db = env("AUTONOMOUS_DB", "")
    sensor = env("AUTONOMOUS_SENSOR_ID", "detectic-ex520-live")
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

    if not secret_hex:
        secret = b"detectic-autonomous-dev-secret"
    else:
        try:
            secret = bytes.fromhex(secret_hex)
        except ValueError:
            secret = b"detectic-autonomous-dev-secret"

    return Config(
        db_path=db, sensor_id=sensor, url=url, user=user, password=password,
        secret=secret, dialect=dialect,
        smtp_host=smtp_host, smtp_port=smtp_port, smtp_user=smtp_user,
        smtp_password=smtp_password, smtp_from=smtp_from, smtp_to=smtp_to,
        smtp_tls=smtp_tls, email_enabled=bool(email_enabled), log_path=log_path,
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
    hostname          TEXT,
    band              TEXT,
    signal_strength   INTEGER,
    signal_level      INTEGER,
    noise             INTEGER,
    operating_standard TEXT,
    tx_rate_kbps      INTEGER,
    rx_rate_kbps      INTEGER,
    status            TEXT,
    FOREIGN KEY(capture_id) REFERENCES captures(capture_id)
);
CREATE INDEX IF NOT EXISTS idx_devobs_capture ON device_observations(capture_id);

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
            self.conn.execute(
                "INSERT OR IGNORE INTO device_observations "
                "(capture_id, pseudonym, hostname, band, signal_strength, signal_level, "
                " noise, operating_standard, tx_rate_kbps, rx_rate_kbps, status) "
                "VALUES (?,?,?,?,?,?,?,?,?,?,?)",
                (capture_id, d.get("pseudonym"), d.get("hostname"), d.get("band"),
                 d.get("signal_strength"), d.get("signal_level"), d.get("noise"),
                 d.get("operating_standard"), d.get("tx_rate_kbps"),
                 d.get("rx_rate_kbps"), d.get("status")),
            )
        self.conn.commit()

    def devices_for(self, capture_id: str) -> List[Dict[str, Any]]:
        rows = self.conn.execute(
            "SELECT pseudonym, hostname, band, signal_strength, signal_level, noise, "
            "       operating_standard, tx_rate_kbps, rx_rate_kbps, status "
            "FROM device_observations WHERE capture_id=? ORDER BY id",
            (capture_id,),
        ).fetchall()
        cols = ["pseudonym", "hostname", "band", "signal_strength", "signal_level",
                "noise", "operating_standard", "tx_rate_kbps", "rx_rate_kbps", "status"]
        return [dict(zip(cols, r)) for r in rows]

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


def derive_band(standard: Optional[str], radio_mac: Optional[str]) -> str:
    if radio_mac:
        mac_lower = radio_mac.lower()
        if mac_lower in KNOWN_RADIO_BANDS:
            return KNOWN_RADIO_BANDS[mac_lower]
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


def normalize_devices(raw_devices: List[Dict], secret: bytes) -> List[Dict[str, Any]]:
    """Pseudonymize raw ASSOCDEV entries. Raw MAC is never persisted."""
    out = []
    for d in raw_devices:
        mac = str(d.get("MACAddress") or "").strip()
        radio_mac = str(d.get("X_TP_RadioMac") or "").strip()
        identity = mac or str(d.get("X_TP_IPAddress") or "") or \
                   str(d.get("X_TP_HostName") or "") or "unknown"
        pseudo = pseudonymize(secret, identity)
        standard = str(d.get("operatingStandard") or "").strip() or None
        out.append({
            "pseudonym": pseudo,
            "hostname": str(d.get("X_TP_HostName") or "").strip() or None,
            "band": derive_band(standard, radio_mac),
            "signal_strength": to_int(d.get("signalStrength")),
            "signal_level": to_int(d.get("X_TP_SignalStrengthLevel")),
            "noise": to_int(d.get("noise")),
            "operating_standard": standard,
            "tx_rate_kbps": to_int(d.get("lastDataDownlinkRate")),
            "rx_rate_kbps": to_int(d.get("lastDataUplinkRate")),
            "status": "active" if str(d.get("active") or "0") == "1" else "inactive",
        })
    return out


def payload_hash(devices: List[Dict[str, Any]]) -> str:
    """Deterministic hash over the normalized device list."""
    compact = json.dumps(devices, sort_keys=True, separators=(",", ":"))
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

    try:
        t1 = time.monotonic()
        with redirect_stdout(io.StringIO()):
            raw = client.gl("DEV2_WIFI_APDEV_ASSOCDEV")
        api_ms = int((time.monotonic() - t1) * 1000)
    except Exception as e:
        return False, [], auth_ms, 0, f"gl_failed(attempt {attempt}):{type(e).__name__}:{e}"

    try:
        parsed = json.loads(raw)
    except Exception as e:
        return False, [], auth_ms, api_ms, f"parse_failed:{type(e).__name__}"
    if parsed.get("success") is False:
        return False, [], auth_ms, api_ms, f"errorcode={parsed.get('errorcode')}"
    data = parsed.get("data", [])
    if isinstance(data, str):
        try:
            data = json.loads(data)
        except Exception:
            data = []
    if isinstance(data, dict):
        for k in ("ASSOCDEV", "devices", "list"):
            if k in data and isinstance(data[k], list):
                data = data[k]
                break
        else:
            data = []
    return True, data, auth_ms, api_ms, str(len(data))


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


def build_report(cfg: Config, cap: Dict[str, Any], devices: List[Dict[str, Any]]) -> Tuple[str, str, str]:
    """Build (text, html) report. Sanitized: pseudonyms only, no MACs."""
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

    lines = [
        "=" * 56,
        "  DETECTIC EX520 AUTONOMOUS OBSERVATION",
        "=" * 56,
        f"Sensor:           {cfg.sensor_id}",
        f"Source:           TP-Link EX520 (GTPR/GDPR IPv6, read-only)",
        f"Report ID:        {report_id}",
        f"Capture ID:       {cap['capture_id']}",
        "",
        f"Scheduled time:   {scheduled}",
        f"Capture started:  {started}",
        f"Capture finished: {completed}",
        f"Auth latency:     {auth_ms} ms",
        f"API latency:      {api_ms} ms",
        f"Capture status:   {cap['status']}",
        f"Devices observed: {cap['device_count']}",
        f"Active devices:   {cap['active_device_count']}",
        "",
        "--- Devices (pseudonymized) ---",
    ]
    for d in devices[:MAX_REPORT_DEVICES]:
        ss = d["signal_strength"] if d["signal_strength"] and d["signal_strength"] > 0 else "N/A"
        lines.append(
            f"  {d['pseudonym'][:16]:<18} {str(d['hostname'] or '-'):<18} "
            f"{str(d['band'] or '?'):<6} signal={str(ss):<5} "
            f"level={str(d['signal_level'] or '-'):<3} {str(d['operating_standard'] or '?'):<4} "
            f"TX={rate_str(d['tx_rate_kbps']):<10} RX={rate_str(d['rx_rate_kbps']):<10} {d['status']}"
        )
    lines += [
        "",
        "RF fields available from EX520:",
        "  signalStrength (0-128), signalStrengthLevel (0-4), noise,",
        "  operatingStandard, lastDataDownlinkRate (TX), lastDataUplinkRate (RX)",
        "",
        "Privacy: all device identifiers are HMAC-SHA256 pseudonyms.",
        "No raw MAC addresses or credentials appear in this report.",
        "Router modifications: NONE (read-only API).",
    ]
    text = "\n".join(lines)

    rows = ""
    for d in devices[:MAX_REPORT_DEVICES]:
        ss = d["signal_strength"] if d["signal_strength"] and d["signal_strength"] > 0 else "-"
        rows += (
            f"<tr><td><code>{d['pseudonym'][:16]}</code></td>"
            f"<td>{d['hostname'] or '-'}</td>"
            f"<td>{d['band'] or '?'}</td>"
            f"<td>{ss}</td><td>{d['signal_level'] if d['signal_level'] is not None else '-'}</td>"
            f"<td>{d['operating_standard'] or '?'}</td>"
            f"<td>{rate_str(d['tx_rate_kbps'])}</td><td>{rate_str(d['rx_rate_kbps'])}</td>"
            f"<td>{d['status']}</td></tr>"
        )

    html = f"""<!DOCTYPE html><html><head><meta charset="utf-8"></head><body>
<h2>DETECTIC EX520 Autonomous Observation</h2>
<table cellpadding="4" cellspacing="0" style="border-collapse:collapse;font-family:sans-serif">
<tr><td><b>Sensor</b></td><td>{cfg.sensor_id}</td></tr>
<tr><td><b>Source</b></td><td>TP-Link EX520 (GTPR/GDPR IPv6, read-only)</td></tr>
<tr><td><b>Report ID</b></td><td>{report_id}</td></tr>
<tr><td><b>Capture ID</b></td><td>{cap['capture_id']}</td></tr>
<tr><td><b>Scheduled</b></td><td>{scheduled}</td></tr>
<tr><td><b>Capture started</b></td><td>{started}</td></tr>
<tr><td><b>Capture finished</b></td><td>{completed}</td></tr>
<tr><td><b>Auth latency</b></td><td>{auth_ms} ms</td></tr>
<tr><td><b>API latency</b></td><td>{api_ms} ms</td></tr>
<tr><td><b>Capture status</b></td><td>{cap['status']}</td></tr>
<tr><td><b>Devices</b></td><td>{cap['device_count']} ({cap['active_device_count']} active)</td></tr>
</table>
<h3>Devices (pseudonymized)</h3>
<table border="1" cellpadding="4" cellspacing="0" style="border-collapse:collapse;font-family:sans-serif">
<tr><th>Pseudonym</th><th>Hostname</th><th>Band</th><th>Signal</th><th>Level</th><th>Std</th><th>TX</th><th>RX</th><th>Status</th></tr>
{rows}
</table>
<p style="color:#666;font-size:12px">Privacy: HMAC-SHA256 pseudonyms only. No raw MACs. Router read-only.</p>
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

        devices = normalize_devices(raw_devices, cfg.secret)
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
        text, html, report_id = build_report(cfg, cap, devices)
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
    jlog.emit("RUN_COMPLETE", run_id=run_id, status=run_status,
              capture_id=capture_id)
    store.close()
    return exit_code


def cmd_verify(cfg: Config, n: int = 24, json_out: bool = False) -> int:
    if not cfg.db_path:
        msg = {
            "mode": "stateless",
            "note": "No persistent SQLite storage; verify is in-memory only for this run.",
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
