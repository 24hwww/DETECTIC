#!/usr/bin/env python3
"""Detectic EX520 event-driven email reporter.

Polls the EX520 for associated and unassociated Wi-Fi devices and sends an
email as soon as a device connects, disconnects, or an unassociated station
enters/leaves RF range.

This sensor is intentionally lightweight: it keeps a small JSON state file
(not SQLite) and sends SMTP directly.  Set EVENT_STATE_FILE="" to keep state
only in memory (state is lost on restart).

Usage:
    export DETECTIC_PASSWORD="..."
    export DETECTIC_SECRET="..."          # HMAC secret for pseudonyms
    export DETECTIC_SMTP_PASSWORD="..."
    python3 autonomous/event_reporter.py

Environment variables (all override .env):
    DETECTIC_URL              EX520 URL (default IPv6 link-local)
    DETECTIC_USER, DETECTIC_PASSWORD
    DETECTIC_DIALECT          gdpr-json | gdpr-text
    DETECTIC_SENSOR_ID        default detectic-ex520-live
    DETECTIC_SECRET           hex secret for pseudonyms
    DETECTIC_SMTP_*           host, port, user, password, from, to, tls
    AUTONOMOUS_*              same prefixes as collector
    EVENT_POLL_INTERVAL       seconds between polls (default 30)
    EVENT_ABSENCE_THRESHOLD   consecutive misses before "gone" (default 2)
    EVENT_COOLDOWN            seconds before re-emitting same event (default 300)
    EVENT_STATE_FILE          JSON state path (default event_state.json; "" = memory)
    EVENT_LOG_PATH            log file (default logs/event_reporter.log)
"""

from __future__ import annotations

import contextlib
import hashlib
import hmac
import io
import json
import os
import signal
import smtplib
import sys
import threading
import time
from dataclasses import dataclass
from datetime import datetime, timezone, timedelta
from email.mime.multipart import MIMEMultipart
from email.mime.text import MIMEText
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

# Reuse the proven GTPR client
_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE.parent / "python"))
from detectic_client import Dialect, GtprClient  # noqa: E402

# Stable device identity (huella) + alias map
from identity import (  # noqa: E402
    AliasMap,
    DeviceClass,
    JsonAliasMap,
    classify_mac,
    infer_device_class,
    is_generic_hostname,
    manufacturer as oui_manufacturer,
    stable_fingerprint,
)

DEFAULT_URL = "http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]"
TZ = timezone(timedelta(hours=-3), name="BRT")
ASSOC_OID = "DEV2_WIFI_APDEV_ASSOCDEV"
UNASSOC_OID = "DEV2_WIFI_DE_UNASSOCSTA"
REQUEST_TIMEOUT = 15

KNOWN_RADIO_BANDS = {
    "3c:6a:d2:5f:ab:c1": "2.4GHz",
    "3c:6a:d2:5f:ab:c3": "5GHz",
}


def load_dotenv(path: str) -> None:
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


def env_float(key: str, default: float, alt: str = "") -> float:
    try:
        return float(env(key, str(default), alt))
    except (ValueError, TypeError):
        return default


@dataclass
class Config:
    url: str
    user: str
    password: str
    dialect: str
    sensor_id: str
    secret: bytes
    smtp_host: str
    smtp_port: int
    smtp_user: str
    smtp_password: str
    smtp_from: str
    smtp_to: List[str]
    smtp_tls: str
    email_enabled: bool
    email_mode: str
    poll_interval: int
    absence_threshold: int
    cooldown: int
    state_file: Optional[str]
    log_path: str
    retention: int
    alias_map_path: Optional[str]


