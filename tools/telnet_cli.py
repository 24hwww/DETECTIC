#!/usr/bin/env python3
"""Telnet CLI helper for EX520V experiments (M11).

Logs into the TP-Link CLI over telnet, runs a list of commands, prints all
output. Credentials come from env: DETECTIC_ADMIN_PW.
Usage: python3 tools/telnet_cli.py <host> '<command1>' '<command2>' ...
"""
import os
import sys
import time

import pexpect


def main() -> int:
    host = sys.argv[1]
    cmds = sys.argv[2:]
    pw = os.environ.get("DETECTIC_ADMIN_PW", "")
    if not pw:
        print("DETECTIC_ADMIN_PW not set", file=sys.stderr)
        return 2

    t = pexpect.spawn(
        f"telnet {host}", encoding="utf-8", timeout=25, codec_errors="replace"
    )
    try:
        i = t.expect(["password:", "Password:", "Set new password:", pexpect.TIMEOUT])
        if i == 2:
            print("!! first-login flow triggered (pwdSign=0) — aborting", file=sys.stderr)
            t.close(force=True)
            return 3
        if i == 3:
            print("!! no password prompt seen", file=sys.stderr)
            print(t.before)
            t.close(force=True)
            return 4
        t.sendline(pw)
        time.sleep(0.5)
        t.expect(["#", ">", pexpect.TIMEOUT], timeout=15)
        banner = t.before
        for c in cmds:
            t.sendline(c)
            time.sleep(0.4)
            idx = t.expect([r"\(conf\)#", r"# ", "> ", pexpect.TIMEOUT], timeout=30)
            if idx == 3:
                # Some commands take longer; grab whatever arrived.
                pass
            out = t.before.replace("\r", "")
            print(f"$ {c}")
            print(out)
        t.sendline("exit")
        time.sleep(0.3)
        t.close(force=True)
        _ = banner
        return 0
    except (pexpect.EOF, pexpect.TIMEOUT) as e:
        print(f"!! session ended: {type(e).__name__}", file=sys.stderr)
        print(t.before or "", file=sys.stderr)
        t.close(force=True)
        return 1


if __name__ == "__main__":
    sys.exit(main())
