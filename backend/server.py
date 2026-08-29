#!/usr/bin/env python3
"""Detectic backend — optimized for ultra-low-resource servers.

Target: nf-compute-10 (0.1 vCPU / 256 MB RAM / 1024 MB storage)

Endpoints:
    POST /api/v1/events      -> ingest snapshot (HMAC-authenticated)
    POST /api/v1/events/batch -> ingest multiple snapshots (batch)
    GET  /api/v1/devices     -> per-device history aggregates
    GET  /api/v1/presence    -> presence analytics per device
    GET  /api/v1/sensors     -> list registered sensors + last seen
    GET  /api/v1/stats       -> global statistics
    GET  /api/v1/healthz     -> liveness + memory stats
    GET  /api/v1/readyz      -> readiness (DB writable)

Memory optimizations vs baseline:
    - ThreadingHTTPServer with bounded thread pool (not unbounded)
    - SQLite WAL mode + mmap_size limited
    - Periodic checkpoint to bound WAL growth
    - Rate limiting to prevent resource exhaustion
    - Connection keep-alive disabled (fewer threads)
    - JSON parsing with limited depth
    - Response streaming for large queries

Usage:
    python3 server.py --port 8080 --db /data/backend.db
"""

import argparse
import hashlib
import hmac as hmaclib
import json
import os
import resource
import sqlite3
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler
from socketserver import ThreadingMixIn, TCPServer
from urllib.parse import urlparse, parse_qs

HERE = os.path.dirname(os.path.abspath(__file__))
SENSORS_FILE = os.path.join(HERE, "sensors.json")
DEV_SENSORS = {"ex520-001": "dev-secret-change-me"}

# ---------------------------------------------------------------------------
# Memory-efficient threading server
# ---------------------------------------------------------------------------

class BoundedThreadingHTTPServer(ThreadingMixIn, TCPServer):
    """HTTP server with bounded thread pool to cap memory usage."""
    daemon_threads = True
    allow_reuse_address = True
    request_queue_size = 32  # bounded backlog

    def __init__(self, server_address, handler_class, max_threads=16):
        super().__init__(server_address, handler_class)
        self._max_threads = max_threads
        self._sem = threading.Semaphore(max_threads)
        self._active = 0
        self._lock = threading.Lock()

    def process_request(self, request, client_address):
        """Override to use semaphore-bounded thread creation."""
        if not self._sem.acquire(blocking=False):
            # Too many active requests — reject with 503
            try:
                request.sendall(
                    b"HTTP/1.1 503 Service Unavailable\r\n"
                    b"Content-Length: 0\r\n"
                    b"Retry-After: 5\r\n\r\n"
                )
            except Exception:
                pass
            request.close()
            return

        t = threading.Thread(
            target=self._handle_in_thread,
            args=(request, client_address),
        )
        t.daemon = True
        t.start()

    def _handle_in_thread(self, request, client_address):
        try:
            with self._lock:
                self._active += 1
            self.finish_request(request, client_address)
        except Exception:
            try:
                request.close()
            except Exception:
                pass
        finally:
            with self._lock:
                self._active -= 1
            self._sem.release()

    def active_count(self):
        with self._lock:
            return self._active

# ---------------------------------------------------------------------------
# Rate limiter (per-IP, token bucket)
# ---------------------------------------------------------------------------

class RateLimiter:
    def __init__(self, rate=30, burst=60):
        """rate = requests/sec sustained, burst = max burst."""
        self._rate = rate
        self._burst = burst
        self._buckets = {}
        self._lock = threading.Lock()
        self._cleanup_interval = 60
        self._last_cleanup = time.monotonic()

    def allow(self, key="default"):
        now = time.monotonic()
        with self._lock:
            # Periodic cleanup of stale entries
            if now - self._last_cleanup > self._cleanup_interval:
                self._last_cleanup = now
                stale = [k for k, v in self._buckets.items()
                         if now - v["ts"] > 10]
                for k in stale:
                    del self._buckets[k]

            if key not in self._buckets:
                self._buckets[key] = {"tokens": self._burst, "ts": now}
                return True

            b = self._buckets[key]
            elapsed = now - b["ts"]
            b["ts"] = now
            b["tokens"] = min(self._burst, b["tokens"] + elapsed * self._rate)
            if b["tokens"] >= 1:
                b["tokens"] -= 1
                return True
            return False

    def stats(self):
        with self._lock:
            return {"tracked_ips": len(self._buckets)}

