#!/usr/bin/env python3
"""EX520 Detectic Edge Supervisor.

Replaces the previous ping-only watchdog with a state machine that monitors
router reachability, GTPR readiness, and sensor health.  It triggers a single
GTPR `so DEV2_LIFEMOTE_AGENT` after a sustained router DOWN->UP transition,
verifies the sensor reports back, and re-triggers with exponential backoff if
the sensor remains unhealthy.

Design constraints (from AGENTS.md):
  * No firmware modification.
  * No router reboots.
  * No public package URL.
  * No duplicate phoenix.sh instances.
  * No plaintext secrets in logs.
"""
import hashlib
import os
import re
import secrets
import subprocess
import sys
import time
from dataclasses import dataclass
from enum import Enum, auto
from pathlib import Path
from typing import Callable, Optional

DETECTIC = os.environ.get("DETECTIC_BIN", "detectic")
ROUTER_URL = os.environ.get(
    "EX520_URL",
    "http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]",
)
USER = os.environ.get("EX520_USER", "user")
PASSWORD = os.environ["DETECTIC_PASSWORD"]
PING6_TARGET = os.environ.get(
    "EX520_PING6",
    "fe80::3e6a:d2ff:fe5f:abc1%enp2s0",
)
PING6_IFACE = os.environ.get("EX520_PING6_IFACE", "enp2s0")
POLL_INTERVAL = int(os.environ.get("POLL_INTERVAL", "10"))
BOOTSTART_URL = os.environ.get(
    "BOOTSTART_URL",
    "http://192.168.0.27:8080/bootstart.sh",
)
DOWN_THRESHOLD = int(os.environ.get("DOWN_THRESHOLD", "30"))
PHOENIX_GRACE = int(os.environ.get("PHOENIX_GRACE", "45"))
HEALTH_TIMEOUT = int(os.environ.get("HEALTH_TIMEOUT", "120"))
PACKAGE_ROOT = os.environ.get("PACKAGE_ROOT", os.path.dirname(os.path.abspath(__file__)))
# Optional TCP health probe target.  The DETECTIC sensor currently does not
# expose a well-known health port, so this is a placeholder for a future sensor
# health endpoint.  When set, the supervisor will attempt to connect to this
# host:port as a secondary health signal.
HEALTH_TCP_HOST = os.environ.get("DETECTIC_HEALTH_TCP_HOST", "")
HEALTH_TCP_PORT = int(os.environ.get("DETECTIC_HEALTH_TCP_PORT", "8787"))

SECRET_RE = re.compile(
    r"(password|passwd|pwd|secret|token|api[_-]?key|auth|cookie|jsessionid|private)"
    r"[\"']?\s*[:=]\s*[\"']?[^\s&\"'<>]+",
    re.IGNORECASE,
)


class State(Enum):
    UNKNOWN = auto()
    ROUTER_DOWN = auto()
    ROUTER_UP = auto()
    GTPR_READY = auto()
    SENSOR_STARTING = auto()
    SENSOR_HEALTHY = auto()
    SENSOR_UNHEALTHY = auto()


@dataclass
class SupervisorState:
    state: State = State.UNKNOWN
    router_up: bool = False
    gtpr_ready: bool = False
    sensor_healthy: bool = False
    last_router_up: Optional[float] = None
    last_router_down: Optional[float] = None
    armed: bool = False
    last_trigger: Optional[float] = None
    last_health: Optional[float] = None
    backoff_level: int = 0
    degraded: bool = False
    sensor_log_mtime: float = 0.0


def redact(msg: str) -> str:
    """Mask credentials or secrets before logging."""
    out = SECRET_RE.sub(r"\1=***", msg)
    out = re.sub(
        r"(Password|password|token|secret|key)\"[^\"]+\"",
        r"\1\"***\"",
        out,
        flags=re.IGNORECASE,
    )
    return out


def log(msg: str) -> None:
    ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    safe = redact(msg)
    print(f"{ts} {safe}", flush=True)


def _run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    env = {**os.environ, "DETECTIC_PASSWORD": PASSWORD}
    return subprocess.run(cmd, env=env, **kwargs)


def ping_reachable() -> bool:
    try:
        ret = _run(
            ["ping6", "-c", "1", "-W", "2", PING6_TARGET],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=5,
        )
        return ret.returncode == 0
    except Exception:
        return False


def gtpr_query(oid: str = "DEV2_LIFEMOTE_AGENT") -> bool:
    try:
        ret = _run(
            [DETECTIC, "--url", ROUTER_URL, "--user", USER, "query", oid],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=20,
        )
        return ret.returncode == 0
    except Exception:
        return False


