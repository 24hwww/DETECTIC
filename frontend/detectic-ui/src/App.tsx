import { useQuery } from "@tanstack/react-query";
import {
  Map,
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
import { RssiTimelineChart } from "@/components/rssi-timeline-chart";
import { ActivityTimelineChart } from "@/components/activity-timeline-chart";
import { DeviceClassChart } from "@/components/device-class-chart";
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

export function DashboardView() {
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
  const allDevs = devices.data || [];
  const allNets = networks.data || [];
  const allSensors = sensors.data || [];
  const detailed = allDevices.data || [];
  const points = timeline.data?.points || [];

  const online = allDevs.filter((d) => d.connected).length;
  const offline = allDevs.length - online;

  return (
    <div className="space-y-6">
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-5">
        <StatCard title="Conectados" value={online} sub="en las últimas 24h" />
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
      </div>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-5">
        <StatCard title="Sensores" value={s.total_sensors ?? "—"} sub="activos" />
        <StatCard
          title="RSSI medio"
          value={s.avg_rssi != null ? `${s.avg_rssi} dBm` : "—"}
          sub="señal (dBm)"
        />
        <StatCard
          title="MAC aleatoria"
          value={s.randomized_macs ?? "—"}
          sub="dispositivos con privacidad MAC"
        />
        <StatCard
          title="Vendores"
          value={s.known_vendors ?? "—"}
          sub={`${s.snapshots_last_hour ?? 0} snapshots última hora`}
        />
        <StatCard
          title="Snapshots"
          value={s.total_snapshots ?? "—"}
          sub={`${s.snapshots_last_day ?? 0} en 24h`}
        />
      </div>

      <DashboardCharts
        devices={allDevs}
        networks={allNets}
        sensors={allSensors}
      />

      <RssiTimelineChart points={points} />

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <ActivityTimelineChart points={points} />
        <DeviceClassChart devices={detailed} />
      </div>

      <LiveFeed />

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <DeviceTable devices={allDevs} />
        <NetworkTable networks={allNets} />
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
          <Map
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
          </Map>
        </div>
      </CardContent>
    </Card>
  );
}

// Keep a default export for backward compatibility; it renders the dashboard.
export default function App() {
  return <DashboardView />;
}