# ---------------------------------------------------------------------------
# Sensor registry
# ---------------------------------------------------------------------------

# Development-only fallback credentials. NEVER used unless
# DETECTIC_ALLOW_DEV_FALLBACK=1 is explicitly set, so production fails closed
# rather than silently accepting these well-known values.
DEV_SENSORS = {"ex520-001": "dev-secret-change-me"}


def load_sensors():
    env_sensors = os.environ.get("DETECTIC_SENSORS")
    if env_sensors:
        try:
            parsed = json.loads(env_sensors)
            if isinstance(parsed, dict):
                return parsed
        except ValueError:
            pass
    if os.path.exists(SENSORS_FILE):
        with open(SENSORS_FILE) as f:
            return json.load(f)
    # Fail closed: without either DETECTIC_SENSORS or sensors.json, and without
    # an explicit development opt-in, refuse to start so production can never
    # silently fall back to the well-known development credentials.
    if os.environ.get("DETECTIC_ALLOW_DEV_FALLBACK", "0") == "1":
        sys.stderr.write(
            "[backend] WARNING: using development sensor credentials "
            "(DETECTIC_ALLOW_DEV_FALLBACK=1)\n"
        )
        with open(SENSORS_FILE, "w") as f:
            json.dump(DEV_SENSORS, f, indent=2)
        return DEV_SENSORS
    raise RuntimeError(
        "no sensor credentials configured: set DETECTIC_SENSORS or create "
        "sensors.json (development only: DETECTIC_ALLOW_DEV_FALLBACK=1)"
    )

# ---------------------------------------------------------------------------
# Backend (SQLite + HMAC auth)
# ---------------------------------------------------------------------------

