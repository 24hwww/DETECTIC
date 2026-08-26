#!/usr/bin/env python3
"""End-to-end WebSocket test for the Detectic dashboard realtime path.

Connects as a sensor, emits device.connected and device.disconnected events,
and verifies that a frontend connection receives the broadcast.

Usage:
    python3 tests/dashboard-realtime-test.py
"""
import asyncio
import json
import time
import websockets

ORIGIN = "wss://detectic.24hwww.workers.dev"
SENSOR_ID = "ex520-001"
DEVICE_ID = f"dev_test_{int(time.time())}"


def event_msg(event_type, rssi=-55, band="2.4GHz"):
    now = int(time.time() * 1000)
    return {
        "type": "event",
        "sensor_id": SENSOR_ID,
        "event_id": f"test-{event_type}-{now}",
        "observed_at": now,
        "payload": {
            "event_type": event_type,
            "device_id": DEVICE_ID,
            "rssi": rssi,
            "band": band,
        },
    }


async def frontend_receiver(queue, stop, ready):
    uri = f"{ORIGIN}/ws?role=frontend&sensor_id={SENSOR_ID}"
    async with websockets.connect(uri) as ws:
        print(f"[frontend] connected to {uri}")
        try:
            while not stop.is_set():
                msg = await asyncio.wait_for(ws.recv(), timeout=8)
                data = json.loads(msg)
                print(f"[frontend] received: {data.get('type')}")
                if data.get("type") == "hello_ack":
                    ready.set()
                if data.get("type") == "broadcast":
                    inner = data.get("payload", {})
                    p = inner.get("payload", {})
                    print(f"[frontend] event: {p.get('event_type')} for {p.get('device_id')}")
                    queue.put_nowait(p.get("event_type"))
        except asyncio.TimeoutError:
            print("[frontend] timeout waiting for messages")


async def sensor_emitter():
    uri = f"{ORIGIN}/ws?role=sensor&sensor_id={SENSOR_ID}"
    async with websockets.connect(uri) as ws:
        hello = await ws.recv()
        print(f"[sensor] hello: {hello}")

        for event_type in ("device.connected", "device.disconnected"):
            msg = event_msg(event_type)
            await ws.send(json.dumps(msg))
            ack = await asyncio.wait_for(ws.recv(), timeout=5)
            ack_data = json.loads(ack)
            print(f"[sensor] {event_type} ack: {ack_data.get('type')} {ack_data.get('event_id')}")
            await asyncio.sleep(1.5)


async def main():
    queue = asyncio.Queue()
    stop = asyncio.Event()
    ready = asyncio.Event()

    receiver = asyncio.create_task(frontend_receiver(queue, stop, ready))
    # wait until the frontend is fully connected and acknowledged
    await asyncio.wait_for(ready.wait(), timeout=8)
    await sensor_emitter()

    # collect broadcasted events for a few seconds
    await asyncio.sleep(3)
    stop.set()
    try:
        await asyncio.wait_for(receiver, timeout=6)
    except asyncio.TimeoutError:
        receiver.cancel()

    events = []
    while not queue.empty():
        events.append(queue.get_nowait())

    print(f"\nEvents seen by frontend: {events}")

    assert "device.connected" in events, "device.connected was not broadcast to frontend"
    assert "device.disconnected" in events, "device.disconnected was not broadcast to frontend"
    print("\n✓ Realtime dashboard path works: connect/disconnect broadcasts reached frontend")


if __name__ == "__main__":
    asyncio.run(main())