def load_config() -> Config:
    repo = _HERE.parent
    load_dotenv(str(repo / ".env"))

    url = env("AUTONOMOUS_URL", DEFAULT_URL)
    user = env("AUTONOMOUS_USER", env("DETECTIC_USER", "user"))
    password = env("AUTONOMOUS_PASSWORD", env("DETECTIC_PASSWORD", ""))
    secret_hex = env("AUTONOMOUS_SECRET", env("DETECTIC_SECRET", ""))
    dialect = env("AUTONOMOUS_DIALECT", env("DETECTIC_DIALECT", "gdpr-json"))
    dialect = {"json": "gdpr-json", "text": "gdpr-text"}.get(dialect, dialect)

    smtp_host = env("AUTONOMOUS_SMTP_HOST", env("DETECTIC_SMTP_HOST", ""))
    smtp_port = env_int("AUTONOMOUS_SMTP_PORT", env_int("DETECTIC_SMTP_PORT", 587))
    smtp_user = env("AUTONOMOUS_SMTP_USER", env("DETECTIC_SMTP_USER", ""))
    smtp_password = env("AUTONOMOUS_SMTP_PASSWORD", env("DETECTIC_SMTP_PASSWORD", ""))
    smtp_from = env("AUTONOMOUS_SMTP_FROM", env("DETECTIC_SMTP_FROM", ""))
    smtp_to_raw = env("AUTONOMOUS_SMTP_TO", env("DETECTIC_SMTP_TO", ""))
    smtp_tls = env("AUTONOMOUS_SMTP_TLS", env("DETECTIC_SMTP_TLS", "starttls"))
    smtp_to = [t.strip() for t in smtp_to_raw.replace(";", ",").split(",") if t.strip()]
    email_enabled = env_int("AUTONOMOUS_EMAIL_ENABLED", 1 if smtp_host and smtp_to else 0)

    if not secret_hex:
        if os.environ.get("AUTONOMOUS_ALLOW_DEV_SECRET", "0") != "1":
            raise ValueError(
                "no sensor secret configured: set AUTONOMOUS_SECRET or "
                "DETECTIC_SECRET (development only: AUTONOMOUS_ALLOW_DEV_SECRET=1)"
            )
        print(
            "[event_reporter] WARNING: using development secret "
            "(AUTONOMOUS_ALLOW_DEV_SECRET=1)",
            file=sys.stderr,
        )
        secret = b"detectic-autonomous-dev-secret"
    else:
        try:
            secret = bytes.fromhex(secret_hex)
        except ValueError as exc:
            raise ValueError(
                "AUTONOMOUS_SECRET/DETECTIC_SECRET is not valid hex"
            ) from exc

    state_file_raw = env("EVENT_STATE_FILE", str(_HERE / "event_state.json"))
    state_file = state_file_raw if state_file_raw else None
    alias_map_raw = env("EVENT_ALIAS_MAP", str(_HERE / "alias_map.json"))
    alias_map_path = alias_map_raw if alias_map_raw else None
    email_mode = env("EVENT_EMAIL_MODE", "individual").lower()
    if email_mode not in ("individual", "batch"):
        email_mode = "individual"

    return Config(
        url=url, user=user, password=password, dialect=dialect,
        sensor_id=env("AUTONOMOUS_SENSOR_ID", env("DETECTIC_SENSOR_ID", "ex520-001")),
        secret=secret,
        smtp_host=smtp_host, smtp_port=smtp_port, smtp_user=smtp_user,
        smtp_password=smtp_password, smtp_from=smtp_from, smtp_to=smtp_to,
        smtp_tls=smtp_tls, email_enabled=bool(email_enabled), email_mode=email_mode,
        poll_interval=env_int("EVENT_POLL_INTERVAL", 30),
        absence_threshold=env_int("EVENT_ABSENCE_THRESHOLD", 2),
        cooldown=env_int("EVENT_COOLDOWN", 300),
        state_file=state_file,
        log_path=env("EVENT_LOG_PATH", str(_HERE / "logs" / "event_reporter.log")),
        retention=env_int("EVENT_RETENTION", 86400),
        alias_map_path=alias_map_path,
    )


class JobLog:
    def __init__(self, path: str):
        self.path = path
        if path:
            Path(path).parent.mkdir(parents=True, exist_ok=True)

    def emit(self, event: str, **fields):
        line = f"{datetime.now(TZ).isoformat()} [{event}] " + " ".join(
            f"{k}={v}" for k, v in fields.items()
        )
        if self.path:
            try:
                with open(self.path, "a") as f:
                    f.write(line + "\n")
            except OSError:
                pass
        else:
            print(line, flush=True)


