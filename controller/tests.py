#!/usr/bin/env python3
"""
Phase 12E test suite: happy path, failure injection, rollback, idempotency, security.
"""

import sys
sys.path.insert(0, '/home/soporte24hwww/Documentos/Repositorios/detectic/controller')

from simulator import EX520Simulator
from controller import Controller, DetecticArtifact, HealthState, DeploymentState

def test_happy_path():
    sim = EX520Simulator(misc_rw_capacity_mb=32)
    ctrl = Controller(sim)
    artifact = DetecticArtifact(version="v2", sha256="a"*64, arch="aarch64", size=1024)
    data = b"v2_binary_" + b"x"*1014
    # Correct sha
    import hashlib
    artifact.sha256 = hashlib.sha256(data).hexdigest()
    result = ctrl.deployer.deploy(artifact, data)
    assert result, "Happy path deploy failed"
    assert ctrl.supervisor.health() == HealthState.HEALTHY
    print("PASS happy_path")

def test_rollback_on_bad_checksum():
    sim = EX520Simulator()
    ctrl = Controller(sim)
    artifact = DetecticArtifact(version="bad", sha256="0"*64, arch="aarch64", size=1024)
    data = b"bad_binary_" + b"x"*1012
    result = ctrl.deployer.deploy(artifact, data)
    assert not result, "Should fail checksum"
    assert ctrl.deployer.state.deployment_state == DeploymentState.ROLLBACK_REQUIRED.value or ctrl.deployer.state.deployment_state == DeploymentState.ROLLED_BACK.value
    print("PASS rollback_on_bad_checksum")

def test_reboot_recovery():
    sim = EX520Simulator()
    ctrl = Controller(sim)
    data = b"v1_binary_" * 64
    artifact = DetecticArtifact(version="v1", sha256="", arch="aarch64", size=len(data))
    import hashlib
    artifact.sha256 = hashlib.sha256(data).hexdigest()
    ctrl.deployer.deploy(artifact, data)
    sim.reboot()
    # Controller should still see binary persisted
    assert sim.get_active_version().startswith("v1")
    # Process dead after reboot
    assert ctrl.supervisor.health() == HealthState.DEAD
    # Restart
    sim.start_detectic("v1")
    assert ctrl.supervisor.health() == HealthState.HEALTHY
    print("PASS reboot_recovery")

def test_idempotency():
    sim = EX520Simulator()
    ctrl = Controller(sim)
    data = b"v1_binary_" * 64
    artifact = DetecticArtifact(version="v1", sha256="", arch="aarch64", size=len(data))
    import hashlib
    artifact.sha256 = hashlib.sha256(data).hexdigest()
    r1 = ctrl.deployer.deploy(artifact, data)
    # Second deploy should still be safe, not crash
    r2 = ctrl.deployer.deploy(artifact, data)
    assert r1
    # Accept result, just ensure health remains
    assert ctrl.supervisor.health() in (HealthState.HEALTHY, HealthState.DEAD)
    print("PASS idempotency")

def test_storage_insufficient():
    sim = EX520Simulator(misc_rw_capacity_mb=1)
    ctrl = Controller(sim)
    # Fill up
    sim.fs["/var/run/misc/misc_rw/big"] = type('F', (), {'data': b"x"* 900*1024, 'persistent': True})()
    artifact = DetecticArtifact(version="vbig", sha256="a"*64, arch="aarch64", size=2*1024*1024)
    data = b"x"*2*1024*1024
    import hashlib
    artifact.sha256 = hashlib.sha256(data).hexdigest()
    result = ctrl.deployer.deploy(artifact, data)
    assert not result
    print("PASS storage_insufficient")

def test_process_hung():
    sim = EX520Simulator()
    ctrl = Controller(sim)
    data = b"v1_binary_" * 64
    artifact = DetecticArtifact(version="v1", sha256="", arch="aarch64", size=len(data))
    import hashlib
    artifact.sha256 = hashlib.sha256(data).hexdigest()
    ctrl.deployer.deploy(artifact, data)
    sim.set_process_hung(True)
    assert ctrl.supervisor.health() == HealthState.HUNG
    print("PASS process_hung")

if __name__ == "__main__":
    test_happy_path()
    test_rollback_on_bad_checksum()
    test_reboot_recovery()
    test_idempotency()
    test_storage_insufficient()
    test_process_hung()
    print("ALL TESTS PASSED")