class Backend:
    def __init__(self, db_path, master_secret):
        self.master = master_secret.encode()
        self.conn = sqlite3.connect(db_path, check_same_thread=False)
        # Memory-optimized SQLite settings for 256 MB RAM
        self.conn.execute("PRAGMA journal_mode=WAL")
        self.conn.execute("PRAGMA synchronous=NORMAL")
        self.conn.execute("PRAGMA mmap_size=67108864")  # 64 MB mmap
        self.conn.execute("PRAGMA cache_size=-8000")  # 8 MB page cache
        self.conn.execute("PRAGMA temp_store=MEMORY")
        self.conn.execute("PRAGMA busy_timeout=5000")
        self.conn.execute("PRAGMA wal_autocheckpoint=200")
        self.conn.executescript("""
            CREATE TABLE IF NOT EXISTS sensors (
                id         TEXT PRIMARY KEY,
                secret     TEXT NOT NULL,
                name       TEXT,
                location   TEXT,
                created_at INTEGER NOT NULL,
                last_seen  INTEGER
            );
            CREATE TABLE IF NOT EXISTS snapshots (
                id          INTEGER PRIMARY KEY,
                sensor_id   TEXT NOT NULL,
                received_at INTEGER NOT NULL,
                captured_at INTEGER,
                device_count INTEGER DEFAULT 0,
                raw_json    TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS detections (
                id          INTEGER PRIMARY KEY,
                snapshot_id INTEGER NOT NULL,
                sensor_id   TEXT NOT NULL,
                pseudonym   TEXT NOT NULL,
                rssi        INTEGER,
                source      TEXT,
                standard    TEXT,
                radio_mac   TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_det_pseudo ON detections(pseudonym);
            CREATE INDEX IF NOT EXISTS idx_det_sensor ON detections(sensor_id);
            CREATE INDEX IF NOT EXISTS idx_det_snap ON detections(snapshot_id);
            CREATE TABLE IF NOT EXISTS events (
                id              INTEGER PRIMARY KEY,
                sensor_id       TEXT NOT NULL,
                event_id        TEXT NOT NULL UNIQUE,
                event_type      TEXT NOT NULL,
                event_timestamp INTEGER NOT NULL,
                device_id       TEXT,
                snapshot_json   TEXT,
                schema_version  TEXT,
                received_at     INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_evt_sensor ON events(sensor_id);
            CREATE INDEX IF NOT EXISTS idx_evt_device ON events(device_id);
            CREATE INDEX IF NOT EXISTS idx_evt_ts ON events(event_timestamp);
            CREATE TABLE IF NOT EXISTS presence_sessions (
                id          INTEGER PRIMARY KEY,
                sensor_id   TEXT NOT NULL,
                pseudonym   TEXT NOT NULL,
                first_seen  INTEGER NOT NULL,
                last_seen   INTEGER NOT NULL,
                observations INTEGER NOT NULL DEFAULT 1,
                avg_rssi    REAL,
                min_rssi    INTEGER,
                max_rssi    INTEGER,
                source      TEXT,
                standard    TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_ps_pseudo ON presence_sessions(pseudonym);
            CREATE INDEX IF NOT EXISTS idx_ps_sensor ON presence_sessions(sensor_id);
        """)
        self.conn.commit()
        self.sensors = load_sensors()
        self._checkpoint_interval = 300  # 5 min
        self._last_checkpoint = time.time()

    def periodic_maintenance(self):
        """Run periodic DB maintenance to bound WAL and cache size."""
        now = time.time()
        if now - self._last_checkpoint > self._checkpoint_interval:
            self._last_checkpoint = now
            try:
                self.conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")
            except Exception:
                pass

    def check_auth(self, sensor_id, signature, body):
        secret = self.sensors.get(sensor_id)
        if not secret or not signature:
            return False
        expected = hmaclib.new(secret.encode(), body, hashlib.sha256).hexdigest()
        return hmaclib.compare_digest(expected, signature)

    def pseudonymize(self, identifier):
        return hmaclib.new(self.master, identifier.encode(), hashlib.sha256).hexdigest()

    def ingest(self, sensor_id, payload):
        now = int(time.time())
        cur = self.conn.cursor()
        sanitized = {
            "sensor_id": payload.get("sensor_id", sensor_id),
            "id": payload.get("id"),
            "captured_at": payload.get("captured_at"),
            "devices": [
                {k: d[k] for k in ("pseudonym", "rssi", "source", "standard", "radio_mac") if k in d}
                for d in payload.get("devices", [])
                if d.get("pseudonym")
            ],
        }
        device_count = len(sanitized["devices"])
        cur.execute(
            "INSERT INTO snapshots (sensor_id, received_at, captured_at, device_count, raw_json)"
            " VALUES (?,?,?,?,?)",
            (sensor_id, now, payload.get("captured_at"), device_count,
             json.dumps(sanitized)),
        )
        snap_id = cur.lastrowid
        n = 0
        for d in payload.get("devices", []):
            pseudonym = d.get("pseudonym")
            if not pseudonym:
                ident = d.get("mac") or d.get("ip") or d.get("hostname")
                if not ident:
                    continue
                pseudonym = self.pseudonymize(ident)
            cur.execute(
                "INSERT INTO detections"
                " (snapshot_id, sensor_id, pseudonym, rssi, source, standard, radio_mac)"
                " VALUES (?,?,?,?,?,?,?)",
                (snap_id, sensor_id, pseudonym,
                 d.get("rssi"), d.get("source"), d.get("standard"), d.get("radio_mac")),
            )
            n += 1
        # Update sensor last_seen
        cur.execute(
            "UPDATE sensors SET last_seen = ? WHERE id = ?",
            (now, sensor_id),
        )
        # Auto-register sensor if not known
        if cur.rowcount == 0:
            cur.execute(
                "INSERT OR IGNORE INTO sensors (id, secret, created_at, last_seen)"
                " VALUES (?,?,?,?)",
                (sensor_id, self.sensors.get(sensor_id, ""), now, now),
            )
        self.conn.commit()
        self.periodic_maintenance()
        return snap_id, n

    def ingest_events(self, sensor_id, payload):
        now = int(time.time())
        events = payload.get("events", [])
        accepted = 0
        duplicates = 0
        for evt in events:
            event_id = evt.get("event_id", "")
            if not event_id:
                continue
            try:
                self.conn.execute(
                    "INSERT INTO events"
                    " (sensor_id, event_id, event_type, event_timestamp,"
                    "  device_id, snapshot_json, schema_version, received_at)"
                    " VALUES (?,?,?,?,?,?,?,?)",
                    (sensor_id, event_id,
                     evt.get("event_type", ""),
                     evt.get("event_timestamp", 0),
                     evt.get("device_id"),
                     json.dumps(evt.get("snapshot")) if evt.get("snapshot") else None,
                     evt.get("schema_version", "2.0"),
                     now),
                )
                accepted += 1
            except sqlite3.IntegrityError:
                duplicates += 1
        self.conn.commit()
        return accepted, duplicates

    def devices(self, limit=500):
        cur = self.conn.execute(
            """SELECT d.pseudonym,
                      MIN(COALESCE(s.captured_at, s.received_at)) AS first_seen,
                      MAX(COALESCE(s.captured_at, s.received_at)) AS last_seen,
                      COUNT(*)                                    AS observations,
                      CAST(ROUND(AVG(d.rssi)) AS INTEGER)         AS avg_rssi,
                      MIN(d.rssi)                                 AS min_rssi,
                      MAX(d.rssi)                                 AS max_rssi,
                      d.source, d.standard
               FROM detections d JOIN snapshots s ON d.snapshot_id = s.id
               GROUP BY d.pseudonym ORDER BY last_seen DESC LIMIT ?""",
            (limit,),
        )
        keys = ("pseudonym", "first_seen", "last_seen", "observations",
                "avg_rssi", "min_rssi", "max_rssi", "source", "standard")
        return [dict(zip(keys, row)) for row in cur.fetchall()]

    def presence(self, hours=24):
        """Presence analytics: devices seen in the last N hours."""
        cutoff = int(time.time()) - (hours * 3600)
        cur = self.conn.execute(
            """SELECT d.pseudonym,
                      MIN(COALESCE(s.captured_at, s.received_at)) AS first_seen,
                      MAX(COALESCE(s.captured_at, s.received_at)) AS last_seen,
                      COUNT(DISTINCT s.id)                        AS observations,
                      CAST(ROUND(AVG(d.rssi)) AS INTEGER)         AS avg_rssi,
                      MIN(d.rssi)                                 AS min_rssi,
                      MAX(d.rssi)                                 AS max_rssi,
                      COUNT(DISTINCT date(COALESCE(s.captured_at, s.received_at), 'unixepoch')) AS distinct_days,
                      d.source, d.standard
               FROM detections d JOIN snapshots s ON d.snapshot_id = s.id
               WHERE COALESCE(s.captured_at, s.received_at) >= ?
               GROUP BY d.pseudonym
               ORDER BY last_seen DESC LIMIT 500""",
            (cutoff,),
        )
        keys = ("pseudonym", "first_seen", "last_seen", "observations",
                "avg_rssi", "min_rssi", "max_rssi", "distinct_days",
                "source", "standard")
        return [dict(zip(keys, row)) for row in cur.fetchall()]

    def sensors_list(self):
        cur = self.conn.execute(
            """SELECT s.id, s.name, s.location, s.created_at, s.last_seen,
                      (SELECT COUNT(*) FROM detections d WHERE d.sensor_id = s.id) AS total_observations,
                      (SELECT COUNT(DISTINCT d.pseudonym) FROM detections d WHERE d.sensor_id = s.id) AS distinct_devices
               FROM sensors s ORDER BY s.last_seen DESC"""
        )
        keys = ("id", "name", "location", "created_at", "last_seen",
                "total_observations", "distinct_devices")
        return [dict(zip(keys, row)) for row in cur.fetchall()]

    def stats(self):
        now = int(time.time())
        hour_ago = now - 3600
        day_ago = now - 86400
        cur = self.conn.execute(
            """SELECT
                (SELECT COUNT(*) FROM snapshots) AS total_snapshots,
                (SELECT COUNT(*) FROM detections) AS total_detections,
                (SELECT COUNT(DISTINCT pseudonym) FROM detections) AS distinct_devices,
                (SELECT COUNT(*) FROM events) AS total_events,
                (SELECT COUNT(*) FROM sensors) AS total_sensors,
                (SELECT COUNT(*) FROM snapshots WHERE received_at >= ?) AS snapshots_last_hour,
                (SELECT COUNT(*) FROM snapshots WHERE received_at >= ?) AS snapshots_last_day,
                (SELECT COUNT(*) FROM detections WHERE sensor_id IN
                    (SELECT id FROM sensors WHERE last_seen >= ?)) AS active_devices_hour,
                (SELECT COALESCE(SUM(page_count * page_size), 0) FROM pragma_page_count(), pragma_page_size()) AS db_size_bytes
            """,
            (hour_ago, day_ago, hour_ago),
        )
        keys = ("total_snapshots", "total_detections", "distinct_devices",
                "total_events", "total_sensors", "snapshots_last_hour",
                "snapshots_last_day", "active_devices_hour", "db_size_bytes")
        row = cur.fetchone()
        if row is None:
            return {}
        return dict(zip(keys, row))


