"""Read-only ARP / IPv6 neighbor table reader.

Used when the sensor host has shell access (e.g. router with BusyBox or a Linux
host) to accelerate presence detection. No network packets are sent; only
existing kernel neighbor tables are read.
"""

import os
import re
import subprocess
from typing import Dict, List, Optional


class Neighbor:
    def __init__(self, ip: str, mac: Optional[str], source: str, interface: Optional[str] = None, state: Optional[str] = None):
        self.ip = ip
        self.mac = mac
        self.source = source  # 'arp' or 'ndp'
        self.interface = interface
        self.state = state

    def as_dict(self) -> Dict[str, Optional[str]]:
        return {
            "ip": self.ip,
            "mac": self.mac,
            "source": self.source,
            "interface": self.interface,
            "state": self.state,
        }


def _is_mac(addr: Optional[str]) -> bool:
    if not addr:
        return False
    return bool(re.fullmatch(r"([0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}", addr))


def read_arp_table() -> List[Neighbor]:
    """Parse /proc/net/arp or fallback to `ip neigh` for IPv4 entries."""
    out: List[Neighbor] = []
    proc_path = "/proc/net/arp"
    if os.path.exists(proc_path):
        try:
            with open(proc_path, "r") as f:
                next(f, None)  # skip header
                for line in f:
                    parts = line.split()
                    if len(parts) < 4:
                        continue
                    ip = parts[0]
                    hw_addr = parts[3]
                    if hw_addr == "00:00:00:00:00:00" or not _is_mac(hw_addr):
                        continue
                    out.append(Neighbor(ip, hw_addr, "arp"))
            return out
        except OSError:
            pass

    # Fallback to `ip -4 neigh show`
    try:
        text = subprocess.check_output(["ip", "-4", "neigh", "show"], text=True, stderr=subprocess.DEVNULL, timeout=10)
    except (subprocess.CalledProcessError, FileNotFoundError, subprocess.TimeoutExpired):
        return out

    for line in text.splitlines():
        m = re.match(r"^(\S+)\s+dev\s+(\S+)\s+lladdr\s+([0-9a-fA-F:]{17})\s+(.*)$", line)
        if not m:
            continue
        ip, iface, mac, rest = m.groups()
        if not _is_mac(mac):
            continue
        state = rest.split()[0] if rest.split() else None
        out.append(Neighbor(ip, mac, "arp", iface, state))
    return out


def read_ipv6_neigh() -> List[Neighbor]:
    """Parse `ip -6 neigh show` output."""
    out: List[Neighbor] = []
    try:
        text = subprocess.check_output(["ip", "-6", "neigh", "show"], text=True, stderr=subprocess.DEVNULL, timeout=10)
    except (subprocess.CalledProcessError, FileNotFoundError, subprocess.TimeoutExpired):
        return out

    for line in text.splitlines():
        # fe80::... dev enp2s0 lladdr 3c:6a:d2:5f:ab:c1 router STALE
        m = re.match(r"^(\S+)\s+dev\s+(\S+)\s+lladdr\s+([0-9a-fA-F:]{17})\s+(.*)$", line)
        if not m:
            # also accept without lladdr (incomplete)
            continue
        ip, iface, mac, rest = m.groups()
        if not _is_mac(mac):
            continue
        state = rest.split()[-1] if rest.split() else None
        out.append(Neighbor(ip, mac, "ndp", iface, state))
    return out


def read_all_neighbors() -> List[Neighbor]:
    return read_arp_table() + read_ipv6_neigh()


def neighbor_events(sensor_id: str, secret: bytes, neighbors: Optional[List[Neighbor]] = None) -> List[Dict[str, object]]:
    """Convert neighbors to canonical Detectic events keyed by MAC pseudonym."""
    import hmac
    import hashlib
    import time

    if neighbors is None:
        neighbors = read_all_neighbors()

    events: List[Dict[str, object]] = []
    ts = int(time.time())
    seq = 0
    for n in neighbors:
        if not n.mac:
            continue
        # Deterministic pseudonym for the neighbor (MAC-based)
        pseudo = hmac.new(secret, f"mac:{n.mac}".encode(), hashlib.sha256).hexdigest()[:24]
        event_id = f"{sensor_id}:{ts}:ip:{seq}:{pseudo}"
        events.append({
            "event_id": event_id,
            "event_type": "arp.announce" if n.source == "arp" else "ipv6.neighbor",
            "event_timestamp": ts,
            "device_id": pseudo,
            "timestamp": ts,
            "type": "arp.announce" if n.source == "arp" else "ipv6.neighbor",
            "sequence": seq,
            "payload": {
                "ip": n.ip,
                "mac": n.mac,
                "lladdr": n.mac,
                "interface": n.interface,
                "state": n.state,
                "source": n.source,
                "confidence": 0.9,
            },
            "schema_version": "3.0",
        })
        seq += 1
    return events


if __name__ == "__main__":
    for n in read_all_neighbors():
        print(n.as_dict())
