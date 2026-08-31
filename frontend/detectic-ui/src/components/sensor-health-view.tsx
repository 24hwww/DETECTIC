import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { PageHeader } from "@/components/page-header";
import { fetchAllHealth, fetchSensors, type AllHealth, type Sensor } from "@/lib/api";

function fmtDuration(seconds?: number | null) {
  if (seconds == null) return "—";
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

function fmtTime(ts?: number | null) {
  if (ts == null) return "—";
  return new Date(ts * 1000).toLocaleString();
}

export function SensorHealthView() {
  const [hours, setHours] = useState(24);
  const { data: all, isLoading } = useQuery<AllHealth | null>({
    queryKey: ["all-health", hours],
    queryFn: () => fetchAllHealth(hours),
  });
  const { data: sensors } = useQuery<Sensor[]>({
    queryKey: ["sensors"],
    queryFn: fetchSensors,
  });

  const sensorMap = new Map((sensors || []).map((s) => [s.id, s]));

  return (
    <div className="space-y-4 md:space-y-6">
      <PageHeader
        title="Salud del sensor"
        description="Métricas históricas de CPU, memoria, uptime y carga."
      />

      <div className="flex items-center gap-2">
        <span className="text-sm text-muted-foreground">Ventana:</span>
        {[24, 72, 168].map((h) => (
          <button
            key={h}
            onClick={() => setHours(h)}
            className={`rounded-md px-2 py-1 text-xs ${
              hours === h
                ? "bg-primary text-primary-foreground"
                : "bg-muted text-muted-foreground hover:text-foreground"
            }`}
          >
            {h}h
          </button>
        ))}
      </div>

      {isLoading ? (
        <p className="text-sm text-muted-foreground">Cargando…</p>
      ) : !all || all.sensors.length === 0 ? (
        <p className="text-sm text-muted-foreground">No hay métricas de salud recientes.</p>
      ) : (
        <div className="grid gap-4 md:grid-cols-2">
          {all.sensors.map((s) => {
            const sensor = sensorMap.get(s.sensor_id);
            const isFresh = s.last_report && Date.now() / 1000 - s.last_report < 600;
            return (
              <Card key={s.sensor_id}>
                <CardHeader className="pb-2">
                  <CardTitle className="flex items-center justify-between text-xs font-medium uppercase tracking-wide text-muted-foreground">
                    <span>{sensor?.name || s.sensor_id}</span>
                    <Badge
                      variant="outline"
                      className={isFresh ? "bg-[var(--color-online)]/10 text-[var(--color-online)]" : ""}
                    >
                      {isFresh ? "activo" : "inactivo"}
                    </Badge>
                  </CardTitle>
                </CardHeader>
                <CardContent className="space-y-2 text-sm">
                  <div className="grid grid-cols-2 gap-2">
                    <div>
                      <span className="text-[10px] uppercase text-muted-foreground">CPU avg</span>
                      <div>{s.avg_cpu != null ? `${s.avg_cpu}%` : "—"}</div>
                    </div>
                    <div>
                      <span className="text-[10px] uppercase text-muted-foreground">Memoria avg</span>
                      <div>{s.avg_memory != null ? `${s.avg_memory}%` : "—"}</div>
                    </div>
                    <div>
                      <span className="text-[10px] uppercase text-muted-foreground">Muestras</span>
                      <div>{s.samples}</div>
                    </div>
                    <div>
                      <span className="text-[10px] uppercase text-muted-foreground">Uptime máx</span>
                      <div>{fmtDuration(s.max_uptime)}</div>
                    </div>
                  </div>
                  <div className="text-[10px] text-muted-foreground">
                    Último reporte: {fmtTime(s.last_report)}
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>
      )}
    </div>
  );
}
