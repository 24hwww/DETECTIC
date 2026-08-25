#!/usr/bin/env python3
"""EX520 GTPR Tool — All-in-one host-side tool for querying and setting OIDs.

Uses the Python detectic_client (no cross-compilation needed on host).

Usage:
    python3 gtpr_tool.py query DEV2_SSH_CFG
    python3 gtpr_tool.py set DEV2_SSH_CFG '{"Enable":"1"}'
    python3 gtpr_tool.py test-ssh
    python3 gtpr_tool.py audit
    python3 gtpr_tool.py exec "uname -a"
"""

import argparse
import json
import os
import sys
import time
import subprocess
from urllib.parse import urljoin

# Add parent dir to path so we can import detectic_client
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python"))
from detectic_client import GtprClient, Dialect


def get_client(args):
    url = args.url or os.environ.get("EX520_URL", 
        "http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]")
    user = args.user or os.environ.get("EX520_USER", "user")
    password = args.password or os.environ.get("DETECTIC_PASSWORD", "")
    if not password:
        print("ERROR: Set DETECTIC_PASSWORD env or use --password", file=sys.stderr)
        sys.exit(1)
    dialect = Dialect.GDPR_JSON if args.dialect == "json" else Dialect.GDPR_TEXT
    return GtprClient(url, user, password, dialect)


def cmd_query(args):
    """Query an OID via GTPR go."""
    c = get_client(args)
    c.connect()
    result = c.gl(args.oid)
    try:
        data = json.loads(result)
        print(json.dumps(data, indent=2))
    except json.JSONDecodeError:
        print(result)


def cmd_set(args):
    """Set fields on an OID via GTPR so."""
    c = get_client(args)
    c.connect()
    
    # Build the so payload
    data = args.data
    if isinstance(data, str):
        data = json.loads(data)
    
    payload = json.dumps({
        "data": data,
        "operation": "so",
        "oid": args.oid,
    })
    
    # Use the _operation method directly
    result = c._operation(payload + "\r\n")
    try:
        parsed = json.loads(result)
        print(json.dumps(parsed, indent=2))
    except json.JSONDecodeError:
        print(result)


def cmd_audit(args):
    """Audit all remote-access OIDs."""
    OIDS = [
        "DEV2_SSH_CFG",
        "DEV2_TELNET_CFG", 
        "X_TTNET_CONF_SHELL",
        "DEV2_USER_CFG",
        "DEV2_HTTP_CFG",
        "DEV2_CURRENT_USER",
        "DEV2_DIAG_TOOL",
        "DEV2_TTNET_CONFIG",
        "DEV2_WIFI_APDEV_ASSOCDEV",
        "DEV2_HOST_ENTRY",
        "DEV2_DHCPV4_CLIENT",
        "DEV2_LIFEMOTE_AGENT",
    ]
    
    c = get_client(args)
    c.connect()
    
    print("=== EX520 OID Audit ===\n")
    for oid in OIDS:
        try:
            result = c.gl(oid)
            data = json.loads(result)
            if "errorcode" in data or "error" in str(data).lower():
                print(f"  DENIED  {oid}: {json.dumps(data)[:200]}")
            else:
                print(f"  OK      {oid}: {json.dumps(data)[:200]}")
        except Exception as e:
            print(f"  ERROR   {oid}: {e}")


