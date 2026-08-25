#!/usr/bin/env python3
"""
EX520 Simulator for Phase 12E offline validation.
"""

import hashlib
import copy
from dataclasses import dataclass, field
from typing import Dict, List, Optional

@dataclass
class SimulatedFile:
    data: bytes
    mode: str = "rw"
    persistent: bool = True

class EX520Simulator:
    def __init__(self, misc_rw_capacity_mb=32):
        self.misc_rw_capacity = misc_rw_capacity_mb * 1024 * 1024
        self.fs: Dict[str, SimulatedFile] = {}
        self.processes: Dict[int, dict] = {}
        self.next_pid = 1000
        self.reboot_count = 0
        self.network_up = True
        self.telnet_available = True
        self.active_binary_path = "/var/run/misc/misc_rw/detectic/detectic.current"
        self.previous_binary = None
        # Initial persistent file
        self.fs["/var/run/misc/misc_rw/detectic/detectic.current"] = SimulatedFile(b"v1_binary", "rw", True)
        
    def free_space(self) -> int:
        used = sum(len(f.data) for f in self.fs.values() if f.persistent)
        return max(0, self.misc_rw_capacity - used)

    def upload_file(self, path: str, data: bytes):
        self.fs[path] = SimulatedFile(data, "rw", True)

    def sync(self):
        pass

    def activate_binary(self, src_path: str):
        if src_path in self.fs:
            # Copy to active
            self.previous_binary = self.fs.get(self.active_binary_path)
            self.fs[self.active_binary_path] = SimulatedFile(self.fs[src_path].data, "rw", True)

    def save_previous(self, prev):
        if prev:
            self.fs["/var/run/misc/misc_rw/detectic/detectic.previous"] = SimulatedFile(prev.data, "rw", True)

    def get_current_binary(self):
        return self.fs.get(self.active_binary_path)

    def get_active_version(self) -> str:
        f = self.fs.get(self.active_binary_path)
        if not f:
            return ""
        # Version encoded in first 10 bytes
        return f.data[:10].decode(errors="ignore").strip("\x00")

    def start_detectic(self, version: str):
        # Kill existing
        self.kill_detectic()
        pid = self.next_pid
        self.next_pid += 1
        self.processes[pid] = {"executable": self.active_binary_path, "version": version, "hung": False}
        
    def kill_detectic(self):
        to_kill = [pid for pid, p in self.processes.items() if p["executable"] == self.active_binary_path]
        for pid in to_kill:
            del self.processes[pid]

    def get_detectic_pids(self) -> List[int]:
        return [pid for pid, p in self.processes.items() if p["executable"] == self.active_binary_path]

    def proc_exe(self, pid: int) -> Optional[str]:
        p = self.processes.get(pid)
        return p["executable"] if p else None

    def process_hung(self) -> bool:
        for p in self.processes.values():
            if p.get("hung"):
                return True
        return False

    def rollback(self):
        prev = self.fs.get("/var/run/misc/misc_rw/detectic/detectic.previous")
        if prev:
            self.fs[self.active_binary_path] = SimulatedFile(prev.data, "rw", True)
            self.kill_detectic()
            self.start_detectic(self.get_active_version())

    def reboot(self):
        self.reboot_count += 1
        # Non-persistent processes die
        self.processes.clear()
        # Persistent files remain
        # /tmp cleared
        keys_to_remove = [k for k in self.fs if not self.fs[k].persistent]
        for k in keys_to_remove:
            del self.fs[k]

    def set_network_down(self, down: bool):
        self.network_up = not down

    def corrupt_file(self, path: str):
        if path in self.fs:
            self.fs[path].data = b"corrupt"

    def set_process_hung(self, hung: bool):
        for p in self.processes.values():
            p["hung"] = hung

# Simple test harness
if __name__ == "__main__":
    sim = EX520Simulator()
    print("Sim free space:", sim.free_space())
    sim.reboot()
    print("After reboot processes:", sim.processes)