def trigger_bootstart() -> bool:
    payload = (
        '{"enable":"1","URL":"%s","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'
        % BOOTSTART_URL
    )
    try:
        ret = _run(
            [DETECTIC, "--url", ROUTER_URL, "--user", USER, "set", "DEV2_LIFEMOTE_AGENT", payload],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=30,
        )
        return ret.returncode == 0
    except Exception:
        return False


def tcp_probe(host: str, port: int, timeout: float = 2.0) -> bool:
    """Best-effort TCP connect probe.  Returns True if the port accepts a connection."""
    import socket

    try:
        with socket.create_connection((host, port), timeout=timeout):
            return True
    except Exception:
        return False


def _sensor_log_mtime() -> float:
    """Return the mtime of the local sensor_log file (uploaded by launcher)."""
    p = Path(PACKAGE_ROOT) / "sensor_log.txt"
    if p.exists():
        return p.stat().st_mtime
    return 0.0


def _latest_done_status() -> Optional[str]:
    """Parse the package server callback log for the most recent status."""
    p = Path(PACKAGE_ROOT) / "done_log.txt"
    if not p.exists():
        return None
    text = p.read_text(errors="replace")
    # Find the last line that looks like done?status=...
    match = None
    for line in text.splitlines():
        m = re.search(r"status=(\w+)", line)
        if m:
            match = m.group(1)
    return match


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


def check_package_integrity() -> bool:
    """Verify the local package files (checksums match current build)."""
    root = Path(PACKAGE_ROOT)
    files = ["detectic.aa", "detectic.ab"]
    try:
        for f in files:
            csum_file = root / f"{f}.sha256"
            data_file = root / f
            if not csum_file.exists() or not data_file.exists():
                return False
            expected = csum_file.read_text().strip().split()[0]
            got = sha256_file(str(data_file))
            if expected != got:
                log(f"package integrity mismatch {f}: expected={expected[:16]}... got={got[:16]}...")
                return False
        return True
    except Exception as e:
        log(f"package integrity check error: {e}")
        return False


@dataclass
class SupervisorConfig:
    poll_interval: int = POLL_INTERVAL
    down_threshold: int = DOWN_THRESHOLD
    phoenix_grace: int = PHOENIX_GRACE
    health_timeout: int = HEALTH_TIMEOUT
    max_backoff: int = 160
    min_boot_interval: int = 60


