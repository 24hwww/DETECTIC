import { useMemo } from "react";
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Clock, Signal, Timer, Users } from "lucide-react";
import type { Analytics, Dweller } from "@/lib/api";

function timeLabel(bucket: string) {
  const d = new Date(bucket + "Z");
  if (isNaN(d.getTime())) return bucket;
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function dateLabel(bucket: string) {
  const d = new Date(bucket + "T00:00:00Z");
  if (isNaN(d.getTime())) return bucket;
  return d.toLocaleDateString([], { day: "2-digit", month: "2-digit" });
}

function Kpi({
  icon: Icon,
  label,
  value,
  sub,
}: {
  icon: React.ElementType;
  label: string;
  value: string | number;
  sub?: string;
}) {
  return (
    <Card>
      <CardContent className="flex items-center gap-4 p-4">
        <div className="rounded-xl bg-primary/10 p-3 text-primary">
          <Icon className="h-5 w-5" />
        </div>
        <div>
          <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            {label}
          </p>
          <p className="text-2xl font-bold tabular-nums">{value}</p>
          {sub && <p className="text-xs text-muted-foreground">{sub}</p>}
        </div>
      </CardContent>
    </Card>
  );
}

function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const m = Math.floor(seconds / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ${m % 60}m`;
  const d = Math.floor(h / 24);
  return `${d}d ${h % 24}h`;
}

export function AnalyticsDashboard({ analytics }: { analytics?: Analytics }) {
  const data = analytics;

  const combined = useMemo(() => {
    if (!data) return [];
    return data.connectionTimeline.map((c, i) => ({
      bucket: c.bucket,
      conectados: c.connected,
      desconectados: data.disconnectionTimeline[i]?.disconnected || 0,
      cercanos: data.nearbyTimeline[i]?.nearby || 0,
    }));
  }, [data]);

  const proximity = useMemo(() => {
    if (!data) return [];
    return data.proximityTimeline.map((p) => ({
      bucket: p.bucket,
      inmediato: p.immediate,
      cerca: p.near,
      medio: p.medium,
      lejos: p.far,
      desconocido: p.unknown,
    }));
  }, [data]);

  const rssi = useMemo(() => {
    if (!data) return [];
    return data.rssiTimeline.map((r) => ({
      bucket: r.bucket,
      promedio: r.avg,
      minimo: r.min,
      maximo: r.max,
    }));
  }, [data]);

  if (!data) {
    return (
      <div className="text-sm text-muted-foreground">
        Cargando analytics…
      </div>
    );
  }

  const totals = data.totals;
  const isHourly = data.granularity === "hour";
  const label = isHourly ? timeLabel : dateLabel;

  return (
    <div className="space-y-4 md:space-y-6">
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Kpi
          icon={Users}
          label="Conectados"
          value={totals.total_connected}
          sub={`${totals.total_observed} observados`}
        />
        <Kpi
          icon={Signal}
          label="Eventos cercanos"
          value={totals.total_nearby_events}
          sub="en el período"
        />
        <Kpi
          icon={Timer}
          label="Tiempo promedio"
          value={formatDuration(totals.avg_session_seconds)}
          sub="por sesión"
        />
        <Kpi
          icon={Clock}
          label="Horas de conexión"
          value={totals.total_dwell_hours}
          sub="tiempo total acumulado"
        />
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <Card className="flex flex-col">
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Conexiones y desconexiones por {isHourly ? "hora" : "día"}
            </CardTitle>
          </CardHeader>
          <CardContent className="flex-1">
            <div className="h-[260px] w-full">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={combined} margin={{ top: 8, right: 8, left: 0, bottom: 8 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" />
                  <XAxis dataKey="bucket" tickFormatter={label} tickLine={false} axisLine={false} tickMargin={8} />
                  <YAxis tickLine={false} axisLine={false} allowDecimals={false} />
                  <Tooltip
                    contentStyle={{
                      background: "var(--card)",
                      border: "1px solid var(--border)",
                      borderRadius: "8px",
                    }}
                  />
                  <Legend />
                  <Line
                    type="monotone"
                    dataKey="conectados"
                    stroke="var(--color-online)"
                    strokeWidth={2}
                    dot={false}
                  />
                  <Line
                    type="monotone"
                    dataKey="desconectados"
                    stroke="var(--color-offline)"
                    strokeWidth={2}
                    dot={false}
                  />
                  <Line
                    type="monotone"
                    dataKey="cercanos"
                    stroke="var(--color-primary)"
                    strokeWidth={2}
                    dot={false}
                  />
                </LineChart>
              </ResponsiveContainer>
            </div>
          </CardContent>
        </Card>

        <Card className="flex flex-col">
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Distribución de proximidad
            </CardTitle>
          </CardHeader>
          <CardContent className="flex-1">
            <div className="h-[260px] w-full">
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={proximity} margin={{ top: 8, right: 8, left: 0, bottom: 8 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" />
                  <XAxis dataKey="bucket" tickFormatter={label} tickLine={false} axisLine={false} tickMargin={8} />
                  <YAxis tickLine={false} axisLine={false} allowDecimals={false} />
                  <Tooltip
                    contentStyle={{
                      background: "var(--card)",
                      border: "1px solid var(--border)",
                      borderRadius: "8px",
                    }}
                  />
                  <Legend />
                  <Area
                    type="monotone"
                    dataKey="inmediato"
                    stackId="1"
                    stroke="#3fb950"
                    fill="#3fb950"
                    fillOpacity={0.25}
                  />
                  <Area
                    type="monotone"
                    dataKey="cerca"
                    stackId="1"
                    stroke="#58a6ff"
                    fill="#58a6ff"
                    fillOpacity={0.25}
                  />
                  <Area
                    type="monotone"
                    dataKey="medio"
                    stackId="1"
                    stroke="#d29922"
                    fill="#d29922"
                    fillOpacity={0.25}
                  />
                  <Area
                    type="monotone"
                    dataKey="lejos"
                    stackId="1"
                    stroke="#f85149"
                    fill="#f85149"
                    fillOpacity={0.25}
                  />
                  <Area
                    type="monotone"
                    dataKey="desconocido"
                    stackId="1"
                    stroke="#9aa5b1"
                    fill="#9aa5b1"
                    fillOpacity={0.25}
                  />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </CardContent>
        </Card>

        <Card className="flex flex-col">
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              RSSI promedio, mínimo y máximo
            </CardTitle>
          </CardHeader>
          <CardContent className="flex-1">
            <div className="h-[260px] w-full">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={rssi} margin={{ top: 8, right: 8, left: 0, bottom: 8 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" />
                  <XAxis dataKey="bucket" tickFormatter={label} tickLine={false} axisLine={false} tickMargin={8} />
                  <YAxis domain={[-95, -20]} tickLine={false} axisLine={false} />
                  <Tooltip
                    contentStyle={{
                      background: "var(--card)",
                      border: "1px solid var(--border)",
                      borderRadius: "8px",
                    }}
                  />
                  <Legend />
                  <Line
                    type="monotone"
                    dataKey="promedio"
                    stroke="var(--color-primary)"
                    strokeWidth={2}
                    dot={false}
                  />
                  <Line
                    type="monotone"
                    dataKey="minimo"
                    stroke="var(--color-destructive)"
                    strokeWidth={1.5}
                    dot={false}
                    strokeDasharray="4 4"
                  />
                  <Line
                    type="monotone"
                    dataKey="maximo"
                    stroke="var(--color-online)"
                    strokeWidth={1.5}
                    dot={false}
                    strokeDasharray="4 4"
                  />
                </LineChart>
              </ResponsiveContainer>
            </div>
          </CardContent>
        </Card>

        <Card className="flex flex-col">
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Actividad por hora del día
            </CardTitle>
          </CardHeader>
          <CardContent className="flex-1">
            <div className="h-[260px] w-full">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={data.activityByHour} margin={{ top: 8, right: 8, left: 0, bottom: 8 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" vertical={false} />
                  <XAxis dataKey="hour" tickFormatter={(h) => `${h}h`} tickLine={false} axisLine={false} tickMargin={8} />
                  <YAxis tickLine={false} axisLine={false} allowDecimals={false} />
                  <Tooltip
                    contentStyle={{
                      background: "var(--card)",
                      border: "1px solid var(--border)",
                      borderRadius: "8px",
                    }}
                  />
                  <Bar dataKey="count" fill="var(--color-primary)" radius={[4, 4, 0, 0]} />
                  {totals.peak_hour != null && (
                    <ReferenceLine
                      x={totals.peak_hour}
                      stroke="var(--color-warning)"
                      strokeDasharray="4 4"
                      label={{ value: `pico ${totals.peak_hour}h`, fill: "var(--color-warning)" }}
                    />
                  )}
                </BarChart>
              </ResponsiveContainer>
            </div>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Top dispositivos por tiempo de conexión
          </CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <div className="max-h-[320px] overflow-auto">
            <table className="w-full text-sm">
              <thead className="bg-muted text-muted-foreground">
                <tr>
                  <th className="p-3 text-left font-medium">Dispositivo</th>
                  <th className="p-3 text-left font-medium">Clase</th>
                  <th className="p-3 text-left font-medium">Sesiones</th>
                  <th className="p-3 text-left font-medium">Tiempo total</th>
                  <th className="p-3 text-left font-medium">Última señal</th>
                </tr>
              </thead>
              <tbody>
                {data.topDwellers.map((d: Dweller) => (
                  <tr key={d.device_id} className="border-b border-border last:border-0">
                    <td className="p-3 font-mono text-xs">{d.device_id.slice(0, 18)}</td>
                    <td className="p-3">
                      <Badge variant="secondary" className="text-[10px]">
                        {d.device_class || "Desconocido"}
                      </Badge>
                    </td>
                    <td className="p-3 tabular-nums">{d.sessions}</td>
                    <td className="p-3 tabular-nums">{formatDuration(d.total_seconds)}</td>
                    <td className="p-3 tabular-nums">
                      {d.last_signal != null ? `${d.last_signal} dBm` : "—"}
                    </td>
                  </tr>
                ))}
                {data.topDwellers.length === 0 && (
                  <tr>
                    <td colSpan={5} className="p-6 text-center text-muted-foreground">
                      Aún no hay datos de sesiones
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
