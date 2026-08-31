"""Sensor health metric collector.

Collects CPU, memory, uptime, load, network and optional Wi-Fi client/AP counts
from a Linux/BusyBox sensor and posts them to the Detectic Worker.

This is intentionally read-only: it only reads existing /proc and command
outputs, never modifying system state.
"""

import hashlib
import hmac
import json
import os
import re
import subprocess
import time
from typing import Any, Dict, List, Optional


def _read_proc(path: str) -> str:
    try:
        with open(path, "r") as f:
            return f.read()
    except OSError:
        return ""


def _run(cmd: List[str]) -> str:
    try:
        return subprocess.check_output(cmd, text=True, stderr=subprocess.DEVNULL, timeout=10)
    except (subprocess.CalledProcessError, FileNotFoundError, subprocess.TimeoutExpired):
        return ""


def read_uptime() -> Optional[int]:
    txt = _read_proc("/proc/uptime")
    if not txt:
        return None
    try:
        return int(float(txt.split()[0]))
    except (ValueError, IndexError):
        return None


def read_load() -> Dict[str, Optional[float]]:
    txt = _read_proc("/proc/loadavg")
    if txt:
        parts = txt.split()
        if len(parts) >= 3:
            try:
                return {
                    "load_1m": float(parts[0]),
                    "load_5m": float(parts[1]),
                    "load_15m": float(parts[2]),
                }
            except ValueError:
                pass
    return {"load_1m": None, "load_5m": None, "load_15m": None}


def read_memory() -> Dict[str, Optional[float]]:
    txt = _read_proc("/proc/meminfo")
    if not txt:
        return {"memory_total_mb": None, "memory_used_mb": None, "memory_percent": None}

    total = used = None
    for line in txt.splitlines():
        if line.startswith("MemTotal:"):
            try:
                total = float(line.split()[1]) / 1024.0  # kB -> MB
            except (ValueError, IndexError):
                pass
        elif line.startswith("MemAvailable:"):
            try:
                available = float(line.split()[1]) / 1024.0
                if total is not None:
                    used = total - available
            except (ValueError, IndexError):
                pass

    memory_percent = None
    if total is not None and used is not None and total > 0:
        memory_percent = round((used / total) * 100, 1)

    return {
        "memory_total_mb": round(total, 1) if total is not None else None,
        "memory_used_mb": round(used, 1) if used is not None else None,
        "memory_percent": memory_percent,
    }


def _cpu_times() -> Dict[str, List[int]]:
    txt = _read_proc("/proc/stat")
    times: Dict[str, List[int]] = {}
    for line in txt.splitlines():
        if line.startswith("cpu"):
            parts = line.split()
            times[parts[0]] = [int(x) for x in parts[1:] if x.isdigit()]
    return times


def read_cpu_percent(sample_seconds: float = 1.0) -> Optional[float]:
    t1 = _cpu_times()
    time.sleep(sample_seconds)
    t2 = _cpu_times()
    if not t1 or not t2 or "cpu" not in t1 or "cpu" not in t2:
        return None

    def total(lst: List[int]) -> int:
        return sum(lst)

    def idle(lst: List[int]) -> int:
        # idle + iowait
        return (lst[3] if len(lst) > 3 else 0) + (lst[4] if len(lst) > 4 else 0)

    total_diff = total(t2["cpu"]) - total(t1["cpu"])
    idle_diff = idle(t2["cpu"]) - idle(t1["cpu"])
    if total_diff <= 0:
        return None
    return round((1 - idle_diff / total_diff) * 100, 1)


def _parse_bytes(value: str, unit: str) -> Optional[float]:
    try:
        v = float(value.replace(",", ""))
    except ValueError:
        return None
    unit = unit.lower()
    if unit == "b":
        return v
    if unit == "kb" or unit == "kib":
        return v * 1024
    if unit == "mb" or unit == "mib":
        return v * 1024 * 1024
    if unit == "gb" or unit == "gib":
        return v * 1024 * 1024 * 1024
    return v


