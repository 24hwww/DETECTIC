#!/usr/bin/env python3
"""Read-only probe for non-associated Wi-Fi devices on a TP-Link EX520V.

This script authenticates to the router using the GTPR/GDPR protocol and then
issues encrypted `getList` (`gl`) calls for every data-model object that might
expose unassociated stations, scanned BSSs, neighbor APs, or radio-level scan
state. It never writes configuration, never starts scans, and never reboots the
router — it is purely a diagnostic read.

Run from the same directory as `detectic_client.py`:

    export DETECTIC_PASSWORD="your-web-password"
    python3 probe_unassociated.py --url http://192.168.0.1 --user admin \
                                  --out unassociated_probe.json

Interpretation:
    - OIDs that return `success: true` and a non-empty `data` field are live
      sources that may carry non-associated client or neighbor information.
    - OIDs that return an `errorcode` (e.g. 9003 / 9804 / 1000) or an empty
      `data` array are not populated in this firmware build.
    - The script is deliberately conservative: it does not send `go` / `set` /
      `ACT_WLAN_SCAN` operations because those can be disruptive.
"""

import argparse
import json
import os
import sys
import time
from datetime import datetime, timezone
from typing import Dict, List

from detectic_client import Dialect, GtprClient

# Objects ordered from most directly relevant to least relevant.
CANDIDATE_OIDS: List[str] = [
    # Direct non-associated station object (DataElements -> UnassociatedSTA)
    "DEV2_WIFI_DE_UNASSOCSTA",
    # Neighboring Wi-Fi diagnostic (CWMP / USP style site survey)
    "DEV2_WIFI_NEIGHBORWIFI",
    # EasyMesh channel / scan result containers
    "DEV2_WIFI_DE_SCAN_RESULT",
    "DEV2_WIFI_DE_OPCLASS_SCAN",
    "DEV2_WIFI_DE_CHANNEL_SCAN",
    "DEV2_WIFI_DE_NEIGHBORBSS",
    "DEV2_WIFI_APDEV_NEIGHBORSIG",
    # Onboarding / mesh scan control and results
    "DEV2_X_TP_ONBOARDBYSCANNING",
    # Radio and diagnostic state (may contain noise / channel / error counters)
    "DEV2_WIFI_RADIO",
    "DEV2_WIFI_APDEV_RADIO",
    "DEV2_WIFI_DIAGNOSTICRESULT",
    "WIFI_RADIO_STATS",
    # Baseline object we already know works for associated clients
    "DEV2_WIFI_APDEV_ASSOCDEV",
]


def parse_response(text: str) -> dict:
    """Best-effort parse; keep the raw string if JSON decoding fails."""
    try:
        return json.loads(text)
    except Exception as e:
        return {"_raw": text, "_parse_error": str(e)}


def run_probe(client: GtprClient, oids: List[str], delay_ms: int = 300) -> Dict:
    """Issue a `gl` for each OID and record the result."""
    results = {
        "probed_at": datetime.now(timezone.utc).isoformat(),
        "router_url": client.base,
        "username": client.user,
        "oids": [],
    }
    for oid in oids:
        entry = {"oid": oid, "success": False, "data_present": False}
        try:
            raw = client.gl(oid)
            entry["raw"] = raw
            parsed = parse_response(raw)
            entry["parsed"] = parsed
            if isinstance(parsed, dict):
                entry["success"] = parsed.get("success", True)
                data = parsed.get("data")
                # data may be a dict (single object) or a list (getList)
                if data is not None:
                    if isinstance(data, list):
                        entry["data_present"] = len(data) > 0
                        entry["count"] = len(data)
                    elif isinstance(data, dict):
                        entry["data_present"] = len(data) > 0
                        entry["count"] = len(data)
            entry["error"] = None
        except Exception as e:
            entry["error"] = f"{type(e).__name__}: {e}"
            entry["raw"] = None
            entry["parsed"] = None
        results["oids"].append(entry)
        if delay_ms:
            time.sleep(delay_ms / 1000.0)
    return results


def summarize(results: Dict) -> str:
    lines = [
        f"Probe target: {results['router_url']}",
        f"Probed at:    {results['probed_at']}",
        "",
        f"{'OID':<40} {'success':>8} {'data':>5} {'count':>6} {'error':<30}",
        "-" * 90,
    ]
    for e in results["oids"]:
        count = "-"
        if "count" in e:
            count = str(e["count"])
        err = e.get("error") or ""
        if not err and not e.get("success"):
            parsed = e.get("parsed", {})
            err = f"errorcode={parsed.get('errorcode', '?')}"
        lines.append(
            f"{e['oid']:<40} {str(e.get('success', False)):>8} "
            f"{str(e.get('data_present', False)):>5} {count:>6} {err:<30}"
        )
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser(
        description="Read-only probe for non-associated Wi-Fi devices on EX520V"
    )
    ap.add_argument("--url",
                    default=os.environ.get("DETECTIC_URL", "http://192.168.0.1"))
    ap.add_argument("--user",
                    default=os.environ.get("DETECTIC_USER", "admin"))
    ap.add_argument("--password",
                    default=os.environ.get("DETECTIC_PASSWORD"))
    ap.add_argument("--dialect", choices=[Dialect.GDPR_JSON, Dialect.GDPR_TEXT],
                    default=Dialect.GDPR_JSON)
    ap.add_argument("--out", default="unassociated_probe.json",
                    help="JSON file to write the raw results to")
    ap.add_argument("--oids", default=",".join(CANDIDATE_OIDS),
                    help="Comma-separated OIDs to probe")
    ap.add_argument("--delay", type=int, default=300,
                    help="Milliseconds between requests (default: 300)")
    args = ap.parse_args()

    if not args.password:
        ap.error("--password or DETECTIC_PASSWORD environment variable is required")

    oids = [o.strip() for o in args.oids.split(",") if o.strip()]

    print(f"[+] Connecting to {args.url} as {args.user} ...")
    client = GtprClient(args.url, args.user, args.password, args.dialect)
    client.connect()
    print("[+] Authenticated. Probing OIDs (read-only, no scans started) ...")

    results = run_probe(client, oids, args.delay)

    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(results, f, indent=2, ensure_ascii=False)

    print(f"[+] Raw results written to {args.out}")
    print()
    print(summarize(results))


if __name__ == "__main__":
    main()
