import { useMemo } from "react";
import {
  Cell,
  Pie,
  PieChart,
} from "recharts";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart";
import type { DetailedDevice } from "@/lib/api";

const CLASS_ORDER = ["Smartphone", "Laptop", "IoT", "NAS", "Unknown"];
const CLASS_COLORS: Record<string, string> = {
  Smartphone: "var(--chart-1)",
  Laptop: "var(--chart-2)",
  IoT: "var(--chart-3)",
  NAS: "var(--chart-4)",
  Unknown: "var(--chart-5)",
};

export function DeviceClassChart({ devices }: { devices: DetailedDevice[] }) {
  const data = useMemo(() => {
    const counts = new Map<string, number>();
    for (const d of devices) {
      const c = CLASS_ORDER.includes(d.device_class || "")
        ? d.device_class!
        : "Unknown";
      counts.set(c, (counts.get(c) || 0) + 1);
    }
    return CLASS_ORDER
      .filter((c) => (counts.get(c) || 0) > 0)
      .map((c) => ({
        name: c,
        value: counts.get(c) || 0,
        fill: CLASS_COLORS[c],
      }));
  }, [devices]);

  return (
    <Card className="flex flex-col">
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Clasificación de dispositivos
        </CardTitle>
      </CardHeader>
      <CardContent className="flex-1">
        <ChartContainer
          config={{
            value: { label: "Dispositivos", color: "var(--chart-1)" },
          }}
          className="mx-auto aspect-square h-[240px] max-h-[260px]"
        >
          <PieChart>
            <ChartTooltip content={<ChartTooltipContent hideLabel />} />
            <Pie data={data} dataKey="value" nameKey="name" innerRadius={48}>
              {data.map((entry, index) => (
                <Cell key={`cell-${index}`} fill={entry.fill} />
              ))}
            </Pie>
          </PieChart>
        </ChartContainer>
      </CardContent>
    </Card>
  );
}
