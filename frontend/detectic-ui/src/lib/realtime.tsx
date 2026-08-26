import {
  createContext,
  useContext,
  useEffect,
  useState,
} from "react";
import type { Device, Network, TimelinePoint } from "@/lib/api";

export type LiveEvent = {
  type: string;
  sensor_id?: string;
  payload?: unknown;
  observed_at?: number;
  server_time?: number;
};

type RealtimeState = {
  status: "conectando" | "en línea" | "desconectado";
  events: LiveEvent[];
  points: TimelinePoint[];
  devices: Map<string, Device>;
  networks: Map<string, Network>;
};

const RealtimeContext = createContext<RealtimeState | null>(null);

function wsUrl() {
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${window.location.host}/ws?role=frontend&sensor_id=*`;
}

// Event envelope broadcast by the backend:
// {
//   type: "broadcast",
//   sensor_id: "...",
//   server_time: 123,
//   payload: {
//     event_id: "...",
//     sequence: 1,
//     sensor_id: "...",
//     timestamp: 1234567890,
//     type: "device.connected",
//     device_id: "...",
//     payload: { rssi, band, hostname, ... }
//   }
// }
//
// `extractDevice` must read the outer event type (not the inner payload)
// to determine whether a device connected or disconnected.

function parseOuterPayload(event: any) {
  const outer = event?.payload || {};
  const inner = outer?.payload || {};
  return { outer, inner, type: String(outer.type || outer.event_type || "") };
}

export function extractDevice(event: any): Device | null {
  const { outer, inner, type } = parseOuterPayload(event);
  if (!type.startsWith("device.")) return null;

  const deviceId = String(
    outer.device_id ||
      inner.pseudonym ||
      outer.pseudonym ||
      event?.payload?.device_id ||
      "unknown"
  );
  if (deviceId === "unknown") return null;

  const rssi =
    inner.rssi != null
      ? Number(inner.rssi)
      : inner.new_signal != null
      ? Number(inner.new_signal)
      : inner.last_signal != null
      ? Number(inner.last_signal)
      : inner.signal != null
      ? Number(inner.signal)
      : undefined;

  const connected = type === "device.disconnected" ? false : true;

  const observedAt =
    typeof outer.timestamp === "number"
      ? outer.timestamp * 1000
      : event?.observed_at ||
        event?.payload?.observed_at ||
        event?.server_time ||
        Date.now();

  return {
    device_id: deviceId,
    connected,
    last_signal: rssi,
    sensor_id: outer.sensor_id || event?.sensor_id,
    last_seen: observedAt,
    event_count: 1,
    last_type: type,
    hostname: inner.hostname,
    band: inner.band || inner.new_band || inner.old_band,
  };
}

export function extractPoint(event: any): TimelinePoint | null {
  const { outer, inner, type } = parseOuterPayload(event);
  if (!type.startsWith("device.")) return null;

  const rssi =
    inner.rssi != null
      ? Number(inner.rssi)
      : inner.new_signal != null
      ? Number(inner.new_signal)
      : inner.last_signal != null
      ? Number(inner.last_signal)
      : null;
  if (rssi == null) return null;

  const observedAt =
    typeof outer.timestamp === "number"
      ? outer.timestamp * 1000
      : event?.observed_at ||
        event?.payload?.observed_at ||
        event?.server_time ||
        Date.now();

  return {
    pseudonym: String(
      outer.device_id ||
        inner.pseudonym ||
        outer.pseudonym ||
        event?.payload?.device_id ||
        "unknown"
    ),
    rssi,
    band: inner.band || inner.new_band || inner.old_band,
    bssid_pseudonym: inner.bssid_pseudonym,
    ts: Math.floor(observedAt / 1000),
  };
}

export function RealtimeProvider({ children }: { children: React.ReactNode }) {
  const [status, setStatus] = useState<RealtimeState["status"]>("conectando");
  const [events, setEvents] = useState<LiveEvent[]>([]);
  const [points, setPoints] = useState<TimelinePoint[]>([]);
  const [devices, setDevices] = useState<Map<string, Device>>(new Map());
  const [networks, setNetworks] = useState<Map<string, Network>>(new Map());

  useEffect(() => {
    let alive = true;
    let ws: WebSocket | null = null;

    const connect = () => {
      if (!alive) return;
      ws = new WebSocket(wsUrl());

      ws.onopen = () => {
        setStatus("en línea");
        ws?.send(JSON.stringify({ type: "subscribe", sensor_id: "*" }));
      };

      ws.onmessage = (e) => {
        try {
          const msg = JSON.parse(e.data);
          if (
            msg.type === "hello_ack" ||
            msg.type === "subscribe_ack" ||
            msg.type === "pong"
          ) {
            return;
          }

          const live: LiveEvent = {
            type: msg.type || "broadcast",
            sensor_id: msg.sensor_id,
            payload: msg.payload,
            observed_at: msg.observed_at,
            server_time: msg.server_time,
          };

          setEvents((prev) => [live, ...prev].slice(0, 50));

          setPoints((prev) => {
            const point = extractPoint(msg);
            return point ? [point, ...prev].slice(0, 200) : prev;
          });

          setDevices((prev) => {
            const dev = extractDevice(msg);
            if (!dev) return prev;
            const next = new Map(prev);
            const existing = next.get(dev.device_id);
            next.set(dev.device_id, {
              ...existing,
              ...dev,
              first_seen: existing?.first_seen ?? dev.last_seen,
              event_count: (existing?.event_count ?? 0) + 1,
              last_signal: dev.last_signal ?? existing?.last_signal,
            });
            return next;
          });

          setNetworks((prev) => {
            const { outer, inner } = parseOuterPayload(msg);
            const apId = String(
              outer.ap_id ||
                outer.device_id ||
                inner.ap_id ||
                inner.bssid_pseudonym ||
                outer.pseudonym ||
                "unknown"
            );
            if (apId === "unknown" || !String(outer.type || "").startsWith("network.")) {
              return prev;
            }
            const next = new Map(prev);
            const existing = next.get(apId);
            next.set(apId, {
              ...existing,
              ap_id: apId,
              ssid: inner.ssid || existing?.ssid,
              status: inner.status || existing?.status,
              last_signal:
                inner.rssi != null
                  ? Number(inner.rssi)
                  : inner.new_signal != null
                  ? Number(inner.new_signal)
                  : inner.last_signal != null
                  ? Number(inner.last_signal)
                  : existing?.last_signal,
              band: inner.band || existing?.band,
              sensor_id: outer.sensor_id || msg.sensor_id || existing?.sensor_id,
            } as Network);
            return next;
          });
        } catch {
          /* ignore malformed */
        }
      };

      ws.onerror = () => setStatus("desconectado");

      ws.onclose = () => {
        setStatus("desconectado");
        if (alive) setTimeout(connect, 5000);
      };
    };

    connect();
    return () => {
      alive = false;
      ws?.close();
    };
  }, []);

  const state: RealtimeState = {
    status,
    events,
    points,
    devices,
    networks,
  };

  return (
    <RealtimeContext.Provider value={state}>
      {children}
    </RealtimeContext.Provider>
  );
}

export function useRealtime() {
  const ctx = useContext(RealtimeContext);
  if (!ctx) throw new Error("useRealtime must be inside RealtimeProvider");
  return ctx;
}
