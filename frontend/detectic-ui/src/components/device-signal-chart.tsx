import { useMemo } from "react";
import {
  CartesianGrid,
  Line,
  LineChart,
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

export function DeviceSignalChart({
  pseudonym,
  points,
}: {
  pseudonym: string;
  points: TimelinePoint[];
}) {
  const data = useMemo(
    () =>
      points
        .filter((p) => p.pseudonym === pseudonym && p.rssi != null)
        .sort((a, b) => a.ts - b.ts)
        .slice(-100)
        .map((p) => ({ ts: p.ts, rssi: p.rssi, band: p.band })),
    [pseudonym, points]
  );

  return (
    <Card className="flex flex-col">
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Historial de señal
        </CardTitle>
      </CardHeader>
      <CardContent className="flex-1">
        <ChartContainer
          config={{
            rssi: { label: "RSSI (dBm)", color: "var(--chart-1)" },
          }}
          className="h-[260px] w-full"
        >
          <LineChart
            data={data}
            margin={{ top: 8, right: 8, bottom: 8, left: 8 }}
          >
            <CartesianGrid strokeDasharray="3 3" />
            <XAxis
              dataKey="ts"
              tickLine={false}
              axisLine={false}
              tickFormatter={timeLabel}
              tickMargin={8}
            />
            <YAxis
              domain={[-100, -20]}
              tickLine={false}
              axisLine={false}
            />
            <ChartTooltip content={<ChartTooltipContent />} />
            <Line
              type="monotone"
              dataKey="rssi"
              stroke="var(--chart-1)"
              strokeWidth={2}
              dot={false}
              activeDot={{ r: 4 }}
            />
          </LineChart>
        </ChartContainer>
      </CardContent>
    </Card>
  );
}
