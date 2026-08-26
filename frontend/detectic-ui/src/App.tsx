import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Map as MapComponent,
  MapControls,
  MapMarker,
  MarkerContent,
  MarkerPopup,
} from "@/components/ui/map";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { DashboardCharts } from "@/components/charts";
import { DeviceTable } from "@/components/device-table";
import { NetworkTable } from "@/components/network-table";
import { LiveFeed } from "@/components/live-feed";
import { ConnectedDevices } from "@/components/connected-devices";
import { RssiTimelineChart } from "@/components/rssi-timeline-chart";
import { ActivityTimelineChart } from "@/components/activity-timeline-chart";
import { DeviceClassChart } from "@/components/device-class-chart";
import { SignalProximityChart } from "@/components/signal-proximity-chart";
import { useRealtime } from "@/lib/realtime";
import { sourceColor } from "@/lib/location";
import {
  fetchSensors,
  fetchDevices,
  fetchNetworks,
  fetchStats,
  fetchAllDevices,
  fetchTimeline,
  type Stats,
  type Device,
  type DetailedDevice,
  type Network,
  type Sensor,
  type Timeline,
} from "@/lib/api";

function Loading() {
  return (
    <div className="flex h-full items-center justify-center text-muted-foreground">
      Cargando…
    </div>
  );
}

function ErrorMessage({ error }: { error?: Error | null }) {
  return (
    <div className="rounded-lg border border-destructive bg-destructive/10 p-4 text-sm text-destructive">
      {error?.message ?? "Error desconocido"}
    </div>
  );
}

function StatCard({
  title,
  value,
  sub,
}: {
  title: string;
  value: React.ReactNode;
  sub?: string;
}) {
  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          {title}
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="text-3xl font-bold">{value}</div>
        {sub && <p className="text-xs text-muted-foreground">{sub}</p>}
      </CardContent>
    </Card>
  );
}

function useDashboardData() {
  const stats = useQuery<Stats>({ queryKey: ["stats"], queryFn: fetchStats });
  const devices = useQuery<Device[]>({
    queryKey: ["devices"],
    queryFn: fetchDevices,
  });
  const networks = useQuery<Network[]>({
    queryKey: ["networks"],
    queryFn: fetchNetworks,
  });
  const allDevices = useQuery<DetailedDevice[]>({
    queryKey: ["all-devices"],
    queryFn: fetchAllDevices,
  });
  const timeline = useQuery<Timeline>({
    queryKey: ["timeline"],
    queryFn: fetchTimeline,
  });
  const sensors = useQuery<Sensor[]>({
    queryKey: ["sensors"],
    queryFn: fetchSensors,
  });
  return { stats, devices, networks, allDevices, timeline, sensors };
}

function mergeLive<T extends { device_id?: string; ap_id?: string }>(
  fetched: T[],
  live: Map<string, T>
): T[] {
  const map = new Map<string, T>();
  for (const d of fetched) {
    const key = d.device_id || d.ap_id || "";
    if (key) map.set(key, d);
  }
  for (const [key, d] of live) {
    map.set(key, { ...map.get(key), ...d } as T);
  }
  return Array.from(map.values()).sort((a, b) => {
    const ta = (a as any).last_seen ?? 0;
    const tb = (b as any).last_seen ?? 0;
    return tb - ta;
  });
}

