import { useMemo } from "react";
import {
  Bar,
  BarChart,
  CartesianGrid,
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

function timeLabel(ts: number) {
  return new Date(ts * 1000).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function ActivityTimelineChart({ points }: { points: TimelinePoint[] }) {
  const data = useMemo(() => {
    const buckets = new Map<number, number>();
    for (const p of points) {
      if (!p.ts) continue;
      const hour = Math.floor(p.ts / 3600) * 3600;
      buckets.set(hour, (buckets.get(hour) || 0) + 1);
    }
    const sorted = Array.from(buckets.entries()).sort((a, b) => a[0] - b[0]);
    return sorted
      .slice(-24)
      .map(([ts, count]) => ({ ts, count, fill: "var(--chart-1)" }));
  }, [points]);

  return (
    <Card className="flex flex-col">
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Actividad (24h)
        </CardTitle>
      </CardHeader>
      <CardContent className="flex-1">
        <ChartContainer
          config={{
            count: { label: "Observaciones", color: "var(--chart-1)" },
          }}
          className="h-[260px] w-full"
        >
          <BarChart data={data} margin={{ top: 8, right: 8, bottom: 8, left: 8 }}>
            <CartesianGrid vertical={false} />
            <XAxis
              dataKey="ts"
              tickLine={false}
              axisLine={false}
              tickFormatter={timeLabel}
              tickMargin={8}
            />
            <YAxis
              tickLine={false}
              axisLine={false}
              allowDecimals={false}
            />
            <ChartTooltip content={<ChartTooltipContent />} />
            <Bar dataKey="count" radius={4} fill="var(--chart-1)" />
          </BarChart>
        </ChartContainer>
      </CardContent>
    </Card>
  );
}
