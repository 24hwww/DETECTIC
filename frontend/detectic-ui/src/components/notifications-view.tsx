import { useEffect, useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Smartphone, Wifi, Info, RefreshCw } from "lucide-react";
import { PageHeader } from "@/components/page-header";
import { useRealtime } from "@/lib/realtime";
import { useNotifications } from "@/lib/notifications";
import {
  fetchEvents,
  fetchAllDevices,
  fetchNetworks,
  fetchSensors,
  fetchHealth,
  type SystemEvent,
} from "@/lib/api";
import {
  bandLabel,
  deviceName,
  deviceSubtitle,
  formatDateTime,
  networkName,
  networkSubtitle,
  proximityText,
  signalWord,
  timeAgo,
} from "@/lib/labels";
import type { Device, DetailedDevice, Network } from "@/lib/api";

const ALERT_TYPES = new Set([
  "device.connected",
  "device.disconnected",
  "device.presence_changed",
  "device.band_changed",
  "network.detected",
  "network.disappeared",
  "network.changed",
]);

function parsePayload(payload_json?: string): Record<string, any> {
  if (!payload_json) return {};
  try {
    const obj = JSON.parse(payload_json);
    if (obj && typeof obj === "object" && obj.payload && typeof obj.payload === "object") {
      return obj.payload;
    }
    return obj || {};
  } catch {
    return {};
  }
}

function alertLabel(type: string): string {
  switch (type) {
    case "device.connected":
      return "Dispositivo conectado";
    case "device.disconnected":
      return "Dispositivo se desconectó";
    case "device.presence_changed":
      return "Cambió la presencia";
    case "device.band_changed":
      return "Cambió de banda";
    case "network.detected":
      return "Red detectada";
    case "network.disappeared":
      return "Red perdió señal";
    case "network.changed":
      return "Una red cambió";
    default:
      return type;
  }
}

