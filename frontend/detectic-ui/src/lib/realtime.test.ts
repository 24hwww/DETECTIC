import { describe, it, expect } from "bun:test";
import { extractDevice, extractPoint } from "./realtime";

function makeEnvelope(eventType: string, deviceId: string, inner: Record<string, unknown>) {
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

describe("extractDevice", () => {
  it("returns connected=true for device.connected", () => {
    const dev = extractDevice(
      makeEnvelope("device.connected", "pseudo-aa", {
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
      makeEnvelope("device.disconnected", "pseudo-bb", {
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
      makeEnvelope("device.signal_changed", "pseudo-aa", {
        old_signal: -60,
        new_signal: -55,
      })
    );
    expect(dev?.connected).toBe(true);
    expect(dev?.last_signal).toBe(-55);
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

describe("extractPoint", () => {
  it("creates a point from device.connected rssi", () => {
    const point = extractPoint(
      makeEnvelope("device.connected", "pseudo-aa", { rssi: -50 })
    );
    expect(point).not.toBeNull();
    expect(point?.pseudonym).toBe("pseudo-aa");
    expect(point?.rssi).toBe(-50);
    expect(point?.ts).toBe(1_700_000_000);
  });

  it("uses last_signal for device.disconnected", () => {
    const point = extractPoint(
      makeEnvelope("device.disconnected", "pseudo-bb", { last_signal: -90 })
    );
    expect(point?.rssi).toBe(-90);
  });

  it("returns null when no signal is present", () => {
    const point = extractPoint(makeEnvelope("device.band_changed", "pseudo-cc", {}));
    expect(point).toBeNull();
  });
});