class EdgeSupervisor:
    def __init__(
        self,
        is_router_reachable: Callable[[], bool] = ping_reachable,
        is_gtpr_ready: Callable[[], bool] = gtpr_query,
        do_trigger: Callable[[], bool] = trigger_bootstart,
        tcp_probe_fn: Callable[[str, int, float], bool] = tcp_probe,
        config: SupervisorConfig = SupervisorConfig(),
        logger: Callable[[str], None] = log,
    ):
        self.cfg = config
        self._router = is_router_reachable
        self._gtpr = is_gtpr_ready
        self._trigger_func = do_trigger
        self._tcp = tcp_probe_fn
        self._log = logger
        self.state = SupervisorState()

    def _transition(self, new_state: State) -> None:
        if self.state.state != new_state:
            self._log(f"state: {self.state.state.name} -> {new_state.name}")
            self.state.state = new_state

    def _router_check(self) -> bool:
        # Prefer ping; fall back to GTPR query if ping fails but httpd is up.
        up = self._router() or self._gtpr()
        now = time.time()
        if up:
            self.state.router_up = True
            self.state.last_router_up = now
        else:
            self.state.router_up = False
            if self.state.last_router_down is None or self.state.router_up:
                self.state.last_router_down = now
        return up

    def _gtpr_ready(self) -> bool:
        ready = self._gtpr()
        self.state.gtpr_ready = ready
        return ready

    def _is_trigger_allowed(self) -> bool:
        now = time.time()
        # Never trigger more frequently than min_boot_interval (prevents
        # duplicate phoenix instances while a previous one is still starting).
        if self.state.last_trigger is not None:
            elapsed = now - self.state.last_trigger
            if elapsed < self.cfg.min_boot_interval:
                self._log(f"trigger skipped: last trigger {elapsed:.0f}s ago")
                return False
        return True

    def _trigger(self) -> bool:
        if not self._is_trigger_allowed():
            return False
        self._transition(State.SENSOR_STARTING)
        self._log("PHOENIX_TRIGGERED")
        self.state.last_trigger = time.time()
        ok = self._trigger_func()
        if ok:
            self._log("GTPR trigger SENT")
        else:
            self._log("GTPR trigger FAILED")
            self._transition(State.SENSOR_UNHEALTHY)
        return ok

    def _check_sensor_health(self) -> bool:
        """Return True if the sensor has reported recently or is responsive."""
        # Primary signal: bootstart/launcher uploaded a log/callback recently.
        mtime = _sensor_log_mtime()
        if mtime > self.state.sensor_log_mtime:
            self.state.sensor_log_mtime = mtime
            self.state.last_health = time.time()

        if self.state.last_trigger is None:
            # We have never triggered; sensor cannot be healthy yet.
            return False

        now = time.time()
        since_trigger = now - self.state.last_trigger

        # Wait at least a short startup window before expecting health.
        if since_trigger < self.cfg.phoenix_grace:
            return False

        # A callback/log newer than the trigger means the sensor has been active.
        if self.state.last_health is not None:
            since_health = now - self.state.last_health
            if since_health <= self.cfg.health_timeout:
                return True

        # Secondary signal: TCP 8787 reachable on the router (only if the
        # sensor exposes it).  Disabled by default because the current DETECTIC
        # sensor does not expose this port.
        host = HEALTH_TCP_HOST
        if host:
            if self._tcp(host, HEALTH_TCP_PORT, timeout=1.0):
                self._log(f"TCP health probe {host}:{HEALTH_TCP_PORT} OK")
                self.state.last_health = now
                return True

        return False

    def _next_backoff(self) -> int:
        base = min(10 * (2 ** self.state.backoff_level), self.cfg.max_backoff)
        # Add small jitter to avoid synchronized retries.
        jitter = secrets.randbelow(1000) / 1000.0
        return int(base + jitter)

    def _recover(self) -> None:
        if not self._router_check():
            return
        if not self._gtpr_ready():
            self._transition(State.ROUTER_UP)
            return

        self._transition(State.SENSOR_UNHEALTHY)
        self._log("RECOVERY_TRIGGERED")
        wait = self._next_backoff()
        self._log(f"recovery backoff {wait}s (level {self.state.backoff_level})")
        time.sleep(wait)

        if self._trigger():
            self.state.backoff_level = 0
        else:
            self.state.backoff_level = min(self.state.backoff_level + 1, 4)
            self._log("RECOVERY_FAILED")
            if self.state.backoff_level >= 4:
                self.state.degraded = True
                self._log("DETECTIC_DEGRADED")

    def tick(self) -> None:
        """Single state machine step.  Caller handles sleep."""
        up = self._router_check()

        if not up:
            # Track sustained downtime.
            now = time.time()
            if self.state.last_router_down is None:
                self.state.last_router_down = now
            down_for = now - self.state.last_router_down
            self._transition(State.ROUTER_DOWN)
            if down_for >= self.cfg.down_threshold:
                if not self.state.armed:
                    self.state.armed = True
                    self._log(f"ROUTER_DOWN armed ({down_for:.0f}s)")
            else:
                self._log(f"ROUTER_DOWN ({down_for:.0f}s)")
            self.state.sensor_healthy = False
            return

        # Router is reachable.
        if self.state.state == State.ROUTER_DOWN or self.state.state == State.UNKNOWN:
            self._log("ROUTER_UP")

        if not self._gtpr_ready():
            self._transition(State.ROUTER_UP)
            return

        self._transition(State.GTPR_READY)

        # On cold boot (armed = we were down long enough), trigger Phoenix.
        if self.state.armed:
            self.state.armed = False
            self._log(f"waiting {self.cfg.phoenix_grace}s for phoenix")
            time.sleep(self.cfg.phoenix_grace)
            self._trigger()
            return

        # Normal monitoring: check sensor health and recover if needed.
        if self._check_sensor_health():
            if self.state.state != State.SENSOR_HEALTHY:
                self._transition(State.SENSOR_HEALTHY)
                self._log("SENSOR_HEALTHY")
            self.state.backoff_level = 0
            self.state.degraded = False
        else:
            if self.state.state == State.SENSOR_HEALTHY:
                self._log("SENSOR_UNHEALTHY")
            self._transition(State.SENSOR_UNHEALTHY)
            if self.state.last_trigger and (
                time.time() - self.state.last_trigger > self.cfg.health_timeout
            ):
                self._recover()

    def run(self) -> None:
        self._log(f"supervisor starting router={ROUTER_URL} bootstart={BOOTSTART_URL}")
        self._log(f"poll={self.cfg.poll_interval}s down_threshold={self.cfg.down_threshold}s")
        while True:
            self.tick()
            time.sleep(self.cfg.poll_interval)


def _ensure_single_instance() -> None:
    lock_file = Path(__file__).with_suffix(".pid")
    try:
        if lock_file.exists():
            old_pid = int(lock_file.read_text().strip())
            try:
                os.kill(old_pid, 0)
                log(f"supervisor already running (PID {old_pid}), exiting")
                sys.exit(0)
            except (OSError, ProcessLookupError):
                pass
    except (FileNotFoundError, ValueError):
        pass
    lock_file.write_text(str(os.getpid()))


def main() -> None:
    _ensure_single_instance()
    try:
        EdgeSupervisor().run()
    finally:
        try:
            Path(__file__).with_suffix(".pid").unlink()
        except OSError:
            pass


if __name__ == "__main__":
    main()