def pseudonymize(secret: bytes, identifier: str) -> str:
    return hmac.new(secret, identifier.encode(), hashlib.sha256).hexdigest()[:16]


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


def _device_stable_fingerprint(secret: bytes, mac: str, hostname: Optional[str],
                               protocol: Optional[str], band: Optional[str]) -> Dict[str, Any]:
    """Compute the stable fingerprint_id (huella) for a device.

    Returns {fingerprint_id, fingerprint_method, fingerprint_confidence,
    manufacturer, device_class}. The fingerprint_id is stable across reconnects
    and band switches (hostname-led), unlike the MAC pseudonym.
    """
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


def _normalize_common(d: Dict[str, Any], secret: bytes) -> Optional[Dict[str, Any]]:
    """Common normalization for ASSOCDEV and UNASSOCSTA entries.

    Computes both the MAC pseudonym (per-band, may rotate) and the stable
    fingerprint_id (huella, stable across reconnects/bands).
    """
    mac = str(d.get("MACAddress") or "").strip()
    radio_mac = str(d.get("X_TP_RadioMac") or "").strip()
    if not mac:
        return None
    identity = mac or str(d.get("X_TP_IPAddress") or "") or \
               str(d.get("X_TP_HostName") or "") or "unknown"
    standard = str(d.get("operatingStandard") or "").strip() or None
    hostname = str(d.get("X_TP_HostName") or "").strip() or None
    band = derive_band(standard, radio_mac)
    fp = _device_stable_fingerprint(secret, mac, hostname, standard, band)
    return {
        "pseudonym": pseudonymize(secret, identity),
        "fingerprint_id": fp["fingerprint_id"],
        "fingerprint_method": fp["fingerprint_method"],
        "fingerprint_confidence": fp["fingerprint_confidence"],
        "manufacturer": fp["manufacturer"],
        "device_class": fp["device_class"],
        "mac": mac,
        "hostname": hostname,
        "band": band,
        "signal_strength": to_int(d.get("signalStrength")),
        "signal_level": to_int(d.get("X_TP_SignalStrengthLevel")),
        "operating_standard": standard,
        "radio_mac": radio_mac or None,
    }


def normalize_assoc(raw: List[Dict[str, Any]], secret: bytes) -> List[Dict[str, Any]]:
    out = []
    for d in raw:
        n = _normalize_common(d, secret)
        if n:
            n["source"] = "associated"
            n["active"] = str(d.get("active", "1")) == "1"
            out.append(n)
    return out


def normalize_unassoc(raw: List[Dict[str, Any]], secret: bytes) -> List[Dict[str, Any]]:
    out = []
    for d in raw:
        n = _normalize_common(d, secret)
        if n:
            n["source"] = "unassociated"
            n["active"] = True
            out.append(n)
    return out


def _parse_gl(client: GtprClient, oid: str) -> Tuple[bool, List[Dict[str, Any]], str]:
    """Issue a gl and return (success, device_list, detail)."""
    try:
        raw = client.gl(oid)
    except Exception as e:
        return False, [], f"{type(e).__name__}: {e}"
    try:
        parsed = json.loads(raw)
    except Exception as e:
        return False, [], f"json_error: {type(e).__name__}"
    if parsed.get("success") is False:
        return False, [], f"errorcode={parsed.get('errorcode')}"
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
    if not isinstance(data, list):
        return False, [], "unexpected_data_shape"
    return True, data, "ok"


def poll_devices(client: GtprClient, cfg: Config, jlog: JobLog) -> Tuple[List[Dict], List[Dict], str]:
    """Poll associated and (best-effort) unassociated devices."""
    # Enforce request timeout on the session
    _orig_post = client.session.post

    def _post_timeout(*args, **kwargs):
        kwargs.setdefault("timeout", REQUEST_TIMEOUT)
        return _orig_post(*args, **kwargs)

    client.session.post = _post_timeout

    with contextlib.redirect_stdout(io.StringIO()):
        ok_a, raw_a, detail_a = _parse_gl(client, ASSOC_OID)
    if not ok_a:
        jlog.emit("ASSOC_POLL_FAILED", reason=detail_a)
        return [], [], detail_a

    assoc = normalize_assoc(raw_a, cfg.secret)

    with contextlib.redirect_stdout(io.StringIO()):
        ok_u, raw_u, detail_u = _parse_gl(client, UNASSOC_OID)
    if ok_u:
        unassoc = normalize_unassoc(raw_u, cfg.secret)
    else:
        jlog.emit("UNASSOC_POLL_EMPTY", reason=detail_u)
        unassoc = []

    return assoc, unassoc, "ok"