def read_network_bytes() -> Dict[str, Optional[float]]:
    txt = _run(["ip", "-s", "link"])
    if not txt:
        return {"network_rx_mb": None, "network_tx_mb": None}

    rx_total = 0.0
    tx_total = 0.0
    found = False
    lines = txt.splitlines()
    i = 0
    while i < len(lines):
        line = lines[i]
        if re.match(r"^\s+[0-9]+:\s+", line) and "lo" not in line:
            # next lines after interface name contain RX/TX stats
            i += 1
            while i < len(lines) and not re.match(r"^\s+[0-9]+:\s+", lines[i]):
                rx_match = re.match(r"^\s*RX:\s+bytes:\s+(\S+)\s+\S+\s+\S+\s+\S+\s+\S+", lines[i])
                if rx_match and i + 1 < len(lines):
                    tx_match = re.match(r"^\s*TX:\s+bytes:\s+(\S+)\s+\S+\s+\S+\s+\S+\s+\S+", lines[i + 1])
                    if rx_match and tx_match:
                        rx = _parse_bytes(rx_match.group(1), "B")
                        tx = _parse_bytes(tx_match.group(1), "B")
                        if rx is not None and tx is not None:
                            rx_total += rx
                            tx_total += tx
                            found = True
                        i += 1
                i += 1
        else:
            i += 1

    if not found:
        return {"network_rx_mb": None, "network_tx_mb": None}
    return {
        "network_rx_mb": round(rx_total / (1024 * 1024), 1),
        "network_tx_mb": round(tx_total / (1024 * 1024), 1),
    }


def read_disk_usage() -> Optional[float]:
    txt = _run(["df", "/"])
    if not txt:
        return None
    for line in txt.splitlines():
        parts = line.split()
        if len(parts) >= 6 and parts[0].startswith("/"):
            try:
                return float(parts[4].rstrip("%"))
            except ValueError:
                continue
    return None


def read_temperature() -> Optional[float]:
    # Try common thermal zone paths
    for path in ["/sys/class/thermal/thermal_zone0/temp", "/sys/class/hwmon/hwmon0/temp1_input"]:
        try:
            with open(path, "r") as f:
                val = int(f.read().strip())
                return round(val / 1000.0, 1)
        except (OSError, ValueError):
            continue
    return None


def read_wifi_clients() -> Dict[str, Optional[int]]:
    # Best effort: parse `iw dev <iface> station dump` or `wlanconfig`
    out = _run(["iw", "dev", "wlan0", "station", "dump"])
    if out:
        clients = sum(1 for line in out.splitlines() if line.startswith("Station"))
        return {"wifi_clients": clients if clients > 0 else None, "wifi_aps": None}
    # Fallback for devices with `iwinfo` or `wlanconfig`
    out = _run(["iwinfo"])
    if out:
        # Look for ESSID / clients count
        clients = 0
        for m in re.finditer(r"(\d+)\s+clients?", out, re.IGNORECASE):
            clients += int(m.group(1))
        return {"wifi_clients": clients if clients > 0 else None, "wifi_aps": None}
    return {"wifi_clients": None, "wifi_aps": None}


def collect_health() -> Dict[str, Any]:
    reported_at = int(time.time())
    metrics: Dict[str, Any] = {
        "reported_at": reported_at,
        "uptime_seconds": read_uptime(),
        **read_load(),
        **read_memory(),
        **read_network_bytes(),
        **read_wifi_clients(),
        "disk_used_percent": read_disk_usage(),
        "temperature_c": read_temperature(),
    }
    metrics["cpu_percent"] = read_cpu_percent(sample_seconds=0.5)
    # Drop None values to keep the payload small
    return {k: v for k, v in metrics.items() if v is not None}


def post_health(url: str, sensor_id: str, secret: bytes, metrics: Dict[str, Any]) -> bool:
    body = json.dumps(metrics, separators=(",", ":"))
    ts = str(int(time.time()))
    signed = ts.encode() + b"\n" + body.encode()
    sig = hmac.new(secret, signed, hashlib.sha256).hexdigest()

    try:
        import urllib.request
        req = urllib.request.Request(
            f"{url.rstrip('/')}/api/v1/health",
            data=body.encode(),
            headers={
                "Content-Type": "application/json",
                "X-Detectic-Sensor": sensor_id,
                "X-Detectic-Signature": sig,
                "X-Detectic-Timestamp": ts,
                "User-Agent": "detectic-health/1.0",
            },
            method="POST",
        )
        with urllib.request.urlopen(req, timeout=15) as resp:
            return resp.status == 202
    except Exception as e:
        print(f"health post failed: {type(e).__name__}: {e}", file=__import__("sys").stderr)
        return False


if __name__ == "__main__":
    import sys
    url = os.environ.get("DETECTIC_URL", "http://127.0.0.1:8787")
    sensor = os.environ.get("DETECTIC_SENSOR_ID", "sensor-001")
    secret = os.environ.get("DETECTIC_SECRET", "").encode("utf-8")
    if not secret:
        print("DETECTIC_SECRET required", file=sys.stderr)
        sys.exit(1)
    metrics = collect_health()
    print(json.dumps(metrics, indent=2))
    if post_health(url, sensor, secret, metrics):
        print("health posted")
    else:
        print("health post failed", file=sys.stderr)
        sys.exit(1)
