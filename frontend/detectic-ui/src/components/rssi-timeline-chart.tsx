import { useMemo } from "react";
import {
  CartesianGrid,
  Scatter,
  ScatterChart,
  XAxis,
  YAxis,
} from "recharts";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart";
import type { TimelinePoint } from "@/lib/api";

const COLORS = [
  "var(--chart-1)",
  "var(--chart-2)",
  "var(--chart-3)",
  "var(--chart-4)",
  "var(--chart-5)",
  "var(--chart-6)",
  "var(--chart-7)",
  "var(--chart-8)",
  "var(--chart-9)",
  "var(--chart-10)",
];

function timeLabel(ts: number) {
  return new Date(ts * 1000).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}

function rssiColor(r?: number | null) {
  if (r == null) return "var(--color-offline)";
  if (r >= -50) return "var(--color-online)";
  if (r >= -70) return "var(--color-warning, #d29922)";
  return "var(--color-destructive, #f85149)";
}

export function RssiTimelineChart({ points }: { points: TimelinePoint[] }) {
  const series = useMemo(() => {
    const byPseudo = new Map<string, TimelinePoint[]>();
    for (const p of points) {
      if (p.rssi == null || !p.pseudonym) continue;
      const list = byPseudo.get(p.pseudonym) || [];
      list.push(p);
      byPseudo.set(p.pseudonym, list);
    }
    const sorted = Array.from(byPseudo.entries())
      .sort((a, b) => b[1].length - a[1].length)
      .slice(0, 10);
    return sorted.map(([pseudonym, pts], i) => ({
      name: pseudonym.slice(0, 12),
      color: COLORS[i % COLORS.length],
      data: pts
        .sort((a, b) => a.ts - b.ts)
        .map((p) => ({
          ts: p.ts,
          rssi: p.rssi,
          pseudonym: p.pseudonym,
          fill: rssiColor(p.rssi),
        })),
    }));
  }, [points]);

  return (
    <Card className="flex flex-col">
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          RSSI por dispositivo (24h)
        </CardTitle>
      </CardHeader>
      <CardContent className="flex-1">
        <ChartContainer
          config={{
            rssi: { label: "RSSI (dBm)", color: "var(--chart-1)" },
          }}
          className="h-[260px] w-full"
        >
          <ScatterChart margin={{ top: 8, right: 8, bottom: 8, left: 8 }}>
            <CartesianGrid strokeDasharray="3 3" />
            <XAxis
              type="number"
              dataKey="ts"
              domain={["dataMin", "dataMax"]}
              tickFormatter={timeLabel}
              tickLine={false}
              axisLine={false}
              tickMargin={8}
            />
            <YAxis
              type="number"
              dataKey="rssi"
              domain={[-95, -20]}
              tickLine={false}
              axisLine={false}
            />
            <ChartTooltip content={<ChartTooltipContent />} />
            {series.map((s, idx) => (
              <Scatter
                key={s.name + idx}
                name={s.name}
                data={s.data}
                line
                lineType="joint"
                fill={s.color}
              />
            ))}
          </ScatterChart>
        </ChartContainer>
      </CardContent>
    </Card>
  );
}
