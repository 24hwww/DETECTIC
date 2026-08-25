#!/usr/bin/env python3
"""EX520 Detectic autostart watchdog.

Detects a router cold boot by monitoring IPv6 link-local reachability.
Sends a single GTPR `so DEV2_LIFEMOTE_AGENT` to start phoenix -> bootstart.sh
after the router has been down for at least DOWN_THRESHOLD seconds and then
comes back up.

State machine:
    UP (initial) -> DOWN -> DOWN >= threshold -> ARMED -> UP -> TRIGGER ONCE -> UP/WAIT
    Only another DOWN >= threshold may arm it again.
"""
import os
import subprocess
import sys
import time

DETECTIC = os.environ.get("DETECTIC_BIN", "detectic")
ROUTER_URL = os.environ.get(
    "EX520_URL",
    "http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]",
)
USER = os.environ.get("EX520_USER", "user")
PASSWORD = os.environ["DETECTIC_PASSWORD"]
PING6_TARGET = os.environ.get(
    "EX520_PING6",
    "fe80::3e6a:d2ff:fe5f:abc1%enp2s0",
)
PING6_IFACE = os.environ.get("EX520_PING6_IFACE", "enp2s0")
POLL_INTERVAL = int(os.environ.get("POLL_INTERVAL", "10"))
BOOTSTART_URL = os.environ.get(
    "BOOTSTART_URL",
    "http://192.168.0.27:8080/bootstart.sh",
)
DOWN_THRESHOLD = int(os.environ.get("DOWN_THRESHOLD", "30"))
PHOENIX_GRACE = int(os.environ.get("PHOENIX_GRACE", "45"))


def log(msg):
    ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    print(f"{ts} {msg}", flush=True)


def ping_reachable():
    try:
        ret = subprocess.run(
            ["ping6", "-c", "1", "-W", "2", PING6_TARGET],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=5,
        )
        return ret.returncode == 0
    except Exception:
        return False


def gtpr_query():
    env = {**os.environ, "DETECTIC_PASSWORD": PASSWORD}
    try:
        ret = subprocess.run(
            [
                DETECTIC,
                "--url",
                ROUTER_URL,
                "--user",
                USER,
                "query",
                "DEV2_LIFEMOTE_AGENT",
            ],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=20,
        )
        return ret.returncode == 0
    except Exception:
        return False


def trigger_bootstart():
    payload = (
        '{"enable":"1","URL":"%s","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'
        % BOOTSTART_URL
    )
    env = {**os.environ, "DETECTIC_PASSWORD": PASSWORD}
    try:
        ret = subprocess.run(
            [
                DETECTIC,
                "--url",
                ROUTER_URL,
                "--user",
                USER,
                "set",
                "DEV2_LIFEMOTE_AGENT",
                payload,
            ],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=30,
        )
        return ret.returncode == 0
    except Exception:
        return False


def _watchdog_loop(is_reachable, do_trigger, poll_interval, down_threshold,
                   phoenix_grace=0,
                   now=time.time, sleep=time.sleep, logger=log):
    """Deterministic watchdog state machine. Runs forever.

    `is_reachable()` should return True when the router is reachable.
    `do_trigger()` should perform the GTPR set and return True on success.
    `phoenix_grace` is the seconds to wait after a cold-boot up event so
    `phoenix` is ready before the `so` trigger is sent.
    `now()` and `sleep()` are injectable for testing.
    """
    router_up = is_reachable()
    down_since = None
    triggered_for_boot = router_up  # True: already up; do not trigger again
    armed_logged = False
    if router_up:
        logger("router UP at startup; waiting for a cold boot to trigger")

    while True:
        reachable = is_reachable()

        if reachable:
            if not router_up:
                if down_since is not None:
                    down_for = now() - down_since
                    if down_for >= down_threshold:
                        logger("router UP after cold boot")
                        if not triggered_for_boot:
                            # Give the router's phoenix service time to start
                            # after a cold boot before firing the so trigger.
                            logger(f"waiting {phoenix_grace}s for phoenix")
                            sleep(phoenix_grace)
                            if do_trigger():
                                triggered_for_boot = True
                                logger("GTPR trigger SENT")
                            else:
                                logger("GTPR trigger FAILED, will retry")
                    else:
                        logger("router reachable after brief blip, no trigger")
                else:
                    logger("router UP")
                router_up = True
            down_since = None
            armed_logged = False
        else:
            if router_up:
                router_up = False
                down_since = now()
                armed_logged = False
                # Re-arm immediately on any DOWN; the UP branch will only fire
                # if the down actually persists past the threshold.
                triggered_for_boot = False
                logger("router went DOWN")
            elif down_since is not None:
                down_for = int(now() - down_since)
                # Log the arming only once per sustained down.
                if down_for >= down_threshold and not armed_logged:
                    armed_logged = True
                    logger(f"router down for {down_for}s, armed for re-trigger")

        sleep(poll_interval)


def main():
    log("watchdog starting")
    log(f"router={ROUTER_URL} bootstart={BOOTSTART_URL} poll={POLL_INTERVAL}s")
    _watchdog_loop(
        is_reachable=lambda: ping_reachable() or gtpr_query(),
        do_trigger=trigger_bootstart,
        poll_interval=POLL_INTERVAL,
        down_threshold=DOWN_THRESHOLD,
        phoenix_grace=PHOENIX_GRACE,
    )


if __name__ == "__main__":
    main()
