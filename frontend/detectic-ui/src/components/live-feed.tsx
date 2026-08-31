import { useMemo } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { useRealtime } from "@/lib/realtime";
import { Wifi, Smartphone, Activity, AlertTriangle } from "lucide-react";
import {
  bandLabel,
  deviceName,
  networkName,
  proximityText,
  signalWord,
} from "@/lib/labels";
import type { Device, DetailedDevice, Network } from "@/lib/api";

function timeAgo(ms?: number) {
  if (ms == null) return "—";
  const diff = Math.floor(Date.now() - ms) / 1000;
  if (diff < 60) return `hace ${Math.floor(diff)}s`;
  if (diff < 3600) return `hace ${Math.floor(diff / 60)}m`;
  if (diff < 86400) return `hace ${Math.floor(diff / 3600)}h`;
  return `hace ${Math.floor(diff / 86400)}d`;
}

function eventLabel(type: string) {
  if (type.includes("device.connected"))
    return "Dispositivo conectado";
  if (type.includes("device.disconnected"))
    return "Dispositivo se desconectó";
  if (type.includes("device.presence_changed"))
    return "Cambió la presencia de un dispositivo";
  if (type.includes("device.proximity_changed"))
    return "Un dispositivo cambió de distancia";
  if (type.includes("device.signal_changed"))
    return "Cambió la señal de un dispositivo";
  if (type.includes("device.network_changed") || type.includes("device.band_changed"))
    return "Cambió la banda de un dispositivo";
  if (type.includes("device")) return "Dispositivo observado";
  if (type.includes("network.detected")) return "Red detectada";
  if (type.includes("network.disappeared")) return "Red se perdió de señal";
  if (type.includes("network.changed")) return "Una red cambió";
  if (type.includes("rf.environment_snapshot")) return "Entorno de redes actualizado";
  if (type.includes("network")) return "Evento de red";
  return type || "Evento";
}

function eventIcon(type: string) {
  if (type.includes("device")) return <Smartphone className="h-3.5 w-3.5" />;
  if (type.includes("network")) return <Wifi className="h-3.5 w-3.5" />;
  if (type.includes("error")) return <AlertTriangle className="h-3.5 w-3.5" />;
  return <Activity className="h-3.5 w-3.5" />;
}

function eventName(
  id: string,
  devicesBy: Map<string, Device>,
  networksBy: Map<string, Network>,
  identity: Map<string, DetailedDevice>
): string | undefined {
  const net = networksBy.get(id);
  if (net) return networkName(net);
  const dev = devicesBy.get(id);
  if (dev) return deviceName(dev, identity.get(id));
  return undefined;
}

export function LiveFeed({
  devices,
  networks,
  identity,
}: {
  devices?: Device[];
  networks?: Network[];
  identity?: Map<string, DetailedDevice>;
}) {
  const { events, status } = useRealtime();

  const devicesBy = useMemo(
    () =>
      new Map((devices || []).map((d) => [d.device_id, d])),
    [devices]
  );
  const networksBy = useMemo(
    () =>
      new Map((networks || []).map((n) => [n.ap_id, n])),
    [networks]
  );

  const rendered = useMemo(() => {
    return events.slice(0, 20).map((e) => {
      const outer = (e.payload as any) || {};
      const inner = outer?.payload || {};
      const type =
        outer.type === "event" && inner?.type
          ? inner.type
          : outer.type || outer.event_type || e.type;
      const label = eventLabel(type);
      const id = String(
        outer.device_id ||
          outer.ap_id ||
          inner.device_id ||
          inner.pseudonym ||
          inner.ap_id ||
          inner.bssid_pseudonym ||
          outer.pseudonym ||
          ""
      );
      const name = id
        ? eventName(id, devicesBy, networksBy, identity ?? new Map())
        : undefined;
      const rssi =
        inner.rssi != null
          ? inner.rssi
          : inner.new_signal != null
          ? inner.new_signal
          : inner.last_signal != null
          ? inner.last_signal
          : null;
      const proximity =
        outer.proximity || inner.proximity || inner.proximity_zone_label || null;
      return {
        label,
        id,
        name,
        rssi,
        proximity,
        band: bandLabel(inner.band),
        time: e.server_time || outer.timestamp * 1000 || e.observed_at,
        type,
      };
    });
  }, [events, devicesBy, networksBy, identity]);

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Últimos eventos
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
              Esperando eventos en vivo…
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
                {e.name && (
                  <div className="truncate text-foreground">{e.name}</div>
                )}
                <div className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-muted-foreground">
                  {e.rssi != null && <span>{signalWord(Number(e.rssi))}</span>}
                  {e.band && <span>{e.band}</span>}
                  {e.proximity && <span>{proximityText(String(e.proximity))}</span>}
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
