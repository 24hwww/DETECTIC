import { useMemo, useState, useEffect } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
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
  fetchReportConfig,
  updateReportConfig,
  type Stats,
  type Device,
  type DetailedDevice,
  type Network,
  type ReportConfig,
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

function ReportConfigPanel() {
  const queryClient = useQueryClient();
  const { data: config, isLoading } = useQuery<ReportConfig | null>({
    queryKey: ["report-config"],
    queryFn: fetchReportConfig,
  });

  const [enabled, setEnabled] = useState(false);
  const [frequency, setFrequency] = useState(24);
  const [changesOnly, setChangesOnly] = useState(false);
  const [topDevices, setTopDevices] = useState(5);
  const [newDetections, setNewDetections] = useState(true);
  const [nearbyAps, setNearbyAps] = useState(true);
  const [emailTo, setEmailTo] = useState("");
  const [emailSubject, setEmailSubject] = useState("");

  useEffect(() => {
    if (!config) return;
    setEnabled(Boolean(config.enabled));
    setFrequency(config.frequency_hours);
    setChangesOnly(Boolean(config.changes_only));
    setTopDevices(config.top_devices);
    setNewDetections(Boolean(config.new_detections));
    setNearbyAps(Boolean(config.nearby_aps));
    setEmailTo(config.email_to || "");
    setEmailSubject(config.email_subject || "");
  }, [config]);

  const save = useMutation({
    mutationFn: () =>
      updateReportConfig({
        enabled: enabled ? 1 : 0,
        frequency_hours: frequency,
        changes_only: changesOnly ? 1 : 0,
        top_devices: topDevices,
        new_detections: newDetections ? 1 : 0,
        nearby_aps: nearbyAps ? 1 : 0,
        email_to: emailTo.trim() || null,
        email_subject: emailSubject.trim() || null,
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["report-config"] }),
  });

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Configuración de informes por email
        </CardTitle>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <p className="text-sm text-muted-foreground">Cargando…</p>
        ) : (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={enabled}
                onChange={(e) => setEnabled(e.target.checked)}
              />
              <span className="text-sm">Informes automáticos activados</span>
            </label>
            <div className="space-y-1">
              <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                Frecuencia (horas)
              </span>
              <select
                value={frequency}
                onChange={(e) => setFrequency(Number(e.target.value))}
                className="h-9 w-full rounded-md border border-input bg-background px-2 text-sm"
              >
                <option value={1}>Cada 1 hora</option>
                <option value={6}>Cada 6 horas</option>
                <option value={12}>Cada 12 horas</option>
                <option value={24}>Diario</option>
                <option value={168}>Semanal</option>
              </select>
            </div>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={changesOnly}
                onChange={(e) => setChangesOnly(e.target.checked)}
              />
              <span className="text-sm">Solo novedades</span>
            </label>
            <div className="space-y-1">
              <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                Top dispositivos
              </span>
              <input
                type="number"
                min={0}
                max={20}
                value={topDevices}
                onChange={(e) => setTopDevices(Number(e.target.value))}
                className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
              />
            </div>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={newDetections}
                onChange={(e) => setNewDetections(e.target.checked)}
              />
              <span className="text-sm">Incluir nuevas detecciones</span>
            </label>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={nearbyAps}
                onChange={(e) => setNearbyAps(e.target.checked)}
              />
              <span className="text-sm">Solo APs cercanos</span>
            </label>
            <div className="space-y-1 sm:col-span-2">
              <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                Destinatarios (separados por coma)
              </span>
              <input
                type="text"
                value={emailTo}
                onChange={(e) => setEmailTo(e.target.value)}
                placeholder="a@ejemplo.com, b@ejemplo.com"
                className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
              />
            </div>
            <div className="space-y-1 sm:col-span-2">
              <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                Asunto
              </span>
              <input
                type="text"
                value={emailSubject}
                onChange={(e) => setEmailSubject(e.target.value)}
                placeholder="[Detectic] Informe de observación"
                className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
              />
            </div>
            <div className="flex items-center gap-3 sm:col-span-2">
              <Button size="sm" onClick={() => save.mutate()} disabled={save.isPending}>
                {save.isPending ? "Guardando…" : "Guardar"}
              </Button>
              <a
                href="/api/v1/reports/email"
                target="_blank"
                rel="noreferrer"
                className="text-sm text-primary hover:underline"
              >
                Vista previa del informe
              </a>
              {save.isSuccess && <span className="text-xs text-green-600">Guardado</span>}
              {save.isError && <span className="text-xs text-destructive">Error</span>}
            </div>
          </div>
        )}
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
        title="Reportes"
        description="Resumen histórico y métricas de 24h"
      />

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <KpiCard
          title="Dispositivos detectados"
          value={s.distinct_devices ?? "—"}
          sub={`${s.identified_devices ?? 0} identificados`}
        />
        <KpiCard
          title="APs detectados"
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

      <ReportConfigPanel />
    </div>
  );
}