export function NotificationsView() {
  const { status: rtStatus } = useRealtime();
  const { markAllRead } = useNotifications();

  const events = useQuery<SystemEvent[]>({
    queryKey: ["notif-events"],
    queryFn: () => fetchEvents(168, 400),
  });
  const devices = useQuery<DetailedDevice[]>({
    queryKey: ["notif-devices"],
    queryFn: fetchAllDevices,
  });
  const networks = useQuery<{ aps?: Network[]; networks?: Network[] }>({
    queryKey: ["notif-networks"],
    queryFn: () =>
      fetchNetworks().then((n) => ({ aps: n, networks: n })),
  });
  const sensors = useQuery({ queryKey: ["notif-sensors"], queryFn: fetchSensors });
  const health = useQuery({ queryKey: ["notif-health"], queryFn: fetchHealth });

  useEffect(() => {
    markAllRead();
  }, [markAllRead]);

  const identity = useMemo(
    () => new Map((devices.data || []).map((d) => [d.pseudonym, d])),
    [devices.data]
  );
  const netMap = useMemo(
    () =>
      new Map(
        (networks.data?.aps || networks.data?.networks || []).map((n) => [
          n.ap_id,
          n,
        ])
      ),
    [networks.data]
  );

  const alerts = useMemo(() => {
    return (events.data || [])
      .filter((e) => ALERT_TYPES.has(e.event_type))
      .slice(0, 300);
  }, [events.data]);

  const sensor = sensors.data?.[0];

  const renderAlert = (e: SystemEvent) => {
    const payload = parsePayload(e.payload_json);
    const isNetwork = e.event_type.startsWith("network.");
    const id = e.device_id || "";
    const icon = isNetwork ? <Wifi className="h-4 w-4" /> : <Smartphone className="h-4 w-4" />;

    let name: string;
    let subtitle: string;
    if (isNetwork) {
      const net: Network = {
        ap_id: id,
        ssid: payload.ssid,
        band: payload.band,
        status: payload.status,
        proximity: payload.proximity,
      };
      name = networkName(netMap.get(id) || net);
      subtitle = networkSubtitle(netMap.get(id) || net);
    } else {
      const dev: Device = {
        device_id: id,
        connected: true,
        hostname: payload.hostname,
        band: payload.band,
        last_signal: payload.rssi_dbm ?? payload.signal ?? null,
        proximity: payload.proximity ?? null,
      };
      const detailed = identity.get(id);
      name = deviceName(dev, detailed);
      subtitle = deviceSubtitle(dev, detailed);
    }

    const detailBits: string[] = [];
    const band = bandLabel(payload.band);
    if (band) detailBits.push(band);
    if (!isNetwork && payload.proximity) detailBits.push(proximityText(String(payload.proximity)));
    if (payload.rssi_dbm != null) detailBits.push(`señal ${signalWord(Number(payload.rssi_dbm))}`);

    return (
      <div
        key={e.event_id}
        className="flex items-start gap-3 rounded-lg border border-border bg-card p-3"
      >
        <div className="mt-0.5 flex h-7 w-7 flex-none items-center justify-center rounded-full bg-muted text-muted-foreground">
          {icon}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
            <span className="text-sm font-semibold text-foreground">
              {alertLabel(e.event_type)}
            </span>
            <Badge
              variant="outline"
              className={
                isNetwork
                  ? "text-[var(--color-primary)]"
                  : "text-[var(--color-online)]"
              }
            >
              {isNetwork ? "red" : "dispositivo"}
            </Badge>
          </div>
          <div className="truncate text-sm text-foreground">{name}</div>
          <div className="text-xs text-muted-foreground">{subtitle}</div>
          <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-muted-foreground">
            <span>{formatDateTime(e.event_timestamp)}</span>
            <span>{timeAgo(e.event_timestamp)}</span>
            {detailBits.length > 0 && <span>{detailBits.join(" · ")}</span>}
          </div>
        </div>
      </div>
    );
  };

  return (
    <div className="space-y-4 md:space-y-6">
      <PageHeader
        title="Notificaciones"
        description="Alertas de dispositivos y redes, y estado del sistema"
      />

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Estado del sistema
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Conexión en vivo</span>
              <Badge
                variant={rtStatus === "en línea" ? "default" : "secondary"}
                className={rtStatus === "en línea" ? "bg-[var(--color-online)]/10 text-[var(--color-online)]" : ""}
              >
                {rtStatus}
              </Badge>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Servidor API</span>
              <Badge
                variant={health.data?.status === "ok" ? "default" : "secondary"}
                className={health.data?.status === "ok" ? "bg-[var(--color-online)]/10 text-[var(--color-online)]" : ""}
              >
                {health.data?.status === "ok" ? "operativo" : "sin datos"}
              </Badge>
            </div>
            {sensor && (
              <div className="flex items-center justify-between">
                <span className="text-muted-foreground">Sensor</span>
                <span className="font-medium">
                  {sensor.name} · {timeAgo(sensor.last_seen)}
                </span>
              </div>
            )}
            {sensor && (
              <div className="text-xs text-muted-foreground">
                {sensor.ap_count ?? 0} APs · {sensor.distinct_devices ?? 0} dispositivos
              </div>
            )}
          </CardContent>
        </Card>

        <Card className="lg:col-span-2">
          <CardHeader className="flex-row items-center justify-between pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Todas las alertas
            </CardTitle>
            <button
              onClick={() => events.refetch()}
              className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
            >
              <RefreshCw className="h-3.5 w-3.5" />
              Actualizar
            </button>
          </CardHeader>
          <CardContent>
            {alerts.length === 0 ? (
              <p className="py-8 text-center text-sm text-muted-foreground">
                {events.isLoading
                  ? "Cargando alertas…"
                  : "Aún no hay alertas registradas."}
              </p>
            ) : (
              <div className="max-h-[70vh] space-y-2 overflow-y-auto pr-1">
                {alerts.map(renderAlert)}
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            ¿Qué cuenta como alerta?
          </CardTitle>
        </CardHeader>
        <CardContent className="flex items-center gap-2 text-xs text-muted-foreground">
          <Info className="h-4 w-4" />
          Se listan conexiones y desconexiones de dispositivos, presencia, cambios de
          banda y redes detectadas/disaparecidas. Se excluyen eventos de ruido (señal y
          proximidad) para no saturar la lista.
        </CardContent>
      </Card>
    </div>
  );
}
