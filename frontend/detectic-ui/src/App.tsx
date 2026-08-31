import { useMemo } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Map as MapComponent,
  MapControls,
  MapMarker,
  MarkerContent,
  MarkerPopup,
} from "@/components/ui/map";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { DeviceTable } from "@/components/device-table";
import { NetworkTable } from "@/components/network-table";
import { LiveFeed } from "@/components/live-feed";
import { ConnectedDevices } from "@/components/connected-devices";
import { LiveNetwork } from "@/components/live-network";
import { PageHeader } from "@/components/page-header";
import { RssiTimelineChart } from "@/components/rssi-timeline-chart";
import { ActivityTimelineChart } from "@/components/activity-timeline-chart";
import { DeviceClassChart } from "@/components/device-class-chart";
import { ProximityRadarChart } from "@/components/proximity-radar-chart";
import { useRealtime } from "@/lib/realtime";
import { mergeLive } from "@/lib/merge";
import { sourceColor } from "@/lib/location";
import {
  fetchSensors,
  fetchDevices,
  fetchNetworks,
  fetchStats,
  fetchAllDevices,
  fetchTimeline,
  fetchAnalytics,
  type Stats,
  type Device,
  type DetailedDevice,
  type Network,
  type Sensor,
  type Timeline,
  type Analytics,
} from "@/lib/api";
import { HeroKpis } from "@/components/hero-kpis";
import { AnalyticsDashboard } from "@/components/analytics-dashboard";
import { LiveToasts } from "@/components/live-toasts";

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

const DASH_POLL_MS = 30_000;

function useDashboardData() {
  const stats = useQuery<Stats>({
    queryKey: ["stats"],
    queryFn: fetchStats,
    refetchInterval: DASH_POLL_MS,
  });
  const devices = useQuery<Device[]>({
    queryKey: ["devices"],
    queryFn: fetchDevices,
    refetchInterval: DASH_POLL_MS,
  });
  const networks = useQuery<Network[]>({
    queryKey: ["networks"],
    queryFn: fetchNetworks,
    refetchInterval: DASH_POLL_MS,
  });
  const allDevices = useQuery<DetailedDevice[]>({
    queryKey: ["all-devices"],
    queryFn: fetchAllDevices,
    refetchInterval: DASH_POLL_MS,
  });
  const timeline = useQuery<Timeline>({
    queryKey: ["timeline"],
    queryFn: fetchTimeline,
    refetchInterval: DASH_POLL_MS,
  });
  const sensors = useQuery<Sensor[]>({
    queryKey: ["sensors"],
    queryFn: fetchSensors,
    refetchInterval: DASH_POLL_MS,
  });
  const analytics = useQuery<Analytics>({
    queryKey: ["analytics"],
    queryFn: () => fetchAnalytics(24),
    refetchInterval: DASH_POLL_MS,
  });
  return { stats, devices, networks, allDevices, timeline, sensors, analytics };
}

export function DashboardView() {
  const queryClient = useQueryClient();
  const live = useRealtime();
  const { stats, devices, networks, allDevices, timeline, sensors, analytics } =
    useDashboardData();

  const fetchedDevs = devices.data || [];
  const fetchedNets = networks.data || [];
  const detailed = allDevices.data || [];
  const fetchedPoints = timeline.data?.points || [];

  const identity = useMemo(
    () => new Map(detailed.map((d) => [d.pseudonym, d])),
    [detailed]
  );

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

  if (
    stats.isLoading ||
    devices.isLoading ||
    networks.isLoading ||
    allDevices.isLoading ||
    timeline.isLoading ||
    sensors.isLoading ||
    analytics.isLoading
  ) {
    return <Loading />;
  }

  const error =
    stats.error ||
    devices.error ||
    networks.error ||
    allDevices.error ||
    timeline.error ||
    sensors.error ||
    analytics.error;
  if (error) {
    return <ErrorMessage error={error as Error} />;
  }

  const connected = liveDevs.filter((d) => d.connected).length;
  const nearby = liveDevs.filter(
    (d) =>
      d.connected &&
      d.proximity &&
      ["immediate", "near"].includes(String(d.proximity).toLowerCase())
  ).length;

  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ["stats"] });
    queryClient.invalidateQueries({ queryKey: ["devices"] });
    queryClient.invalidateQueries({ queryKey: ["networks"] });
    queryClient.invalidateQueries({ queryKey: ["all-devices"] });
    queryClient.invalidateQueries({ queryKey: ["timeline"] });
    queryClient.invalidateQueries({ queryKey: ["sensors"] });
    queryClient.invalidateQueries({ queryKey: ["analytics"] });
  };

  return (
    <div className="space-y-4 md:space-y-6">
      <PageHeader
        title="Panel de control"
        description="Inteligencia de red y RF en tiempo real"
        onRefresh={refresh}
      />

      <LiveToasts
        devices={liveDevs}
        networks={liveNets}
        identity={identity}
      />

      <HeroKpis connected={connected} nearby={nearby} />

      <AnalyticsDashboard analytics={analytics.data || undefined} />

      <LiveNetwork devices={liveDevs} networks={liveNets} identity={identity} />

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <ProximityRadarChart devices={liveDevs} />
        <RssiTimelineChart points={livePoints} />
      </div>

      <ActivityTimelineChart points={livePoints} />

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <DeviceClassChart devices={detailed} />
        <NetworkTable networks={liveNets} />
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <DeviceTable devices={liveDevs} identity={identity} />
        <ConnectedDevices devices={liveDevs} identity={identity} />
      </div>

      <LiveFeed devices={liveDevs} networks={liveNets} identity={identity} />
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
