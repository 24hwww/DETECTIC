#!/usr/bin/env python3
"""
Poll DEV2_WIFI_DE_UNASSOCSTA and DEV2_WIFI_APDEV_ASSOCDEV continuously.
Purpose: Determine if the EX520 can detect Wi-Fi devices NOT associated to it.
"""
import sys, json, time, datetime
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'python'))
from detectic_client import GtprClient

URL = 'http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]'
USER = 'user'
PASSWORD = 'CHANGE_ME'
INTERVAL = 5       # seconds between polls
DURATION = 120     # total seconds to run

def ts():
    return datetime.datetime.now().strftime('%H:%M:%S')

def main():
    c = GtprClient(URL, USER, PASSWORD)
    c.connect()

    print(f'=== UNASSOCSTA Polling Test ===')
    print(f'Start: {ts()} | Interval: {INTERVAL}s | Duration: {DURATION}s')
    print(f'Instructions: Put a test device Wi-Fi ON but NOT connected to REYES network')
    print()

    seen_unassoc = set()
    seen_assoc = set()
    poll_count = 0
    start = time.time()

    while time.time() - start < DURATION:
        elapsed = int(time.time() - start)
        poll_count += 1

        # Poll associated devices
        assoc_macs = set()
        try:
            r = c.gl('DEV2_WIFI_APDEV_ASSOCDEV')
            d = json.loads(r)
            data = d.get('data', [])
            if isinstance(data, list):
                for entry in data:
                    mac = entry.get('MACAddress', '?')
                    host = entry.get('X_TP_HostName', '?')
                    sig = entry.get('signalStrength', '?')
                    assoc_macs.add(mac)
                    seen_assoc.add(mac)
        except Exception as e:
            print(f'  [{ts()}] ASSOCDEV error: {e}')

        # Poll unassociated stations
        unassoc_count = 0
        unassoc_data = []
        try:
            r = c.gl('DEV2_WIFI_DE_UNASSOCSTA')
            d = json.loads(r)
            data = d.get('data', [])
            if isinstance(data, list):
                unassoc_count = len(data)
                unassoc_data = data
                for entry in data:
                    mac = entry.get('MACAddress', entry.get('mac', '?'))
                    if mac not in seen_unassoc:
                        seen_unassoc.add(mac)
        except Exception as e:
            print(f'  [{ts()}] UNASSOCSTA error: {e}')

        # Print status
        status = f'[{ts()}] #{poll_count:3d} t={elapsed:3d}s | assoc={len(assoc_macs)}'
        if unassoc_count > 0:
            status += f' | UNASSOC={unassoc_count} *** DETECTED ***'
            print(status, flush=True)
            for entry in unassoc_data:
                print(f'    >> {json.dumps(entry)}', flush=True)
        else:
            status += f' | unassoc=0'
            print(status, flush=True)

        time.sleep(INTERVAL)

    # Summary
    print()
    print(f'=== Summary ===')
    print(f'Duration: {DURATION}s | Polls: {poll_count}')
    print(f'Unique associated MACs seen: {len(seen_assoc)}')
    for mac in seen_assoc:
        print(f'  {mac}')
    print(f'Unique unassociated MACs seen: {len(seen_unassoc)}')
    for mac in seen_unassoc:
        print(f'  {mac}')
    if len(seen_unassoc) > 0:
        print(f'\\nRESULT: EX520 CAN detect unassociated devices via DEV2_WIFI_DE_UNASSOCSTA')
    else:
        print(f'\\nRESULT: No unassociated devices detected (test device may not be in range)')

if __name__ == '__main__':
    main()
