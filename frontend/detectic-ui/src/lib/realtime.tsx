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

function extractDevice(event: any): Device | null {
  const p = event?.payload?.payload || event?.payload || {};
  const deviceId = String(
    p.device_id || p.pseudonym || event?.payload?.device_id || "unknown"
  );
  const rssi =
    p.rssi != null
      ? Number(p.rssi)
      : p.new_signal != null
      ? Number(p.new_signal)
      : undefined;
  const connected = !String(p.type || p.event_type || "").includes(
    "disconnected"
  );
  const observedAt =
    event?.observed_at || event?.payload?.observed_at || Date.now();
  return {
    device_id: deviceId,
    connected,
    last_signal: rssi,
    sensor_id: event?.sensor_id || event?.payload?.sensor_id,
    last_seen: observedAt,
    event_count: 1,
    last_type: String(p.type || p.event_type || ""),
    hostname: p.hostname || undefined,
    band: p.band || undefined,
  };
}

function extractPoint(event: any): TimelinePoint | null {
  const p = event?.payload?.payload || event?.payload || {};
  const rssi =
    p.rssi != null
      ? Number(p.rssi)
      : p.new_signal != null
      ? Number(p.new_signal)
      : null;
  if (rssi == null) return null;
  const observedAt =
    event?.observed_at || event?.payload?.observed_at || Date.now();
  return {
    pseudonym: String(
      p.device_id || p.pseudonym || event?.payload?.device_id || "unknown"
    ),
    rssi,
    band: p.band || undefined,
    bssid_pseudonym: p.bssid_pseudonym || undefined,
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
            const p = msg?.payload?.payload || msg?.payload || {};
            const apId = String(p.ap_id || p.bssid_pseudonym || p.device_id || "unknown");
            if (apId === "unknown") return prev;
            const next = new Map(prev);
            const existing = next.get(apId);
            next.set(apId, {
              ...existing,
              ap_id: apId,
              ssid: p.ssid || existing?.ssid,
              status: p.status || existing?.status,
              last_signal:
                p.rssi != null
                  ? Number(p.rssi)
                  : p.new_signal != null
                  ? Number(p.new_signal)
                  : existing?.last_signal,
              band: p.band || existing?.band,
              sensor_id: msg.sensor_id || existing?.sensor_id,
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
