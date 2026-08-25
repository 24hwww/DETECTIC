#!/usr/bin/env python3
"""Phase 14.8 — Read-only RF sensor capability audit for TP-Link EX520V.

This script authenticates to the router via the proven IPv6 link-local GTPR
path and issues encrypted `getList` (`gl`) calls for EVERY Wi-Fi-related OID
declared in `oid_str.js`, plus a few scalar `get` calls on radio/diagnostic
objects. It is strictly read-only:

  * only `gl` and `get` operations are sent
  * no `so`, no `go`, no `ACT_*`, no scan triggers, no reboots
  * no configuration is modified

The output is a structured JSON file per probe run, suitable for diffing
across T0..T4 controlled observations.

Usage:
    export DETECTIC_PASSWORD='CHANGE_ME'
    python3 probe_phase14_8.py \\
        --url 'http://[fe80::3e6a:d2ff:fe5f:abc1%enp2s0]' \\
        --user admin \\
        --tag T0_baseline \\
        --out /tmp/phase14_8_T0.json

Repeat with --tag T1_present, T2_farther, T3_gone, T4_return to capture
the controlled observation series.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from datetime import datetime, timezone
from typing import Dict, List

from detectic_client import Dialect, GtprClient


# ---------------------------------------------------------------------------
# OID catalog — every Wi-Fi / RF / station / radio / scan / neighbor /
# diagnostic / mesh object declared in _rootfs/web/js/oid_str.js, grouped by
# expected relevance.  We deliberately over-probe: even OIDs whose handlers
# are static stubs are queried so the live result is recorded empirically.
# ---------------------------------------------------------------------------

# Group A — proven live associated-device source (control / sanity check)
GROUP_PROVEN = [
    "DEV2_WIFI_APDEV_ASSOCDEV",
    "DEV2_HOST_ENTRY",
]

# Group B — radio / radio-stats / diagnostic (real handlers expected)
GROUP_RADIO = [
    "DEV2_WIFI_RADIO",
    "WIFI_RADIO_STATS",
    "DEV2_WIFI_DIAGNOSTICRESULT",
    "DEV2_WIFI_APDEV_RADIO",
    "DEV2_WIFI_APDEV_STATS",
    "DEV2_WIFI_APDEV_MEMSTATUS",
]

# Group C — per-station stats / history / QoE (handlers reported REAL)
GROUP_STATION_STATS = [
    "DEV2_WIFI_APDEV_STESTATS",
    "DEV2_WIFI_APDEV_STEHISTORY",
    "DEV2_WIFI_APDEV_QOE",
    "DEV2_WIFI_APDEV_INTERFACE",
    "DEV2_WIFI_APDEV_COMPONENT",
    "DEV2_WIFI_APDEV_ETHASSOCDEV",
    "DEV2_WIFI_APDEV_ETHERNET",
    "DEV2_WIFI_ASSOC_DEV_STAT",
    "DEV2_WIFI_MACTABLE",
    "DEV2_WIFI_STEERINGSTATS",
    "DEV2_STA_X_TP_QOE",
    "DEV2_STA_X_TP_QOE_FACTOR",
]

# Group D — EasyMesh DataElements (handlers reported STUB, but probe anyway)
GROUP_DATAELEMENTS = [
    "DEV2_WIFI_DATAELEMENT",
    "DEV2_WIFI_DE_NETWORK",
    "DEV2_WIFI_DE_DEV",
    "DEV2_WIFI_DE_RADIO",
    "DEV2_WIFI_DE_BSS",
    "DEV2_WIFI_DE_STA",
    "DEV2_WIFI_DE_STA_MULTIAPSTA",
    "DEV2_WIFI_DE_BACKHAUL_STA",
    "DEV2_WIFI_DE_CAP",
    "DEV2_WIFI_DE_CAP_PROF",
    "DEV2_WIFI_DE_SAMESSIDRESULT",
    "DEV2_WIFI_DE_CUROPCLASSPROF",
    "DEV2_WIFI_DE_SCAN_RESULT",
    "DEV2_WIFI_DE_OPCLASS_SCAN",
    "DEV2_WIFI_DE_CHANNEL_SCAN",
    "DEV2_WIFI_DE_NEIGHBORBSS",
    "DEV2_WIFI_DE_UNASSOCSTA",
    "DEV2_WIFI_DE_ASSOC_EVENT",
    "DEV2_WIFI_DE_ASSOC_DATA",
    "DEV2_WIFI_DE_DISASSOC_EVENT",
    "DEV2_WIFI_DE_DISASSOC_DATA",
    "DEV2_WIFI_DE_FAILCNNCT",
    "DEV2_WIFI_DE_FAILCNNCT_DATA",
]

# Group E — neighbor / survey / onboarding / customtopo
GROUP_NEIGHBOR = [
    "DEV2_WIFI_NEIGHBORWIFI",
    "DEV2_WIFI_APDEV_NEIGHBORSIG",
    "DEV2_WIFI_APDEV_CUSTOMTOPO",
    "DEV2_X_TP_ONBOARDBYSCANNING",
    "DEV2_WIFI_APDEV_AP",
    "DEV2_WIFI_APDEV_WPS",
    "DEV2_WIFI_APDEV",
    "DEV2_WIFI",
    "DEV2_WIFI_SSID",
    "WIFI_SSID_STATS",
    "DEV2_WIFI_AP",
    "DEV2_WIFI_ASSOC_DEV",
    "DEV2_WIFI_BANDSTEERING",
    "DEV2_WIFI_MULTIAP",
    "DEV2_WIFIINTERFERENCETEST",
]

# Group F — mesh / RE / status objects that may carry RF state
GROUP_MESH_STATUS = [
    "DEV2_X_TP_MESH_INTL_STATUS",
    "DEV2_RE_MAP_STATUS",
    "DEV2_WIFI_DPPINFO",
]

ALL_GROUPS = [
    ("proven", GROUP_PROVEN),
    ("radio", GROUP_RADIO),
    ("station_stats", GROUP_STATION_STATS),
    ("dataelements", GROUP_DATAELEMENTS),
    ("neighbor", GROUP_NEIGHBOR),
    ("mesh_status", GROUP_MESH_STATUS),
]


def all_oids() -> List[str]:
    seen = set()
    out = []
    for _, group in ALL_GROUPS:
        for oid in group:
            if oid not in seen:
                seen.add(oid)
                out.append(oid)
    return out


def parse_response(text: str) -> dict:
    try:
        return json.loads(text)
    except Exception as e:
        return {"_raw": text, "_parse_error": str(e)}


def classify(parsed: dict) -> Dict[str, object]:
    """Produce a compact classification of one OID response."""
    if not isinstance(parsed, dict):
        return {"status": "unparseable"}
    if "errorcode" in parsed:
        return {"status": "error", "errorcode": parsed.get("errorcode")}
    if parsed.get("success") is False:
        return {"status": "failed", "errorcode": parsed.get("errorcode")}
    data = parsed.get("data")
    if data is None:
        return {"status": "no_data"}
    if isinstance(data, list):
        return {"status": "list", "count": len(data), "empty": len(data) == 0}
    if isinstance(data, dict):
        return {
            "status": "object",
            "keys": list(data.keys())[:20],
            "empty": len(data) == 0,
        }
    return {"status": "unknown_shape"}


def run_probe(client: GtprClient, oids: List[str], delay_ms: int = 250) -> Dict:
    results = {
        "probed_at": datetime.now(timezone.utc).isoformat(),
        "router_url": client.base,
        "username": client.user,
        "oid_count": len(oids),
        "oids": [],
    }
    for oid in oids:
        entry: Dict = {"oid": oid}
        try:
            raw = client.gl(oid)
            entry["raw_len"] = len(raw) if raw else 0
            parsed = parse_response(raw)
            entry["parsed"] = parsed
            entry["class"] = classify(parsed)
            # If list with data, capture a redacted summary of the first entry
            if isinstance(parsed, dict) and isinstance(parsed.get("data"), list) and parsed["data"]:
                first = parsed["data"][0]
                if isinstance(first, dict):
                    entry["first_entry_keys"] = list(first.keys())
            entry["error"] = None
        except Exception as e:
            entry["raw_len"] = 0
            entry["parsed"] = None
            entry["class"] = {"status": "exception"}
            entry["error"] = f"{type(e).__name__}: {e}"
        results["oids"].append(entry)
        if delay_ms:
            time.sleep(delay_ms / 1000.0)
    return results


def summarize(results: Dict) -> str:
    lines = [
        f"Probe target : {results['router_url']}",
        f"Probed at    : {results['probed_at']}",
        f"OIDs probed  : {results['oid_count']}",
        "",
        f"{'OID':<40} {'status':<12} {'count':>6} {'err':>6} {'keys'}",
        "-" * 100,
    ]
    for e in results["oids"]:
        cls = e.get("class", {})
        status = cls.get("status", "?")
        count = cls.get("count", "-")
        err = cls.get("errorcode", "-")
        keys = ", ".join(cls.get("keys", [])[:6]) if cls.get("keys") else ""
        if not keys and e.get("first_entry_keys"):
            keys = ", ".join(e["first_entry_keys"][:8])
        lines.append(
            f"{e['oid']:<40} {str(status):<12} {str(count):>6} "
            f"{str(err):>6} {keys}"
        )
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser(
        description="Phase 14.8 read-only RF capability audit for EX520V"
    )
    ap.add_argument(
        "--url",
        default=os.environ.get(
            "DETECTIC_URL", "http://[fe80::3e6a:d2ff:fe5f:abc1%enp2s0]"
        ),
    )
    ap.add_argument("--user", default=os.environ.get("DETECTIC_USER", "admin"))
    ap.add_argument(
        "--password", default=os.environ.get("DETECTIC_PASSWORD")
    )
    ap.add_argument(
        "--dialect",
        choices=[Dialect.GDPR_JSON, Dialect.GDPR_TEXT],
        default=Dialect.GDPR_JSON,
    )
    ap.add_argument(
        "--tag",
        default="T0",
        help="Tag for this probe run (e.g. T0_baseline, T1_present)",
    )
    ap.add_argument(
        "--out",
        default=None,
        help="JSON output file (default: /tmp/phase14_8_<tag>.json)",
    )
    ap.add_argument(
        "--delay", type=int, default=250,
        help="Milliseconds between requests (default: 250)",
    )
    ap.add_argument(
        "--oids", default=None,
        help="Comma-separated OIDs to probe (default: all groups)",
    )
    args = ap.parse_args()

    if not args.password:
        ap.error("--password or DETECTIC_PASSWORD env var is required")

    oids = (
        [o.strip() for o in args.oids.split(",") if o.strip()]
        if args.oids
        else all_oids()
    )
    out_path = args.out or f"/tmp/phase14_8_{args.tag}.json"

    print(f"[+] Connecting to {args.url} as {args.user} (tag={args.tag}) ...")
    client = GtprClient(args.url, args.user, args.password, args.dialect)
    client.connect()
    print(f"[+] Authenticated. Probing {len(oids)} OIDs read-only ...")

    results = run_probe(client, oids, args.delay)
    results["tag"] = args.tag

    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(results, f, indent=2, ensure_ascii=False)

    print(f"[+] Raw results written to {out_path}")
    print()
    print(summarize(results))


if __name__ == "__main__":
    main()
