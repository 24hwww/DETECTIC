#!/usr/bin/env python3
"""Deterministic state-machine tests for the EX520 cold-boot watchdog.

Does NOT require a live router. It drives `watchdog._watchdog_loop` with a
fake clock and a scripted reachability stream.
"""
import os
import sys

# Make the package directory importable and provide a dummy secret so the
# module loads without real credentials.
os.environ.setdefault("DETECTIC_PASSWORD", "dummy")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import watchdog


class FakeClock:
    def __init__(self, start=0):
        self.t = start

    def now(self):
        return self.t

    def sleep(self, n):
        self.t += n


def run_scenario(states, threshold=2, poll=1, expect=0):
    """Run the state machine through `states` and return (events, trigger_count).

    `states` is a list of True/False values consumed one per watchdog tick.
    The first value is also the initial reachability check.
    """
    clock = FakeClock()
    events = []
    triggers = []

    def is_reachable():
        return states.pop(0)

    def do_trigger():
        triggers.append(clock.t)
        return True

    try:
        watchdog._watchdog_loop(
            is_reachable=is_reachable,
            do_trigger=do_trigger,
            poll_interval=poll,
            down_threshold=threshold,
            now=clock.now,
            sleep=clock.sleep,
            logger=events.append,
        )
    except IndexError:
        # states exhausted
        pass

    return events, len(triggers), triggers


def assert_test(name, states, threshold, expect, expected_substrings=None):
    events, count, _ = run_scenario(list(states), threshold=threshold)
    ok = count == expect
    if expected_substrings:
        for s in expected_substrings:
            ok = ok and any(s in e for e in events)
    status = "PASS" if ok else "FAIL"
    print(f"[{status}] {name}: {count} trigger(s), expected {expect}")
    for e in events:
        print(f"    {e}")
    if not ok:
        raise AssertionError(f"Test {name} failed")


def main():
    threshold = 2  # seconds for testing; clock advances 1 s per tick

    # TEST 1: UP -> UP -> UP  -> 0 triggers
    assert_test(
        "TEST 1: persistent UP",
        [True, True, True, True, True],
        threshold,
        0,
        ["router UP at startup"],
    )

    # TEST 2: UP -> DOWN (< threshold) -> UP  -> 0 triggers
    # t=0 UP, t=1 DOWN (down_since=1), t=2 UP (down_for=1 < 2)
    assert_test(
        "TEST 2: brief blip",
        [True, False, True, True, True],
        threshold,
        0,
        ["brief blip"],
    )

    # TEST 3: UP -> DOWN (>= threshold) -> UP  -> exactly 1 trigger
    # t=0 UP, t=1 DOWN (since=1), t=2 DOWN (for=1), t=3 DOWN (for=2 arm),
    # t=4 UP (for=3 >= 2) -> trigger
    assert_test(
        "TEST 3: cold boot",
        [True, False, False, False, True, True, True],
        threshold,
        1,
        ["router went DOWN", "armed for re-trigger", "router UP after cold boot", "GTPR trigger SENT"],
    )

    # TEST 4: UP -> sustained DOWN -> UP -> UP -> UP  -> exactly 1 trigger
    assert_test(
        "TEST 4: trigger once then stable",
        [True, False, False, False, True, True, True, True],
        threshold,
        1,
        ["GTPR trigger SENT"],
    )

    # TEST 5: two full down/up cycles  -> exactly 2 triggers
    assert_test(
        "TEST 5: two cold boots",
        [True, False, False, False, True, True, False, False, False, True, True],
        threshold,
        2,
        ["GTPR trigger SENT"],
    )

    # TEST 6: long UP  -> 0 repeated triggers
    assert_test(
        "TEST 6: long stable UP",
        [True] + [True] * 20,
        threshold,
        0,
        ["router UP at startup"],
    )

    print("\nAll watchdog state-machine tests passed.")


if __name__ == "__main__":
    main()
