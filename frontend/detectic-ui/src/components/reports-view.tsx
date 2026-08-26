import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { PageHeader } from "@/components/page-header";
import { ActivityTimelineChart } from "@/components/activity-timeline-chart";
import { RssiTimelineChart } from "@/components/rssi-timeline-chart";
import { DeviceClassChart } from "@/components/device-class-chart";
import { SignalProximityChart } from "@/components/signal-proximity-chart";
import { useRealtime } from "@/lib/realtime";
import { mergeLive } from "@/lib/merge";
import {
  fetchStats,
  fetchDevices,
  fetchAllDevices,
  fetchTimeline,
  fetchNetworks,
  type Stats,
  type Device,
  type DetailedDevice,
  type Network,
} from "@/lib/api";

function KpiCard({
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
        <div className="text-3xl font-bold tabular-nums">{value}</div>
        {sub && <p className="text-xs text-muted-foreground">{sub}</p>}
      </CardContent>
    </Card>
  );
}

export function ReportsView() {
  const live = useRealtime();
  const { data: stats, isLoading: l1 } = useQuery<Stats>({
    queryKey: ["stats"],
    queryFn: fetchStats,
  });
  const { data: devices, isLoading: l2 } = useQuery<Device[]>({
    queryKey: ["devices"],
    queryFn: fetchDevices,
  });
  const { data: allDevices, isLoading: l3 } = useQuery<DetailedDevice[]>({
    queryKey: ["all-devices"],
    queryFn: fetchAllDevices,
  });
  const { data: timeline, isLoading: l4 } = useQuery({
    queryKey: ["timeline"],
    queryFn: fetchTimeline,
  });
  const { data: networks, isLoading: l5 } = useQuery<Network[]>({
    queryKey: ["networks"],
    queryFn: fetchNetworks,
  });

  const liveDevs = useMemo(
    () => mergeLive(devices || [], live.devices),
    [devices, live.devices]
  );

  const liveNets = useMemo(
    () => mergeLive(networks || [], live.networks),
    [networks, live.networks]
  );

  const livePoints = useMemo(
    () =>
      [...(timeline?.points || []), ...live.points]
        .sort((a, b) => a.ts - b.ts)
        .slice(-500),
    [timeline, live.points]
  );

  if (l1 || l2 || l3 || l4 || l5) {
    return <div className="py-12 text-center text-muted-foreground">Cargando…</div>;
  }

  const s = stats || {};

  return (
    <div className="space-y-4 md:space-y-6">
      <PageHeader
        title="Reports"
        description="Resumen histórico y métricas de 24h"
      />

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <KpiCard
          title="Dispositivos detectados"
          value={s.distinct_devices ?? "—"}
          sub={`${s.identified_devices ?? 0} identificados`}
        />
        <KpiCard
          title="APs detectadas"
          value={liveNets.length}
          sub="señales Wi-Fi"
        />
        <KpiCard
          title="Observaciones"
          value={s.total_snapshots ?? "—"}
          sub={`${s.snapshots_last_hour ?? 0} en la última hora`}
        />
        <KpiCard
          title="RSSI medio"
          value={s.avg_rssi != null ? `${s.avg_rssi} dBm` : "—"}
          sub="señal promedio"
        />
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <SignalProximityChart devices={liveDevs} />
        <RssiTimelineChart points={livePoints} />
      </div>

      <ActivityTimelineChart points={livePoints} />

      <DeviceClassChart devices={allDevices || []} />
    </div>
  );
}
