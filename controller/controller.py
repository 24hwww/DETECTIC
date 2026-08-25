#!/usr/bin/env python3
"""
Phase 12E Controller implementation - offline simulation ready.

Architecture:
Controller
 ├── RouterDiscovery
 ├── ManagementTransport
 ├── FileTransfer
 ├── ArtifactManager
 ├── DeploymentManager
 ├── ProcessSupervisor
 ├── HealthManager
 ├── ResourceMonitor
 ├── QueueManager
 ├── StateStore
 ├── RecoveryManager
 └── Metrics
"""

import hashlib
import json
import os
import time
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Optional, Dict

class HealthState(str, Enum):
    DEAD = "DEAD"
    HUNG = "HUNG"
    STARTING = "STARTING"
    HEALTHY = "HEALTHY"
    DEGRADED = "DEGRADED"
    UNKNOWN = "UNKNOWN"

class DeploymentState(str, Enum):
    IDLE = "IDLE"
    PRECHECK = "PRECHECK"
    TRANSFERRING = "TRANSFERRING"
    TRANSFERRED = "TRANSFERRED"
    VERIFIED = "VERIFIED"
    SYNCED = "SYNCED"
    ACTIVATED = "ACTIVATED"
    STARTING = "STARTING"
    HEALTHCHECKING = "HEALTHCHECKING"
    COMMITTED = "COMMITTED"
    ROLLBACK_REQUIRED = "ROLLBACK_REQUIRED"
    ROLLED_BACK = "ROLLED_BACK"

@dataclass
class RouterIdentity:
    firmware: str = ""
    build: str = ""
    arch: str = "aarch64"
    ip: str = ""

@dataclass
class DetecticArtifact:
    version: str
    sha256: str
    arch: str
    size: int
    manifest: dict = field(default_factory=dict)

@dataclass
class ControllerState:
    desired_version: str = ""
    active_version: str = ""
    previous_version: str = ""
    deployment_state: str = DeploymentState.IDLE.value
    restart_counter: int = 0
    last_known_health: str = HealthState.UNKNOWN.value
    transaction_id: str = ""

class CommandAllowlist:
    ALLOWED = {
        "discovery": ["df","mount","ps","cat /proc/version","cat /proc/mtd"],
        "verification": ["sha256sum","ls","stat"],
        "deployment": ["mkdir","mv","chmod","cp"],
        "process": ["pidof","ps","kill"],
        "recovery": ["sync"]
    }
    @classmethod
    def allow(cls, category, cmd):
        return cmd in cls.ALLOWED.get(category, [])

class ArtifactManager:
    @staticmethod
    def verify(artifact: DetecticArtifact, data: bytes) -> bool:
        if len(data) != artifact.size:
            return False
        h = hashlib.sha256(data).hexdigest()
        return h == artifact.sha256 and artifact.arch == "aarch64"

class StateStore:
    def __init__(self, path: Path):
        self.path = path
        self.path.parent.mkdir(parents=True, exist_ok=True)

    def save(self, state: ControllerState):
        tmp = self.path.with_suffix(".tmp")
        with open(tmp, "w") as f:
            json.dump(state.__dict__, f)
        tmp.replace(self.path)

    def load(self) -> ControllerState:
        if not self.path.exists():
            return ControllerState()
        try:
            with open(self.path, "r") as f:
                d = json.load(f)
            return ControllerState(**d)
        except Exception:
            return ControllerState()

class ProcessSupervisor:
    def __init__(self, simulator):
        self.sim = simulator

    def discover_pid(self) -> Optional[int]:
        # Use simulator process table, not pidof
        pids = self.sim.get_detectic_pids()
        return pids[0] if pids else None

    def verify_executable(self, pid: int, expected_path: str) -> bool:
        exe = self.sim.proc_exe(pid)
        return exe == expected_path

    def health(self) -> HealthState:
        pids = self.sim.get_detectic_pids()
        if not pids:
            return HealthState.DEAD
        # Simplified: if process marked running -> HEALTHY
        if self.sim.process_hung():
            return HealthState.HUNG
        return HealthState.HEALTHY

class DeploymentManager:
    def __init__(self, simulator, state_store: StateStore):
        self.sim = simulator
        self.store = state_store
        self.state = state_store.load()

    def deploy(self, artifact: DetecticArtifact, data: bytes) -> bool:
        # Transaction steps
        self.state.deployment_state = DeploymentState.PRECHECK.value
        self.store.save(self.state)

        # PRECHECK
        if self.sim.free_space() < artifact.size + 1024*1024:
            return False

        self.state.deployment_state = DeploymentState.TRANSFERRING.value
        self.store.save(self.state)
        # Simulate upload
        self.sim.upload_file("/var/run/misc/misc_rw/detectic/detectic.new", data)

        self.state.deployment_state = DeploymentState.VERIFIED.value
        self.store.save(self.state)
        if not ArtifactManager.verify(artifact, data):
            self.state.deployment_state = DeploymentState.ROLLBACK_REQUIRED.value
            self.store.save(self.state)
            return False

        self.state.deployment_state = DeploymentState.SYNCED.value
        self.store.save(self.state)
        self.sim.sync()

        self.state.deployment_state = DeploymentState.ACTIVATED.value
        # Atomic switch: keep previous
        prev = self.sim.get_current_binary()
        self.sim.save_previous(prev)
        self.sim.activate_binary("/var/run/misc/misc_rw/detectic/detectic.new")
        self.store.save(self.state)

        self.state.deployment_state = DeploymentState.STARTING.value
        self.store.save(self.state)
        self.sim.start_detectic(artifact.version)

        self.state.deployment_state = DeploymentState.HEALTHCHECKING.value
        self.store.save(self.state)
        time.sleep(0.1)
        supervisor = ProcessSupervisor(self.sim)
        health = supervisor.health()
        if health != HealthState.HEALTHY:
            # Rollback
            self.state.deployment_state = DeploymentState.ROLLBACK_REQUIRED.value
            self.store.save(self.state)
            self.sim.rollback()
            self.state.deployment_state = DeploymentState.ROLLED_BACK.value
            self.store.save(self.state)
            return False

        self.state.deployment_state = DeploymentState.COMMITTED.value
        self.state.active_version = artifact.version
        self.store.save(self.state)
        return True

class Controller:
    def __init__(self, simulator):
        self.sim = simulator
        self.state_store = StateStore(Path("/tmp/controller_state.json"))
        self.state = self.state_store.load()
        self.supervisor = ProcessSupervisor(simulator)
        self.deployer = DeploymentManager(simulator, self.state_store)

    def run_cycle(self):
        health = self.supervisor.health()
        self.state.last_known_health = health.value
        self.state_store.save(self.state)

    def recover(self):
        # Crash recovery: load state and reconcile
        self.state = self.state_store.load()
        # Reconcile by inspecting simulator
        actual_version = self.sim.get_active_version()
        if self.state.deployment_state == DeploymentState.HEALTHCHECKING.value:
            # Resume healthcheck
            pass

if __name__ == "__main__":
    print("Controller module loaded")