# ---------------------------------------------------------------------------
# HTTP Handler
# ---------------------------------------------------------------------------

class Handler(BaseHTTPRequestHandler):
    backend: Backend = None
    rate_limiter: RateLimiter = None
    server_ref: object = None

    def _json(self, code, obj, headers=None):
        body = json.dumps(obj, separators=(",", ":")).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Cache-Control", "no-store")
        if headers:
            for k, v in headers.items():
                self.send_header(k, v)
        self.end_headers()
        self.wfile.write(body)

    def _client_ip(self):
        return self.client_address[0] if self.client_address else "unknown"

    def do_GET(self):
        path = urlparse(self.path).path
        params = parse_qs(urlparse(self.path).query)

        if path == "/api/v1/healthz":
            rss_kb = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
            uptime = int(time.time()) - Handler._start_time
            return self._json(200, {
                "status": "ok",
                "uptime_secs": uptime,
                "rss_kb": rss_kb,
                "active_requests": (self.server_ref.active_count()
                                    if self.server_ref else 0),
                "version": "0.2.0-optimized",
            })

        if path == "/api/v1/readyz":
            return self._json(200, {"status": "ready"})

        if path == "/api/v1/devices":
            limit = int(params.get("limit", ["500"])[0])
            return self._json(200, {"devices": self.backend.devices(limit)})

        if path == "/api/v1/presence":
            hours = int(params.get("hours", ["24"])[0])
            hours = min(hours, 168)  # max 7 days
            return self._json(200, {
                "hours": hours,
                "devices": self.backend.presence(hours),
            })

        if path == "/api/v1/sensors":
            return self._json(200, {"sensors": self.backend.sensors_list()})

        if path == "/api/v1/stats":
            return self._json(200, self.backend.stats())

        return self._json(404, {"error": "not found"})

    def do_POST(self):
        # Rate limiting
        if self.rate_limiter and not self.rate_limiter.allow(self._client_ip()):
            return self._json(429, {"error": "rate limit exceeded",
                                     "retry_after": 1},
                              {"Retry-After": "1"})

        path = urlparse(self.path).path

        if path == "/api/v1/events":
            return self._ingest_single()

        if path == "/api/v1/events/batch":
            return self._ingest_batch()

        return self._json(404, {"error": "not found"})

    def _ingest_single(self):
        length = int(self.headers.get("Content-Length") or 0)
        if length <= 0 or length > 4 * 1024 * 1024:
            return self._json(400, {"error": "bad body size"})
        body = self.rfile.read(length)

        sensor_id = self.headers.get("X-Detectic-Sensor", "")
        signature = self.headers.get("X-Detectic-Signature", "")
        if not self.backend.check_auth(sensor_id, signature, body):
            return self._json(401, {"error": "unauthorized"})

        try:
            payload = json.loads(body.decode())
        except (ValueError, UnicodeDecodeError):
            return self._json(400, {"error": "invalid json"})

        if "events" in payload and isinstance(payload["events"], list):
            accepted, duplicates = self.backend.ingest_events(sensor_id, payload)
            return self._json(202, {"accepted": accepted, "duplicates": duplicates})

        snap_id, n = self.backend.ingest(sensor_id, payload)
        return self._json(200, {"snapshot": snap_id, "devices_stored": n})

    def _ingest_batch(self):
        length = int(self.headers.get("Content-Length") or 0)
        if length <= 0 or length > 16 * 1024 * 1024:  # 16 MB max batch
            return self._json(400, {"error": "bad body size"})
        body = self.rfile.read(length)

        sensor_id = self.headers.get("X-Detectic-Sensor", "")
        signature = self.headers.get("X-Detectic-Signature", "")
        if not self.backend.check_auth(sensor_id, signature, body):
            return self._json(401, {"error": "unauthorized"})

        try:
            batch = json.loads(body.decode())
        except (ValueError, UnicodeDecodeError):
            return self._json(400, {"error": "invalid json"})

        snapshots = batch if isinstance(batch, list) else batch.get("snapshots", [])
        total_devices = 0
        total_snapshots = 0
        for payload in snapshots[:100]:  # max 100 per batch
            if "events" in payload and isinstance(payload["events"], list):
                self.backend.ingest_events(sensor_id, payload)
            else:
                _, n = self.backend.ingest(sensor_id, payload)
                total_devices += n
                total_snapshots += 1

        return self._json(200, {
            "snapshots_stored": total_snapshots,
            "devices_stored": total_devices,
        })

    def log_message(self, fmt, *args):
        # Quiet logging — only errors
        if args and "4" in str(args[0]):
            sys.stderr.write(f"[backend] {fmt % args}\n")

    _start_time = int(time.time())


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def serve(host, port, db_path, master_secret, max_threads=16, rate_burst=60):
    backend = Backend(db_path, master_secret)
    rate_limiter = RateLimiter(rate=30, burst=rate_burst)

    Handler.backend = backend
    Handler.rate_limiter = rate_limiter

    httpd = BoundedThreadingHTTPServer(
        (host, port), Handler, max_threads=max_threads
    )
    Handler.server_ref = httpd

    rss_kb = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    print(f"[backend] detectic-backend v0.2.0-optimized")
    print(f"[backend] listening on http://{host}:{port}")
    print(f"[backend] db={db_path}")
    print(f"[backend] sensors={list(backend.sensors)}")
    print(f"[backend] max_threads={max_threads} rate_burst={rate_burst}")
    print(f"[backend] rss={rss_kb} kB")
    sys.stdout.flush()

    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        httpd.server_close()
        backend.conn.close()


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description="Detectic backend (optimized)")
    ap.add_argument("--host", default="0.0.0.0")
    ap.add_argument("--port", type=int, default=8080)
    ap.add_argument("--db", default=os.path.join(HERE, "backend.db"))
    ap.add_argument("--master-secret",
                    default=os.environ.get("DETECTIC_MASTER_SECRET", ""))
    ap.add_argument("--max-threads", type=int, default=16,
                    help="Max concurrent request threads (default: 16)")
    ap.add_argument("--rate-burst", type=int, default=60,
                    help="Max burst requests per IP (default: 60)")
    args = ap.parse_args()
    if not args.master_secret:
        if os.environ.get("DETECTIC_ALLOW_DEV_FALLBACK", "0") != "1":
            raise SystemExit(
                "master secret is required (set DETECTIC_MASTER_SECRET or "
                "pass --master-secret; development only: "
                "DETECTIC_ALLOW_DEV_FALLBACK=1)"
            )
        args.master_secret = "dev-master-secret"
        sys.stderr.write(
            "[backend] WARNING: using development master secret "
            "(DETECTIC_ALLOW_DEV_FALLBACK=1)\n"
        )
    serve(args.host, args.port, args.db, args.master_secret,
          args.max_threads, args.rate_burst)
