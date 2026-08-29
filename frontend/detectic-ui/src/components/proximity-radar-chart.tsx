import { useMemo } from "react";
import {
  Radar,
  RadarChart,
  PolarGrid,
  PolarAngleAxis,
  PolarRadiusAxis,
} from "recharts";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ChartContainer, ChartTooltip, ChartLegend, ChartLegendContent } from "@/components/ui/chart";
import type { Device } from "@/lib/api";

const SECTORS = [
  "N",
  "NE",
  "E",
  "SE",
  "S",
  "SW",
  "W",
  "NW",
] as const;

const BANDS = ["2.4GHz", "5GHz", "6GHz", "unknown"] as const;

const BAND_COLORS: Record<string, string> = {
  "2.4GHz": "var(--chart-1)",
  "5GHz": "var(--chart-2)",
  "6GHz": "var(--chart-3)",
  unknown: "var(--chart-4)",
};

function normalizeBand(raw?: string | null): string {
  if (!raw) return "unknown";
  const s = String(raw).toLowerCase();
  if (s.includes("2.4") || s.includes("2_4") || s.includes("2.4ghz")) return "2.4GHz";
  if (s.includes("5") || s.includes("5ghz")) return "5GHz";
  if (s.includes("6") || s.includes("6ghz")) return "6GHz";
  return "unknown";
}

function signalScore(rssi?: number | null): number {
  if (rssi == null) return 0;
  // Map -100..-30 dBm to 0..100; clamp.
  return Math.max(0, Math.min(100, 100 + rssi));
}

function hashCode(str: string): number {
  let h = 0;
  for (let i = 0; i < str.length; i++) {
    h = (h << 5) - h + str.charCodeAt(i);
    h |= 0;
  }
  return Math.abs(h);
}

function isOnline(d: Device): boolean {
  return d.connected || (d.state != null && d.state !== "ABSENT" && d.state !== "DISCONNECTED");
}

export function ProximityRadarChart({ devices }: { devices: Device[] }) {
  const data = useMemo(() => {
    // For each (sector, band) keep the strongest signal score.
    const perBand = new Map<string, number[]>();
    for (const d of devices) {
      if (!isOnline(d)) continue;
      const band = normalizeBand(d.band);
      const sectorIdx = hashCode(d.device_id) % SECTORS.length;
      const score = signalScore(d.last_signal);
      const arr = perBand.get(band) || new Array(SECTORS.length).fill(0);
      arr[sectorIdx] = Math.max(arr[sectorIdx], score);
      perBand.set(band, arr);
    }

    // Ensure all bands are present so the legend is stable.
    for (const b of BANDS) {
      if (!perBand.has(b)) perBand.set(b, new Array(SECTORS.length).fill(0));
    }

    return SECTORS.map((sector, i) => {
      const row: Record<string, number | string> = { sector };
      for (const [band, arr] of perBand.entries()) {
        row[band] = arr[i];
      }
      return row;
    });
  }, [devices]);



  return (
    <Card className="flex flex-col">
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Proximidad de señales alrededor del EX520
        </CardTitle>
      </CardHeader>
      <CardContent className="flex-1">
        <ChartContainer
          config={{
            "2.4GHz": { label: "2.4 GHz", color: BAND_COLORS["2.4GHz"] },
            "5GHz": { label: "5 GHz", color: BAND_COLORS["5GHz"] },
            "6GHz": { label: "6 GHz", color: BAND_COLORS["6GHz"] },
            unknown: { label: "Desconocido", color: BAND_COLORS.unknown },
          }}
          className="h-[320px] w-full"
        >
          <RadarChart
            data={data}
            margin={{ top: 8, right: 24, bottom: 8, left: 24 }}
          >
            <PolarGrid />
            <PolarAngleAxis dataKey="sector" tick={{ fontSize: 10 }} />
            <PolarRadiusAxis
              angle={90}
              domain={[0, 100]}
              tick={{ fontSize: 9 }}
            />
            {BANDS.map((band) => (
              <Radar
                key={band}
                name={band}
                dataKey={band}
                stroke={BAND_COLORS[band]}
                fill={BAND_COLORS[band]}
                fillOpacity={0.25}
              />
            ))}
            <ChartTooltip
              content={({ active, payload, label }) => {
                if (!active || !payload?.length) return null;
                return (
                  <div className="rounded-md border border-border bg-card px-3 py-2 text-xs shadow-sm">
                    <div className="font-medium">{label}</div>
                    {payload
                      .filter((p) => Number(p.value) > 0)
                      .map((p, i) => (
                        <div
                          key={i}
                          className="flex items-center gap-2 text-muted-foreground"
                        >
                          <span
                            className="inline-block h-2 w-2 rounded-full"
                            style={{ background: p.color }}
                          />
                          <span>
                            {p.name}: {p.value}%
                          </span>
                        </div>
                      ))}
                  </div>
                );
              }}
            />
            <ChartLegend
              content={<ChartLegendContent nameKey="name" />}
            />
          </RadarChart>
        </ChartContainer>
      </CardContent>
    </Card>
  );
}
