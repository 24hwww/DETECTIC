#!/usr/bin/env python3
"""M12.2 — Temporal observation collector for the real EX520.

Runs `detectic sensor --once` via IPv6 link-local every ~60 seconds,
stores pseudonymized observations locally, and computes basic statistics.

Usage:
    python3 tests/temporal_collect.py --duration 300   # 5 minutes
    python3 tests/temporal_collect.py --duration 3600  # 60 minutes
"""

import argparse
import hashlib
import hmac
import json
import os
import subprocess
import sys
import time
from datetime import datetime, timezone, timedelta

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(REPO, "target", "release", "detectic")
ROUTER_URL = "http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]"
ROUTER_USER = "user"
ROUTER_PASS = "<REDACTED>"
PSEUDONYM_SECRET = b"detectic-live-test-secret"
SENSOR_ID = "detectic-ex520-live"
DEFAULT_INTERVAL = 60
DEFAULT_DURATION = 300  # 5 minutes


def pseudonymize(identifier: str) -> str:
    return hmac.new(PSEUDONYM_SECRET, identifier.encode(), hashlib.sha256).hexdigest()


def collect_once():
    """Run detectic map once and return parsed JSON."""
    env = os.environ.copy()
    env["DETECTIC_PASSWORD"] = ROUTER_PASS
    env["DETECTIC_SECRET"] = "dummy"
    try:
        result = subprocess.run(
            [BIN, "--url", ROUTER_URL, "--user", ROUTER_USER, "map"],
            capture_output=True, text=True, timeout=30, env=env,
        )
        if result.returncode != 0:
            return None, result.stderr.strip()[:200]
        raw = result.stdout
        start = raw.find("{")
        if start < 0:
            return None, "no JSON in output"
        return json.loads(raw[start:]), None
    except subprocess.TimeoutExpired:
        return None, "timeout"
    except Exception as e:
        return None, str(e)[:200]


def extract_samples(data):
    """Extract pseudonymized per-device samples from a network map."""
    samples = []
    captured_at = data.get("captured_at", 0)
    devices = data.get("devices", [])

    for d in devices:
        mac = d.get("mac", "")
        if not mac:
            continue
        device_pseudo = pseudonymize(mac)[:16]

        radio_mac = d.get("radio_mac", "")
        radio_pseudo = pseudonymize(radio_mac)[:16] if radio_mac else ""

        standard = d.get("standard")
        if standard == "ac":
            band = "5GHz"
        elif standard in ("n", "ax"):
            band = "2.4GHz"
        else:
            band = "unknown"

        samples.append({
            "timestamp": captured_at,
            "sensor_id": SENSOR_ID,
            "device_pseudonym": device_pseudo,
            "hostname": d.get("hostname", "Unknown"),
            "active": d.get("active") == "1",
            "source": d.get("source", "unknown"),
            "band": band,
            "radio_pseudonym": radio_pseudo,
            "operating_standard": standard,
            "raw_signal_strength": d.get("rssi"),
            "signal_strength_level": d.get("signal_level"),
            "noise": d.get("noise"),
            "tx_rate_kbps": d.get("tx_rate"),
            "rx_rate_kbps": d.get("rx_rate"),
            "max_link_rate_kbps": d.get("max_link_rate"),
        })

    return samples


def compute_statistics(dataset):
    """Compute basic descriptive statistics per device/band."""
    from collections import defaultdict
    import statistics

    groups = defaultdict(list)
    for s in dataset:
        key = (s["device_pseudonym"], s["band"])
        groups[key].append(s)

    stats = []
    for (pseudo, band), samples in sorted(groups.items()):
        signals = [s["raw_signal_strength"] for s in samples if s["raw_signal_strength"] is not None]
        levels = [s["signal_strength_level"] for s in samples if s["signal_strength_level"] is not None]
        noises = [s["noise"] for s in samples if s["noise"] is not None]
        txs = [s["tx_rate_kbps"] for s in samples if s["tx_rate_kbps"] is not None]
        rxs = [s["rx_rate_kbps"] for s in samples if s["rx_rate_kbps"] is not None]
        active_count = sum(1 for s in samples if s["active"])

        stat = {
            "device_pseudonym": pseudo,
            "band": band,
            "hostname": samples[0].get("hostname", "?"),
            "sample_count": len(samples),
            "active_count": active_count,
            "inactive_count": len(samples) - active_count,
        }

        if signals:
            stat["signal_min"] = min(signals)
            stat["signal_max"] = max(signals)
            stat["signal_mean"] = round(statistics.mean(signals), 1)
            stat["signal_median"] = round(statistics.median(signals), 1)
            stat["signal_stddev"] = round(statistics.stdev(signals), 1) if len(signals) > 1 else 0

        if levels:
            stat["level_distribution"] = dict(sorted(
                [(str(l), levels.count(l)) for l in set(levels)]
            ))

        if noises:
            stat["noise_min"] = min(noises)
            stat["noise_max"] = max(noises)

        if txs:
            stat["tx_min"] = min(txs)
            stat["tx_max"] = max(txs)
        if rxs:
            stat["rx_min"] = min(rxs)
            stat["rx_max"] = max(rxs)

        stats.append(stat)

    return stats