export function DashboardView() {
  const live = useRealtime();
  const { stats, devices, networks, allDevices, timeline, sensors } =
    useDashboardData();

  if (
    stats.isLoading ||
    devices.isLoading ||
    networks.isLoading ||
    allDevices.isLoading ||
    timeline.isLoading ||
    sensors.isLoading
  ) {
    return <Loading />;
  }

  const error =
    stats.error ||
    devices.error ||
    networks.error ||
    allDevices.error ||
    timeline.error ||
    sensors.error;
  if (error) {
    return <ErrorMessage error={error as Error} />;
  }

  const s = stats.data || {};
  const fetchedDevs = devices.data || [];
  const fetchedNets = networks.data || [];
  const allSensors = sensors.data || [];
  const detailed = allDevices.data || [];
  const fetchedPoints = timeline.data?.points || [];

  const liveDevs = useMemo(
    () => mergeLive(fetchedDevs, live.devices),
    [fetchedDevs, live.devices]
  );

  const liveNets = useMemo(
    () => mergeLive(fetchedNets, live.networks),
    [fetchedNets, live.networks]
  );

  const livePoints = useMemo(
    () =>
      [...fetchedPoints, ...live.points]
        .sort((a, b) => a.ts - b.ts)
        .slice(-500),
    [fetchedPoints, live.points]
  );

  const offline = liveDevs.filter((d) => !d.connected).length;

  return (
    <div className="space-y-4 md:space-y-6">
      <ConnectedDevices devices={liveDevs} />

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5">
        <StatCard title="No conectados" value={offline} sub="en las últimas 24h" />
        <StatCard
          title="Dispositivos detectados"
          value={s.distinct_devices ?? "—"}
          sub={`${s.identified_devices ?? 0} identificados`}
        />
        <StatCard
          title="APs detectadas"
          value={s.total_networks ?? "—"}
          sub="señales Wi-Fi"
        />
        <StatCard title="Detecciones" value={s.total_detections ?? "—"} sub="eventos" />
        <StatCard title="Sensores" value={s.total_sensors ?? "—"} sub="activos" />
        <StatCard
          title="RSSI medio"
          value={s.avg_rssi != null ? `${s.avg_rssi} dBm` : "—"}
          sub="señal (dBm)"
        />
        <StatCard
          title="MAC aleatoria"
          value={s.randomized_macs ?? "—"}
          sub="privacidad MAC"
        />
        <StatCard
          title="Vendores"
          value={s.known_vendors ?? "—"}
          sub={`${s.snapshots_last_hour ?? 0} snapshots/h`}
        />
        <StatCard
          title="Snapshots"
          value={s.total_snapshots ?? "—"}
          sub={`${s.snapshots_last_day ?? 0} en 24h`}
        />
      </div>

      <DashboardCharts
        devices={liveDevs}
        networks={liveNets}
        sensors={allSensors}
      />

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <SignalProximityChart devices={liveDevs} />
        <RssiTimelineChart points={livePoints} />
      </div>

      <ActivityTimelineChart points={livePoints} />

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <DeviceClassChart devices={detailed} />
      </div>

      <LiveFeed />

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <DeviceTable devices={liveDevs} />
        <NetworkTable networks={liveNets} />
      </div>
    </div>
  );
}

export function MapView() {
  const { data, isLoading, error } = useQuery({
    queryKey: ["sensors"],
    queryFn: fetchSensors,
  });

  if (isLoading) return <Loading />;
  if (error) return <ErrorMessage error={error as Error} />;

  const knownSensors = (data || []).filter((s) => s.location?.latitude != null);

  return (
    <Card className="overflow-hidden">
      <CardHeader>
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Mapa RF
        </CardTitle>
      </CardHeader>
      <CardContent className="p-0">
        <div className="h-[70vh] w-full">
          <MapComponent
            theme="dark"
            className="h-full w-full"
            viewport={{
              center: [-49.35, -28.68],
              zoom: 14,
            }}
          >
            <MapControls />
            {knownSensors.map((s) =>
              s.location?.latitude != null && s.location.longitude != null ? (
                <MapMarker
                  key={s.id}
                  latitude={s.location.latitude}
                  longitude={s.location.longitude}
                >
                  <MarkerContent>
                    <div className="flex items-center gap-2">
                      <div
                        className={`h-3 w-3 rounded-full border border-white ${sourceColor(
                          s.location.source
                        )}`}
                      />
                      <span className="text-xs font-medium text-foreground">
                        {s.name || s.id}
                      </span>
                    </div>
                  </MarkerContent>
                  <MarkerPopup>
                    <div className="min-w-[180px] p-2">
                      <div className="mb-1 font-semibold">{s.name || s.id}</div>
                      <div className="text-xs text-muted-foreground">
                        Fuente: <Badge variant="secondary">{s.location.source}</Badge>
                      </div>
                      <div className="text-xs text-muted-foreground">
                        Precisión: ~{s.location.accuracy_m ?? "?"} m
                      </div>
                      {s.public_ip && (
                        <div className="text-xs text-muted-foreground">IP: {s.public_ip}</div>
                      )}
                    </div>
                  </MarkerPopup>
                </MapMarker>
              ) : null
            )}
          </MapComponent>
        </div>
      </CardContent>
    </Card>
  );
}

// Keep a default export for backward compatibility; it renders the dashboard.
export default function App() {
  return <DashboardView />;
}