def cmd_test_ssh(args):
    """Test all SSH/telnet access vectors."""
    print("=== SSH/Telnet Vector Test ===\n")
    
    c = get_client(args)
    c.connect()
    
    # Step 1: Read current configs
    print("--- Current SSH config ---")
    try:
        r = c.gl("DEV2_SSH_CFG")
        print(f"  {r[:300]}")
    except Exception as e:
        print(f"  Error: {e}")
    
    print("\n--- Current Telnet config ---")
    try:
        r = c.gl("DEV2_TELNET_CFG")
        print(f"  {r[:300]}")
    except Exception as e:
        print(f"  Error: {e}")
    
    print("\n--- X_TTNET_CONF_SHELL ---")
    try:
        r = c.gl("X_TTNET_CONF_SHELL")
        print(f"  {r[:300]}")
    except Exception as e:
        print(f"  Error: {e}")
    
    # Step 2: Try enabling SSH
    print("\n--- Attempting SSH enable via DEV2_SSH_CFG ---")
    try:
        payload = json.dumps({
            "data": {
                "Enable": "1",
                "Port": "22",
                "stack": "0,0,0,0,0,0",
                "pstack": "0,0,0,0,0,0",
            },
            "operation": "so",
            "oid": "DEV2_SSH_CFG",
        })
        r = c._operation(payload + "\r\n")
        print(f"  Result: {r[:300]}")
    except Exception as e:
        print(f"  Error: {e}")
    
    # Step 3: Try enabling Telnet
    print("\n--- Attempting Telnet enable via DEV2_TELNET_CFG ---")
    try:
        payload = json.dumps({
            "data": {
                "telnetLocalEnabled": "1",
                "telnetLocalPort": "23",
                "stack": "0,0,0,0,0,0",
                "pstack": "0,0,0,0,0,0",
            },
            "operation": "so",
            "oid": "DEV2_TELNET_CFG",
        })
        r = c._operation(payload + "\r\n")
        print(f"  Result: {r[:300]}")
    except Exception as e:
        print(f"  Error: {e}")
    
    # Step 4: Try X_TTNET_CONF_SHELL
    print("\n--- Attempting Shell enable via X_TTNET_CONF_SHELL ---")
    try:
        payload = json.dumps({
            "data": {
                "Enable": "1",
                "stack": "0,0,0,0,0,0",
                "pstack": "0,0,0,0,0,0",
            },
            "operation": "so",
            "oid": "X_TTNET_CONF_SHELL",
        })
        r = c._operation(payload + "\r\n")
        print(f"  Result: {r[:300]}")
    except Exception as e:
        print(f"  Error: {e}")
    
    # Step 5: Wait and check ports
    print("\n--- Waiting 5s for services... ---")
    time.sleep(5)
    
    import socket
    ipv6 = os.environ.get("EX520_IPV6", "fe80::3e6a:d2ff:fe5f:abc1%enp2s0")
    for port in [22, 23, 80, 443]:
        try:
            s = socket.socket(socket.AF_INET6, socket.SOCK_STREAM)
            s.settimeout(3)
            s.connect((ipv6, port))
            s.close()
            print(f"  Port {port}: OPEN")
        except:
            print(f"  Port {port}: closed")
    
    # Step 6: Revert
    print("\n--- Reverting changes ---")
    for oid, data in [
        ("DEV2_SSH_CFG", {"Enable": "0", "stack": "0,0,0,0,0,0", "pstack": "0,0,0,0,0,0"}),
        ("DEV2_TELNET_CFG", {"telnetLocalEnabled": "0", "stack": "0,0,0,0,0,0", "pstack": "0,0,0,0,0,0"}),
        ("X_TTNET_CONF_SHELL", {"Enable": "0", "stack": "0,0,0,0,0,0", "pstack": "0,0,0,0,0,0"}),
    ]:
        try:
            payload = json.dumps({"data": data, "operation": "so", "oid": oid})
            c._operation(payload + "\r\n")
            print(f"  Reverted {oid}")
        except Exception as e:
            print(f"  Failed to revert {oid}: {e}")


def cmd_exec(args):
    """Execute command on router via phoenix.sh."""
    # This is a placeholder — the actual implementation uses the gateway
    print(f"Command: {args.command}")
    print("Use bidir_gateway.py or remote_exec.sh for remote execution")
    print("Or use: ./remote_exec.sh '" + args.command + "'")


def main():
    ap = argparse.ArgumentParser(description="EX520 GTPR Tool")
    ap.add_argument("--url", help="Router URL")
    ap.add_argument("--user", help="Router username")
    ap.add_argument("--password", help="Router password")
    ap.add_argument("--dialect", default="json", choices=["json", "text"])
    
    sub = ap.add_subparsers(dest="command", required=True)
    
    p_query = sub.add_parser("query", help="Query an OID")
    p_query.add_argument("oid", help="OID to query")
    
    p_set = sub.add_parser("set", help="Set OID fields")
    p_set.add_argument("oid", help="OID to set")
    p_set.add_argument("data", help="JSON data fields")
    
    sub.add_parser("audit", help="Audit all remote-access OIDs")
    sub.add_parser("test-ssh", help="Test SSH/telnet access vectors")
    
    p_exec = sub.add_parser("exec", help="Execute command on router")
    p_exec.add_argument("command", help="Command to execute")
    
    args = ap.parse_args()
    
    if args.command == "query":
        cmd_query(args)
    elif args.command == "set":
        cmd_set(args)
    elif args.command == "audit":
        cmd_audit(args)
    elif args.command == "test-ssh":
        cmd_test_ssh(args)
    elif args.command == "exec":
        cmd_exec(args)


if __name__ == "__main__":
    main()
