import { useMemo } from "react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  XAxis,
  YAxis,
} from "recharts";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ChartContainer, ChartTooltip } from "@/components/ui/chart";
import type { Device } from "@/lib/api";

function barColor(rssi?: number) {
  if (rssi == null) return "var(--color-offline)";
  if (rssi >= -50) return "var(--color-online)";
  if (rssi >= -70) return "var(--color-warning)";
  return "var(--color-offline)";
}

export function SignalProximityChart({ devices }: { devices: Device[] }) {
  const isOnline = (d: Device) => d.connected || (d.state && d.state !== "ABSENT" && d.state !== "DISCONNECTED");
  const data = useMemo(() => {
    return devices
      .filter((d) => isOnline(d) && d.last_signal != null)
      .sort((a, b) => (b.last_signal || -100) - (a.last_signal || -100))
      .slice(0, 15)
      .map((d) => ({
        name: (d.hostname || d.device_id).slice(0, 18),
        rssi: d.last_signal!,
        proximity: Math.max(0, 100 + d.last_signal!),
        fill: barColor(d.last_signal),
      }));
  }, [devices]);

  return (
    <Card className="flex flex-col">
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Proximidad por señal (más larga = más cerca del EX520)
        </CardTitle>
      </CardHeader>
      <CardContent className="flex-1">
        <ChartContainer
          config={{
            proximity: { label: "Proximidad", color: "var(--chart-1)" },
          }}
          className="h-[300px] w-full"
        >
          <BarChart
            data={data}
            layout="vertical"
            margin={{ top: 8, right: 24, bottom: 8, left: 8 }}
          >
            <CartesianGrid horizontal={true} vertical={false} />
            <XAxis
              type="number"
              domain={[0, 80]}
              tickLine={false}
              axisLine={false}
            />
            <YAxis
              type="category"
              dataKey="name"
              tickLine={false}
              axisLine={false}
              width={120}
              tick={{ fontSize: 11 }}
            />
            <ChartTooltip
              content={({ active, payload }) => {
                if (!active || !payload?.length) return null;
                const p = payload[0].payload as { name: string; rssi: number };
                return (
                  <div className="rounded-md border border-border bg-card px-3 py-2 text-xs shadow-sm">
                    <div className="font-medium">{p.name}</div>
                    <div className="text-muted-foreground">{p.rssi} dBm</div>
                  </div>
                );
              }}
            />
            <Bar dataKey="proximity" radius={[0, 4, 4, 0]}>
              {data.map((entry, index) => (
                <Cell key={`cell-${index}`} fill={entry.fill} />
              ))}
            </Bar>
          </BarChart>
        </ChartContainer>
      </CardContent>
    </Card>
  );
}
