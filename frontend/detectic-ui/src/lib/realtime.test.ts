import { describe, it, expect } from "bun:test";
import { extractDevice, extractPoint } from "./realtime";

// HTTP-ingested EventEnvelope shape (device_id at outer level)
function makeHttpEnvelope(eventType: string, deviceId: string, inner: Record<string, unknown>) {
  return {
    type: "broadcast",
    sensor_id: "lab-sensor-01",
    server_time: 1_700_000_000_000,
    payload: {
      event_id: "ev-1",
      sequence: 1,
      sensor_id: "lab-sensor-01",
      timestamp: 1_700_000_000,
      type: eventType,
      device_id: deviceId,
      payload: inner,
    },
  };
}

// WSS sensor-event broadcast shape (device_id inside inner payload)
// After the backend fix, the event object is the broadcast payload.
function makeWssEventBroadcast(eventType: string, deviceId: string, inner: Record<string, unknown>) {
  return {
    type: "broadcast",
    sensor_id: "ex520-001",
    server_time: 1_700_000_000_000,
    observed_at: 1_700_000_000_000,
    payload: {
      event_id: "ev-wss-1",
      sequence: 1,
      sensor_id: "ex520-001",
      timestamp: 1_700_000_000,
      type: eventType,
      payload: { device_id: deviceId, ...inner },
    },
  };
}

describe("extractDevice HTTP shape", () => {
  it("returns connected=true for device.connected", () => {
    const dev = extractDevice(
      makeHttpEnvelope("device.connected", "pseudo-aa", {
        rssi: -45,
        band: "2.4GHz",
        hostname: "phone",
      })
    );
    expect(dev).not.toBeNull();
    expect(dev?.device_id).toBe("pseudo-aa");
    expect(dev?.connected).toBe(true);
    expect(dev?.last_signal).toBe(-45);
    expect(dev?.band).toBe("2.4GHz");
    expect(dev?.last_type).toBe("device.connected");
    expect(dev?.last_seen).toBe(1_700_000_000_000);
  });

  it("returns connected=false for device.disconnected", () => {
    const dev = extractDevice(
      makeHttpEnvelope("device.disconnected", "pseudo-bb", {
        last_signal: -88,
        band: "5GHz",
        session_id: "s-1",
        duration_seconds: 120,
      })
    );
    expect(dev).not.toBeNull();
    expect(dev?.device_id).toBe("pseudo-bb");
    expect(dev?.connected).toBe(false);
    expect(dev?.last_signal).toBe(-88);
    expect(dev?.last_type).toBe("device.disconnected");
  });

  it("updates last_signal on device.signal_changed", () => {
    const dev = extractDevice(
      makeHttpEnvelope("device.signal_changed", "pseudo-aa", {
        old_signal: -60,
        new_signal: -55,
      })
    );
    expect(dev?.connected).toBe(true);
    expect(dev?.last_signal).toBe(-55);
  });
});

describe("extractDevice WSS shape", () => {
  it("reads device_id from inner payload for device.signal_changed", () => {
    const dev = extractDevice(
      makeWssEventBroadcast("device.signal_changed", "wss-aa", {
        new_signal: -62,
        old_signal: -58,
        band: "2.4GHz",
        hostname: "moto",
      })
    );
    expect(dev).not.toBeNull();
    expect(dev?.device_id).toBe("wss-aa");
    expect(dev?.connected).toBe(true);
    expect(dev?.last_signal).toBe(-62);
    expect(dev?.band).toBe("2.4GHz");
  });

  it("returns connected=false for device.disconnected", () => {
    const dev = extractDevice(
      makeWssEventBroadcast("device.disconnected", "wss-bb", {
        last_signal: -90,
        band: "5GHz",
      })
    );
    expect(dev).not.toBeNull();
    expect(dev?.device_id).toBe("wss-bb");
    expect(dev?.connected).toBe(false);
    expect(dev?.last_signal).toBe(-90);
  });

  it("ignores non-device events", () => {
    const net = extractDevice({
      type: "broadcast",
      payload: {
        type: "network.detected",
        payload: { ap_id: "ap-1", rssi: -70 },
      },
    });
    expect(net).toBeNull();
  });
});

// Raw WSS sensor message shape (deprecated backend format):
// broadcast.payload is the sensor WebSocket message, real event is nested.
function makeRawWssBroadcast(eventType: string, deviceId: string, inner: Record<string, unknown>) {
  return {
    type: "broadcast",
    sensor_id: "ex520-001",
    server_time: 1_700_000_000_000,
    payload: {
      event_id: "ev-raw-1",
      observed_at: 1_700_000_000_000,
      protocol: 1,
      sensor_id: "ex520-001",
      type: "event",
      payload: {
        event_id: "ev-raw-1",
        sequence: 1,
        sensor_id: "ex520-001",
        timestamp: 1_700_000_000,
        type: eventType,
        device_id: eventType.startsWith("network.") ? undefined : deviceId,
        payload: eventType.startsWith("network.") ? { ap_id: deviceId, ...inner } : inner,
      },
    },
  };
}

describe("extractDevice raw WSS message shape", () => {
  it("handles device.connected inside raw sensor msg", () => {
    const dev = extractDevice(
      makeRawWssBroadcast("device.connected", "raw-aa", {
        rssi: -50,
        band: "2.4GHz",
        hostname: "phone",
      })
    );
    expect(dev).not.toBeNull();
    expect(dev?.device_id).toBe("raw-aa");
    expect(dev?.connected).toBe(true);
    expect(dev?.last_signal).toBe(-50);
    expect(dev?.last_type).toBe("device.connected");
  });

  it("handles device.disconnected inside raw sensor msg", () => {
    const dev = extractDevice(
      makeRawWssBroadcast("device.disconnected", "raw-bb", {
        last_signal: -88,
        band: "5GHz",
      })
    );
    expect(dev).not.toBeNull();
    expect(dev?.device_id).toBe("raw-bb");
    expect(dev?.connected).toBe(false);
    expect(dev?.last_signal).toBe(-88);
  });

  it("handles device.signal_changed inside raw sensor msg", () => {
    const dev = extractDevice(
      makeRawWssBroadcast("device.signal_changed", "raw-cc", { new_signal: -60, band: "2.4GHz" })
    );
    expect(dev?.device_id).toBe("raw-cc");
    expect(dev?.last_signal).toBe(-60);
  });
});

describe("extractPoint", () => {
  it("creates a point from HTTP device.connected rssi", () => {
    const point = extractPoint(
      makeHttpEnvelope("device.connected", "pseudo-aa", { rssi: -50 })
    );
    expect(point).not.toBeNull();
    expect(point?.pseudonym).toBe("pseudo-aa");
    expect(point?.rssi).toBe(-50);
    expect(point?.ts).toBe(1_700_000_000);
  });

  it("uses last_signal for HTTP device.disconnected", () => {
    const point = extractPoint(
      makeHttpEnvelope("device.disconnected", "pseudo-bb", { last_signal: -90 })
    );
    expect(point?.rssi).toBe(-90);
  });

  it("reads pseudonym from inner payload for WSS shape", () => {
    const point = extractPoint(
      makeWssEventBroadcast("device.signal_changed", "wss-cc", { new_signal: -55 })
    );
    expect(point?.pseudonym).toBe("wss-cc");
    expect(point?.rssi).toBe(-55);
  });

  it("returns null when no signal is present", () => {
    const point = extractPoint(makeHttpEnvelope("device.band_changed", "pseudo-cc", {}));
    expect(point).toBeNull();
  });
});