def main():
    ap = argparse.ArgumentParser(description="M12.2 temporal observation collector")
    ap.add_argument("--duration", type=int, default=DEFAULT_DURATION,
                    help="Collection duration in seconds (default: 300)")
    ap.add_argument("--interval", type=int, default=DEFAULT_INTERVAL,
                    help="Seconds between samples (default: 60)")
    ap.add_argument("--output", default="tests/temporal_dataset.jsonl",
                    help="Output JSONL file")
    args = ap.parse_args()

    if not os.path.exists(BIN):
        print("[*] Building detectic binary...")
        subprocess.run(["cargo", "build", "--release"], cwd=REPO, check=True)

    output_path = os.path.join(REPO, args.output)
    os.makedirs(os.path.dirname(output_path), exist_ok=True)

    end_time = time.time() + args.duration
    sample_num = 0
    failed = 0
    dataset = []
    next_sample_time = time.time()

    print(f"[*] M12.2 temporal collection: {args.duration}s @ {args.interval}s interval")
    print(f"[*] Output: {output_path}")
    print(f"[*] Router: {ROUTER_URL}")
    print()

    while time.time() < end_time:
        sample_num += 1
        ts = datetime.now(timezone(timedelta(hours=-3))).strftime("%H:%M:%S")
        print(f"[{ts}] Sample #{sample_num}...", end=" ", flush=True)

        data, err = collect_once()
        if data is None:
            failed += 1
            print(f"FAILED: {err}")
        else:
            samples = extract_samples(data)
            for s in samples:
                dataset.append(s)
                with open(output_path, "a") as f:
                    f.write(json.dumps(s) + "\n")

            active = sum(1 for s in samples if s["active"])
            print(f"OK: {len(samples)} devices ({active} active)")

        # Sleep until next scheduled sample time
        next_sample_time += args.interval
        sleep_time = max(0, next_sample_time - time.time())
        if time.time() + sleep_time < end_time:
            time.sleep(sleep_time)
        else:
            # We're past or very close to end_time; stop sampling
            break

    # Compute statistics
    print(f"\n[*] Collection complete: {sample_num} samples, {failed} failed, {len(dataset)} records")
    print(f"[*] Computing statistics...")

    stats = compute_statistics(dataset)
    stats_path = output_path.replace(".jsonl", "_stats.json")
    with open(stats_path, "w") as f:
        json.dump(stats, f, indent=2)
    print(f"[*] Statistics written to: {stats_path}")

    # Print summary
    print(f"\n{'='*70}")
    print(f"M12.2 TEMPORAL COLLECTION SUMMARY")
    print(f"{'='*70}")
    print(f"Duration:     {args.duration}s")
    print(f"Interval:     {args.interval}s")
    print(f"Samples:      {sample_num}")
    print(f"Failed:       {failed}")
    print(f"Records:      {len(dataset)}")
    print(f"Unique dev/band: {len(stats)}")
    print(f"{'='*70}")

    for s in stats:
        sig = f"signal: {s.get('signal_min','?')}-{s.get('signal_max','?')} mean={s.get('signal_mean','?')}" if "signal_min" in s else "signal: N/A"
        print(f"  {s['device_pseudonym'][:12]} | {s['band']:<6} | {s['hostname']:<20} | samples={s['sample_count']} active={s['active_count']} | {sig}")

    print(f"{'='*70}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
