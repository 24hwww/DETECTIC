import { useMemo } from "react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Pie,
  PieChart,
  XAxis,
  YAxis,
} from "recharts";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart";
import type { Device, Network, Sensor } from "@/lib/api";

type DashboardChartsProps = {
  devices: Device[];
  networks: Network[];
  sensors: Sensor[];
};

function getSignalQuality(rssi?: number) {
  if (rssi == null) return "desconocido";
  if (rssi > -50) return "excelente";
  if (rssi > -60) return "bueno";
  if (rssi > -70) return "medio";
  if (rssi > -80) return "débil";
  return "muy débil";
}

export function DashboardCharts({
  devices,
  networks,
  sensors,
}: DashboardChartsProps) {
  const statusData = useMemo(
    () => [
      {
        name: "online",
        value: devices.filter((d) => d.connected).length,
        fill: "var(--color-online)",
      },
      {
        name: "offline",
        value: devices.filter((d) => !d.connected).length,
        fill: "var(--color-offline)",
      },
    ],
    [devices]
  );

  const sensorSourceData = useMemo(() => {
    const bySource = new Map<string, number>();
    for (const s of sensors) {
      const source = s.location?.source || "unknown";
      bySource.set(source, (bySource.get(source) || 0) + 1);
    }
    return Array.from(bySource.entries())
      .map(([name, value], idx) => ({
        name,
        value,
        fill: `var(--chart-${(idx % 5) + 1})`,
      }))
      .sort((a, b) => b.value - a.value);
  }, [sensors]);

  const networkStatusData = useMemo(() => {
    const byStatus = new Map<string, number>();
    for (const n of networks) {
      const status = n.status || "unknown";
      byStatus.set(status, (byStatus.get(status) || 0) + 1);
    }
    return Array.from(byStatus.entries())
      .map(([name, value], idx) => ({
        name,
        value,
        fill: `var(--chart-${(idx % 5) + 1})`,
      }))
      .sort((a, b) => b.value - a.value);
  }, [networks]);

  const rssiData = useMemo(() => {
    const byQuality = new Map<string, number>();
    for (const d of devices) {
      if (!d.connected || d.last_signal == null) continue;
      const q = getSignalQuality(d.last_signal);
      byQuality.set(q, (byQuality.get(q) || 0) + 1);
    }
    const order = ["excelente", "bueno", "medio", "débil", "muy débil", "desconocido"];
    return order
      .filter((k) => byQuality.has(k))
      .map((name, idx) => ({
        name,
        value: byQuality.get(name) || 0,
        fill: `var(--chart-${(idx % 5) + 1})`,
      }));
  }, [devices]);

  return (
    <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
      <Card className="flex flex-col">
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Dispositivos online
          </CardTitle>
        </CardHeader>
        <CardContent className="flex-1">
          <ChartContainer
            config={{
              online: { label: "Online", color: "var(--chart-1)" },
              offline: { label: "Offline", color: "var(--chart-2)" },
            }}
            className="mx-auto aspect-square h-[220px] max-h-[240px]"
          >
            <PieChart>
              <ChartTooltip content={<ChartTooltipContent hideLabel />} />
              <Pie data={statusData} dataKey="value" nameKey="name" innerRadius={48}>
                {statusData.map((entry, index) => (
                  <Cell key={`cell-${index}`} fill={entry.fill} />
                ))}
              </Pie>
            </PieChart>
          </ChartContainer>
        </CardContent>
      </Card>

      <Card className="flex flex-col">
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Fuente de ubicación de sensores
          </CardTitle>
        </CardHeader>
        <CardContent className="flex-1">
          <ChartContainer
            config={{
              value: { label: "Sensores", color: "var(--chart-1)" },
            }}
            className="h-[220px] max-h-[240px] w-full"
          >
            <BarChart data={sensorSourceData} margin={{ top: 8, right: 8, left: 8, bottom: 8 }}>
              <CartesianGrid vertical={false} />
              <XAxis dataKey="name" tickLine={false} axisLine={false} tickMargin={8} />
              <YAxis tickLine={false} axisLine={false} allowDecimals={false} />
              <ChartTooltip content={<ChartTooltipContent />} />
              <Bar dataKey="value" radius={4}>
                {sensorSourceData.map((entry, index) => (
                  <Cell key={`cell-${index}`} fill={entry.fill} />
                ))}
              </Bar>
            </BarChart>
          </ChartContainer>
        </CardContent>
      </Card>

      <Card className="flex flex-col">
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Estado de redes Wi-Fi
          </CardTitle>
        </CardHeader>
        <CardContent className="flex-1">
          <ChartContainer
            config={{
              value: { label: "Redes", color: "var(--chart-1)" },
            }}
            className="h-[220px] max-h-[240px] w-full"
          >
            <BarChart data={networkStatusData} margin={{ top: 8, right: 8, left: 8, bottom: 8 }}>
              <CartesianGrid vertical={false} />
              <XAxis dataKey="name" tickLine={false} axisLine={false} tickMargin={8} />
              <YAxis tickLine={false} axisLine={false} allowDecimals={false} />
              <ChartTooltip content={<ChartTooltipContent />} />
              <Bar dataKey="value" radius={4}>
                {networkStatusData.map((entry, index) => (
                  <Cell key={`cell-${index}`} fill={entry.fill} />
                ))}
              </Bar>
            </BarChart>
          </ChartContainer>
        </CardContent>
      </Card>

      <Card className="flex flex-col">
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Distribución de señal (RSSI)
          </CardTitle>
        </CardHeader>
        <CardContent className="flex-1">
          <ChartContainer
            config={{
              value: { label: "Dispositivos", color: "var(--chart-1)" },
            }}
            className="h-[220px] max-h-[240px] w-full"
          >
            <BarChart data={rssiData} margin={{ top: 8, right: 8, left: 8, bottom: 8 }}>
              <CartesianGrid vertical={false} />
              <XAxis dataKey="name" tickLine={false} axisLine={false} tickMargin={8} />
              <YAxis tickLine={false} axisLine={false} allowDecimals={false} />
              <ChartTooltip content={<ChartTooltipContent />} />
              <Bar dataKey="value" radius={4}>
                {rssiData.map((entry, index) => (
                  <Cell key={`cell-${index}`} fill={entry.fill} />
                ))}
              </Bar>
            </BarChart>
          </ChartContainer>
        </CardContent>
      </Card>
    </div>
  );
}