def _event_allowed(prev: Dict[str, Any], event_type: str, now: int, cooldown: int) -> bool:
    last = prev.get("last_event", {}).get(event_type, 0)
    return (now - last) >= cooldown


def _set_last_event(info: Dict[str, Any], event_type: str, now: int) -> None:
    info.setdefault("last_event", {})[event_type] = now


def _band_info(d: Dict[str, Any]) -> Dict[str, Any]:
    """Per-band sub-identity record (the band-level view of a device)."""
    return {
        "mac_pseudonym": d.get("pseudonym"),
        "signal_strength": d.get("signal_strength"),
        "signal_level": d.get("signal_level"),
        "operating_standard": d.get("operating_standard"),
        "radio_mac": d.get("radio_mac"),
        "last_seen": 0,
    }


def detect_events(
    cfg: Config,
    prev_state: Dict[str, Any],
    assoc: List[Dict[str, Any]],
    unassoc: List[Dict[str, Any]],
    now: int,
    jlog: JobLog,
    alias_map: Optional[AliasMap] = None,
) -> Tuple[List[Dict[str, Any]], Dict[str, Any]]:
    """Compare current poll with previous state and return events + new state.

    State is keyed by the **stable fingerprint_id** (huella), not the MAC
    pseudonym. A single device seen on multiple bands (different MACs) or that
    rotated its MAC on reconnect collapses into one entry. Per-band info is
    kept under the ``bands`` sub-dict (the band sub-id), so 2.4GHz vs 5GHz
    detail is not lost.

    Events:
      * ``connected`` / ``unassociated_in_range``: fired when the device
        (fingerprint_id) transitions into the state, regardless of band.
      * ``disconnected`` / ``unassociated_left``: fired only when the device
        is not seen on ANY band for ``absence_threshold`` consecutive polls.
      * ``band_changed``: fired when an already-connected device adds a new
        band (e.g. roams from 2.4GHz to 5GHz). Does NOT fire connect/disconnect.

    The event ``device_id`` is the fingerprint_id; the MAC pseudonym is carried
    as ``mac_pseudonym`` (an alias) plus the full alias set in ``aliases``.
    """
    prev = prev_state.get("devices", {})
    current: Dict[str, Any] = {}
    events: List[Dict[str, Any]] = []
    baseline = prev_state.get("baseline", True)

    def _register_alias(fp_id: str, d: Dict[str, Any]) -> None:
        if alias_map is None:
            return
        try:
            alias_map.register(
                fp_id, d["pseudonym"], ts=now,
                hostname=d.get("hostname") or None, band=d.get("band") or None,
            )
        except Exception:
            pass

    def _device_event(event_type: str, fp_id: str, info: Dict[str, Any],
                      d: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        band = (d or {}).get("band") or info.get("band", "unknown")
        ev = {
            "event_type": event_type,
            "device_id": fp_id,
            "hostname": info.get("hostname") or "unknown",
            "band": band,
            "signal_strength": (d or {}).get("signal_strength") or info.get("signal_strength"),
            "timestamp": now,
            # Stable identity metadata
            "fingerprint_id": fp_id,
            "fingerprint_method": info.get("fingerprint_method"),
            "fingerprint_confidence": info.get("fingerprint_confidence"),
            "manufacturer": info.get("manufacturer"),
            "device_class": info.get("device_class"),
            # MAC alias for this observation (per-band / may rotate)
            "mac_pseudonym": (d or {}).get("pseudonym"),
            "aliases": sorted(info.get("aliases", set())),
            "bands": sorted(info.get("bands", {}).keys()),
        }
        return ev

    # --- Process connected devices, grouped by fingerprint_id ---
    seen_fps: set = set()
    # Map fingerprint_id -> list of band observations (a device may be on >1 band)
    assoc_by_fp: Dict[str, List[Dict[str, Any]]] = {}
    for d in assoc:
        fp = d["fingerprint_id"]
        assoc_by_fp.setdefault(fp, []).append(d)

    for fp, band_obs in assoc_by_fp.items():
        seen_fps.add(fp)
        info = prev.get(fp, {}).copy() if fp in prev else {}
        # Carry stable identity metadata forward
        info["fingerprint_method"] = info.get("fingerprint_method") or band_obs[0].get("fingerprint_method")
        info["fingerprint_confidence"] = info.get("fingerprint_confidence") or band_obs[0].get("fingerprint_confidence")
        info["manufacturer"] = info.get("manufacturer") or band_obs[0].get("manufacturer")
        info["device_class"] = info.get("device_class") or band_obs[0].get("device_class")
        info["hostname"] = band_obs[0].get("hostname") or info.get("hostname")
        # Per-band sub-tracking
        bands = dict(info.get("bands", {}))
        prev_bands = set(bands.keys())
        for d in band_obs:
            b = d.get("band") or "unknown"
            bi = _band_info(d)
            bi["last_seen"] = now
            bands[b] = bi
            _register_alias(fp, d)
        info["bands"] = bands
        # Top-level band = the strongest / most recent band observed
        best = max(band_obs, key=lambda x: x.get("signal_level") or 0)
        info["band"] = best.get("band") or info.get("band")
        info["signal_strength"] = best.get("signal_strength")
        info["signal_level"] = best.get("signal_level")
        info["operating_standard"] = best.get("operating_standard")
        info["radio_mac"] = best.get("radio_mac")
        info["last_seen"] = now
        info["misses"] = 0
        info["aliases"] = sorted(set(info.get("aliases", [])) | {d["pseudonym"] for d in band_obs})
        old_state = prev.get(fp, {}).get("state", "absent")
        info["state"] = "connected"
        current[fp] = info
        if not baseline:
            if old_state != "connected":
                if _event_allowed(info, "connected", now, cfg.cooldown):
                    events.append(_device_event("connected", fp, info, best))
                    _set_last_event(info, "connected", now)
            else:
                # Already connected: emit band_changed if a new band appeared
                new_bands = set(bands.keys()) - prev_bands
                if new_bands and _event_allowed(info, "band_changed", now, cfg.cooldown):
                    nd = next(d for d in band_obs if (d.get("band") or "unknown") in new_bands)
                    events.append(_device_event("band_changed", fp, info, nd))
                    _set_last_event(info, "band_changed", now)

    # --- Process unassociated devices not already connected ---
    unassoc_by_fp: Dict[str, List[Dict[str, Any]]] = {}
    for d in unassoc:
        fp = d["fingerprint_id"]
        if fp in seen_fps:
            continue
        unassoc_by_fp.setdefault(fp, []).append(d)

    for fp, band_obs in unassoc_by_fp.items():
        seen_fps.add(fp)
        info = prev.get(fp, {}).copy() if fp in prev else {}
        info["fingerprint_method"] = info.get("fingerprint_method") or band_obs[0].get("fingerprint_method")
        info["fingerprint_confidence"] = info.get("fingerprint_confidence") or band_obs[0].get("fingerprint_confidence")
        info["manufacturer"] = info.get("manufacturer") or band_obs[0].get("manufacturer")
        info["device_class"] = info.get("device_class") or band_obs[0].get("device_class")
        info["hostname"] = band_obs[0].get("hostname") or info.get("hostname")
        bands = dict(info.get("bands", {}))
        for d in band_obs:
            b = d.get("band") or "unknown"
            bi = _band_info(d)
            bi["last_seen"] = now
            bands[b] = bi
            _register_alias(fp, d)
        info["bands"] = bands
        best = max(band_obs, key=lambda x: x.get("signal_level") or 0)
        info["band"] = best.get("band") or info.get("band")
        info["signal_strength"] = best.get("signal_strength")
        info["signal_level"] = best.get("signal_level")
        info["operating_standard"] = best.get("operating_standard")
        info["radio_mac"] = best.get("radio_mac")
        info["last_seen"] = now
        info["misses"] = 0
        info["aliases"] = sorted(set(info.get("aliases", [])) | {d["pseudonym"] for d in band_obs})
        old_state = prev.get(fp, {}).get("state", "absent")
        info["state"] = "unassociated_in_range"
        current[fp] = info
        if not baseline and old_state != "unassociated_in_range":
            if _event_allowed(info, "unassociated_in_range", now, cfg.cooldown):
                events.append(_device_event("unassociated_in_range", fp, info, best))
                _set_last_event(info, "unassociated_in_range", now)

    # --- Devices that disappeared (not seen on ANY band) ---
    for fp, info in prev.items():
        if fp in current:
            continue
        misses = info.get("misses", 0) + 1
        if misses >= cfg.absence_threshold:
            new_state = "absent"
            old_state = info.get("state", "absent")
            if not baseline and old_state != "absent":
                event_type = "disconnected" if old_state == "connected" else "unassociated_left"
                if _event_allowed(info, event_type, now, cfg.cooldown):
                    events.append(_device_event(event_type, fp, info))
                    _set_last_event(info, event_type, now)
            current[fp] = {**info, "state": new_state, "misses": misses, "last_seen": now}
        else:
            current[fp] = {**info, "misses": misses, "last_seen": now}

    # Cleanup stale absent devices
    cutoff = now - cfg.retention
    for fp in list(current):
        if current[fp].get("state") == "absent" and current[fp].get("last_seen", now) < cutoff:
            del current[fp]

    new_state = {"devices": current, "baseline": False, "last_poll": now}
    return events, new_state


def load_state(cfg: Config) -> Dict[str, Any]:
    if not cfg.state_file or not os.path.exists(cfg.state_file):
        return {"devices": {}, "baseline": True, "last_poll": 0}
    try:
        with open(cfg.state_file, "r", encoding="utf-8") as f:
            return json.load(f)
    except Exception:
        return {"devices": {}, "baseline": True, "last_poll": 0}


def save_state(cfg: Config, state: Dict[str, Any]) -> None:
    if not cfg.state_file:
        return
    tmp = cfg.state_file + ".tmp"
    try:
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(state, f, indent=2)
        os.replace(tmp, cfg.state_file)
    except OSError:
        pass


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


def _fmt_ts(ts: int) -> str:
    return datetime.fromtimestamp(ts, TZ).strftime("%Y-%m-%d %H:%M:%S %Z")


def _event_emoji(et: str) -> str:
    return {
        "connected": "🟢",
        "disconnected": "🔴",
        "unassociated_in_range": "🟣",
        "unassociated_left": "⚪",
        "band_changed": "🔵",
    }.get(et, "⚫")


def build_event_email(cfg: Config, events: List[Dict[str, Any]]) -> Tuple[str, str]:
    now = int(time.time())
    ts = _fmt_ts(now)
    counts: Dict[str, int] = {}
    for e in events:
        counts[e["event_type"]] = counts.get(e["event_type"], 0) + 1

    text_lines = [
        "=" * 56,
        "  DETECTIC EX520 EVENT REPORT",
        "=" * 56,
        f"Sensor:  {cfg.sensor_id}",
        f"Time:    {ts}",
        f"Events:  {len(events)}",
        "",
    ]
    for et, n in counts.items():
        text_lines.append(f"  {et}: {n}")
    text_lines.append("")
    text_lines.append("--- Devices ---")
    for e in events:
        sig = e.get("signal_strength")
        sig_s = str(sig) if sig is not None else "N/A"
        text_lines.append(
            f"  {_event_emoji(e['event_type'])} {e['event_type']:<26} "
            f"{e['device_id']}  {e.get('hostname','-'):<18} "
            f"{e.get('band','?'):<6} signal={sig_s}"
        )
    text_lines += [
        "",
        "Privacy: pseudonymized HMAC-SHA256 identifiers only.",
        "No raw MAC addresses are included in this report.",
    ]
    text = "\n".join(text_lines)

    # Responsive, table-free, color-free HTML (emoji as visual cues).
    cards = ""
    for e in events:
        sig = e.get("signal_strength")
        sig_s = str(sig) if sig is not None else "N/D"
        emoji = _event_emoji(e["event_type"])
        cards += (
            "<div style=\"border-top:1px solid currentColor;padding:10px 0;"
            "font-size:14px;line-height:1.7;word-break:break-word;overflow-wrap:anywhere;\">"
            f"{emoji} <b>{e['event_type']}</b><br>"
            f"\U0001f506 {sig_s} \u00b7 \U0001f4e1 {e.get('band','?')}<br>"
            f"\U0001f4f1 {e.get('hostname','-')} \u00b7 <code>{e['device_id']}</code>"
            "</div>"
        )

    summary = " \u00b7 ".join(f"{_event_emoji(et)} {n}" for et, n in counts.items())

    html = f"""<!DOCTYPE html>
<html lang="es"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Detectic EX520 — Event Report</title></head>
<body style="margin:0;padding:0;">
<div style="max-width:600px;width:100%;margin:0 auto;padding:24px 16px;
  font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;
  line-height:1.5;">
  <h2 style="margin:0 0 8px 0;font-size:18px;">🛰️ Detectic EX520 — Event Report</h2>
  <p style="margin:0 0 16px 0;font-size:14px;line-height:1.7;">
    Sensor: <b>{cfg.sensor_id}</b><br>
    🕐 Time: {ts}<br>
    📊 Events: {len(events)} ({summary})
  </p>
  {cards}
  <div style="margin-top:24px;border-top:1px solid currentColor;padding-top:10px;
    font-size:12px;line-height:1.6;">
    🔒 Privacy: HMAC-SHA256 pseudonyms only. No raw MACs.
  </div>
</div>
</body></html>"""
    return text, html


def build_single_event_email(cfg: Config, e: Dict[str, Any]) -> Tuple[str, str, str]:
    ts = _fmt_ts(e["timestamp"])
    et = e["event_type"]
    emoji = _event_emoji(et)
    sig = e.get("signal_strength")
    sig_s = str(sig) if sig is not None else "N/A"
    hostname = e.get("hostname") or "unknown"

    subject = f"[DETECTIC EX520] {emoji} {et.replace('_', ' ').title()}: {e['device_id'][:12]}"

    text = f"""=====================================
  DETECTIC EX520 EVENT
=====================================
Sensor:    {cfg.sensor_id}
Time:      {ts}
Event:     {et}
Device:    {e['device_id']}
Hostname:  {hostname}
Band:      {e.get('band', '?')}
Signal:    {sig_s}

Privacy: HMAC-SHA256 pseudonym only. No raw MAC.
"""

    html = f"""<!DOCTYPE html>
<html lang="es"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Detectic EX520 Event</title></head>
<body style="margin:0;padding:0;">
<div style="max-width:600px;width:100%;margin:0 auto;padding:24px 16px;
  font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;
  line-height:1.5;">
  <h2 style="margin:0 0 12px 0;font-size:18px;">{emoji} Detectic EX520 Event</h2>
  <div style="border:1px solid currentColor;padding:14px 16px;margin-bottom:20px;
    font-size:14px;line-height:1.8;word-break:break-word;overflow-wrap:anywhere;">
    Sensor: <b>{cfg.sensor_id}</b><br>
    🕐 Time: {ts}<br>
    📊 Event: {et}<br>
    🆔 Device: <code>{e['device_id']}</code><br>
    📱 Hostname: {hostname}<br>
    📡 Band: {e.get('band','?')}<br>
    📶 Signal: {sig_s}
  </div>
  <div style="border-top:1px solid currentColor;padding-top:10px;
    font-size:12px;line-height:1.6;">
    🔒 Privacy: HMAC-SHA256 pseudonyms only. No raw MACs.
  </div>
</div>
</body></html>"""
    return subject, text, html


def send_event_report(cfg: Config, events: List[Dict[str, Any]], jlog: JobLog) -> None:
    if not cfg.email_enabled or not cfg.smtp_host or not cfg.smtp_to:
        jlog.emit("EMAIL_DISABLED", reason="missing_smtp_config", events=len(events))
        return

    if cfg.email_mode == "batch" and len(events) > 1:
        # One summary email for the whole poll
        counts: Dict[str, int] = {}
        for e in events:
            counts[e["event_type"]] = counts.get(e["event_type"], 0) + 1
        summary = ", ".join(f"{k}={v}" for k, v in counts.items())
        subject = f"[DETECTIC EX520] {len(events)} eventos — {summary}"
        text, html = build_event_email(cfg, events)
        try:
            send_email(cfg, subject, text, html)
            jlog.emit("EVENT_EMAIL_SENT", events=len(events), subject=subject)
        except Exception as exc:
            jlog.emit("EVENT_EMAIL_FAILED", error=f"{type(exc).__name__}: {exc}", events=len(events))
        return

    # Default / individual: one email per event
    for e in events:
        subject, text, html = build_single_event_email(cfg, e)
        try:
            send_email(cfg, subject, text, html)
            jlog.emit("EVENT_EMAIL_SENT", event_type=e["event_type"], device=e["device_id"], subject=subject)
        except Exception as exc:
            jlog.emit("EVENT_EMAIL_FAILED", error=f"{type(exc).__name__}: {exc}",
                      event_type=e["event_type"], device=e["device_id"])


def run(cfg: Config, jlog: JobLog, stop_event) -> None:
    state = load_state(cfg)
    if state.get("baseline", True):
        jlog.emit("BASELINE_ESTABLISH", message="first poll will suppress events; subsequent polls trigger emails")

    alias_map = JsonAliasMap(cfg.alias_map_path) if cfg.alias_map_path else AliasMap()

    client = GtprClient(cfg.url, cfg.user, cfg.password, cfg.dialect)
    jlog.emit("EVENT_REPORTER_START", url=cfg.url, interval=cfg.poll_interval,
              state_file=cfg.state_file or "memory-only",
              absence_threshold=cfg.absence_threshold, cooldown=cfg.cooldown)

    while not stop_event.is_set():
        t0 = time.time()
        try:
            with contextlib.redirect_stdout(io.StringIO()):
                client.connect()
        except Exception as e:
            jlog.emit("AUTH_FAILED", error=f"{type(e).__name__}: {e}")
            stop_event.wait(max(5, cfg.poll_interval))
            continue

        try:
            assoc, unassoc, detail = poll_devices(client, cfg, jlog)
        except Exception as e:
            jlog.emit("POLL_EXCEPTION", error=f"{type(e).__name__}: {e}")
            stop_event.wait(max(5, cfg.poll_interval))
            continue

        now = int(time.time())
        events, state = detect_events(cfg, state, assoc, unassoc, now, jlog, alias_map)
        save_state(cfg, state)

        if state.get("baseline", True):
            jlog.emit("BASELINE_SET", assoc=len(assoc), unassoc=len(unassoc))
            state["baseline"] = False
            save_state(cfg, state)
        elif events:
            jlog.emit("EVENTS_DETECTED", count=len(events),
                      connected=sum(1 for e in events if e["event_type"] == "connected"),
                      disconnected=sum(1 for e in events if e["event_type"] == "disconnected"),
                      unassociated_in_range=sum(1 for e in events if e["event_type"] == "unassociated_in_range"),
                      unassociated_left=sum(1 for e in events if e["event_type"] == "unassociated_left"))
            send_event_report(cfg, events, jlog)
        else:
            jlog.emit("POLL_OK", assoc=len(assoc), unassoc=len(unassoc))

        elapsed = time.time() - t0
        sleep_time = max(1, cfg.poll_interval - elapsed)
        stop_event.wait(sleep_time)

    jlog.emit("EVENT_REPORTER_STOP")


def main() -> int:
    cfg = load_config()
    jlog = JobLog(cfg.log_path)
    stop_event = threading.Event()

    def _handler(signum, _frame):
        jlog.emit("SIGNAL", signum=signum)
        stop_event.set()

    signal.signal(signal.SIGINT, _handler)
    signal.signal(signal.SIGTERM, _handler)

    try:
        run(cfg, jlog, stop_event)
    except Exception as e:
        jlog.emit("FATAL", error=f"{type(e).__name__}: {e}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
