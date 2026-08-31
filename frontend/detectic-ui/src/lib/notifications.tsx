import { createContext, useContext, useEffect, useRef, useState } from "react";
import { useRealtime } from "@/lib/realtime";

/**
 * Global notification state: a running unread counter for device/AP alerts.
 *
 * The bell in the topbar shows `unread`. Visiting the Notifications page calls
 * `markAllRead()`, which records the timestamp and zeroes the counter.
 */

const STORAGE_KEY = "detectic.lastReadAt";

type NotificationsState = {
  unread: number;
  markAllRead: () => void;
};

const NotificationsContext = createContext<NotificationsState | null>(null);

const ALERT_TYPES = new Set([
  "device.connected",
  "device.disconnected",
  "device.presence_changed",
  "device.band_changed",
  "network.detected",
  "network.disappeared",
  "network.changed",
]);

function lastReadAt(): number {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    const n = raw ? parseInt(raw, 10) : 0;
    return Number.isFinite(n) ? n : 0;
  } catch {
    return 0;
  }
}

function storeLastReadAt(ts: number) {
  try {
    localStorage.setItem(STORAGE_KEY, String(ts));
  } catch {
    /* ignore */
  }
}

export function NotificationsProvider({ children }: { children: React.ReactNode }) {
  const { events } = useRealtime();
  const [unread, setUnread] = useState(0);
  const counted = useRef<Set<string>>(new Set());
  const baselineDone = useRef(false);

  // One-time baseline: count already-persisted alert events newer than lastReadAt.
  useEffect(() => {
    if (baselineDone.current) return;
    baselineDone.current = true;
    const cutoff = lastReadAt();
    if (!cutoff) return;
    fetch("/api/v1/events?hours=24&limit=250")
      .then((r) => (r.ok ? r.json() : { events: [] }))
      .then(({ events: evs }: { events: any[] }) => {
        const fresh = (evs || []).filter((e) => {
          const ts = (Number(e.event_timestamp) || 0) * 1000;
          return ts > cutoff && ALERT_TYPES.has(String(e.event_type));
        });
        if (fresh.length > 0) setUnread((u) => u + fresh.length);
      })
      .catch(() => {
        /* offline: ignore */
      });
  }, []);

  // Live: count new alerts arriving over the WebSocket.
  useEffect(() => {
    const cutoff = lastReadAt();
    for (const ev of events) {
      const payload = (ev.payload as any) || {};
      const inner = payload?.payload || {};
      const type = String(
        payload.type === "event" && inner?.type ? inner.type : payload.type || ev.type
      );
      if (!ALERT_TYPES.has(type)) continue;
      const ts = ev.server_time || ev.observed_at || 0;
      if (cutoff && ts <= cutoff) continue;
      const id = String(
        payload.device_id ||
          payload.ap_id ||
          inner.device_id ||
          inner.bssid_pseudonym ||
          inner.ap_id ||
          ""
      );
      const key = `${type}::${id}::${ts}`;
      if (counted.current.has(key)) continue;
      counted.current.add(key);
      setUnread((u) => u + 1);
    }
  }, [events]);

  const markAllRead = () => {
    storeLastReadAt(Date.now());
    setUnread(0);
  };

  return (
    <NotificationsContext.Provider value={{ unread, markAllRead }}>
      {children}
    </NotificationsContext.Provider>
  );
}

export function useNotifications() {
  const ctx = useContext(NotificationsContext);
  if (!ctx) throw new Error("useNotifications must be inside NotificationsProvider");
  return ctx;
}
