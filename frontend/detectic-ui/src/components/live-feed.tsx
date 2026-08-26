import { useMemo } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { useRealtime } from "@/lib/realtime";
import { Wifi, Smartphone, Activity, AlertTriangle } from "lucide-react";

function timeAgo(ms?: number) {
  if (ms == null) return "—";
  const diff = Math.floor(Date.now() - ms) / 1000;
  if (diff < 60) return `${Math.floor(diff)}s`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h`;
  return `${Math.floor(diff / 86400)}d`;
}

function eventLabel(type: string) {
  if (type.includes("device.connected")) return "Device connected";
  if (type.includes("device.disconnected")) return "Device disconnected";
  if (type.includes("device")) return "Device observed";
  if (type.includes("network.detected")) return "AP observed";
  if (type.includes("network.disappeared")) return "AP disappeared";
  if (type.includes("network")) return "AP event";
  return type || "Event";
}

function eventIcon(type: string) {
  if (type.includes("device")) return <Smartphone className="h-3.5 w-3.5" />;
  if (type.includes("network")) return <Wifi className="h-3.5 w-3.5" />;
  if (type.includes("error")) return <AlertTriangle className="h-3.5 w-3.5" />;
  return <Activity className="h-3.5 w-3.5" />;
}

export function LiveFeed() {
  const { events, status } = useRealtime();

  const rendered = useMemo(() => {
    return events.slice(0, 20).map((e) => {
      const outer = (e.payload as any) || {};
      const inner = outer?.payload || {};
      const type = outer.type || outer.event_type || e.type;
      const label = eventLabel(type);
      const id = String(
        outer.device_id ||
          outer.ap_id ||
          inner.device_id ||
          inner.pseudonym ||
          inner.ap_id ||
          inner.bssid_pseudonym ||
          outer.pseudonym ||
          "—"
      ).slice(0, 24);
      const rssi =
        inner.rssi != null
          ? `${inner.rssi} dBm`
          : inner.new_signal != null
          ? `${inner.new_signal} dBm`
          : inner.last_signal != null
          ? `${inner.last_signal} dBm`
          : null;
      const band = inner.band ? `· ${inner.band}` : "";
      return {
        label,
        id,
        rssi,
        band,
        time: e.server_time || outer.timestamp * 1000 || e.observed_at,
        type,
      };
    });
  }, [events]);

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Live Events
        </CardTitle>
        <div className="text-[10px] text-muted-foreground">
          {status === "en línea" ? (
            <Badge variant="default" className="text-[10px]">
              {status}
            </Badge>
          ) : (
            <Badge variant="secondary" className="text-[10px]">
              {status}
            </Badge>
          )}
        </div>
      </CardHeader>
      <CardContent>
        <div className="max-h-[320px] space-y-2 overflow-y-auto pr-2">
          {rendered.length === 0 && (
            <div className="py-6 text-center text-sm text-muted-foreground">
              Esperando eventos WebSocket…
            </div>
          )}
          {rendered.map((e, i) => (
            <div
              key={i}
              className="flex items-start gap-3 rounded-md border border-border bg-muted/40 p-2.5 text-xs"
            >
              <div className="mt-0.5 text-muted-foreground">{eventIcon(e.type)}</div>
              <div className="flex-1">
                <div className="font-medium">{e.label}</div>
                <div className="font-mono text-muted-foreground">{e.id}</div>
                <div className="mt-0.5 text-muted-foreground">
                  {e.rssi && <span className="mr-2">{e.rssi}</span>}
                  {e.band && <span className="mr-2">{e.band}</span>}
                  <span className="tabular-nums">{timeAgo(e.time)}</span>
                </div>
              </div>
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}
