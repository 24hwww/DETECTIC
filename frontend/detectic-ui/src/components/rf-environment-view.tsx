import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { Bar, BarChart, CartesianGrid, Cell, XAxis, YAxis } from "recharts";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { PageHeader } from "@/components/page-header";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart";
import { fetchAllNetworks, type RfSnapshot } from "@/lib/api";

function parseChannelDistribution(
  dist?: string | Record<string, number> | null
): Record<string, number> {
  if (!dist) return {};
  if (typeof dist === "string") {
    try {
      return JSON.parse(dist) as Record<string, number>;
    } catch {
      return {};
    }
  }
  return dist;
}

function bandColor(channel: number) {
  return channel <= 14 ? "var(--color-online)" : "var(--color-warning)";
}

export function RFEnvironmentView() {
  const { data, isLoading, error } = useQuery({
    queryKey: ["all-networks"],
    queryFn: fetchAllNetworks,
  });

  const latest = useMemo<RfSnapshot | undefined>(
    () => data?.rf_snapshots?.[0],
    [data]
  );

  const channels = useMemo(() => {
    const dist = parseChannelDistribution(latest?.channel_distribution);
    return Object.entries(dist)
      .map(([ch, count]) => ({
        channel: Number(ch),
        count,
        fill: bandColor(Number(ch)),
      }))
      .sort((a, b) => a.channel - b.channel);
  }, [latest]);

  if (isLoading) return <div className="py-12 text-center text-muted-foreground">Cargando…</div>;
  if (error) return <div className="py-12 text-center text-destructive">{error.message}</div>;

  return (
    <div className="space-y-4 md:space-y-6">
      <PageHeader
        title="Entorno RF"
        description="Canales, densidad de APs y señal"
      />

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              APs totales
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold tabular-nums">
              {latest?.ap_count ?? "—"}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              2.4 GHz
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold tabular-nums">
              {latest?.ap_count_2_4 ?? "—"}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              5 GHz
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold tabular-nums">
              {latest?.ap_count_5 ?? "—"}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Señal media
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold tabular-nums">
              {latest?.average_signal != null
                ? `${latest.average_signal} dBm`
                : "—"}
            </div>
          </CardContent>
        </Card>
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Distribución de canales
            </CardTitle>
          </CardHeader>
          <CardContent>
            {channels.length === 0 ? (
              <p className="py-6 text-center text-sm text-muted-foreground">
                Sin datos de canales
              </p>
            ) : (
              <ChartContainer
                config={{
                  count: { label: "APs por canal", color: "var(--chart-1)" },
                }}
                className="h-[260px] w-full"
              >
                <BarChart
                  data={channels}
                  margin={{ top: 8, right: 8, bottom: 24, left: 8 }}
                >
                  <CartesianGrid vertical={false} />
                  <XAxis
                    dataKey="channel"
                    type="category"
                    tickLine={false}
                    axisLine={false}
                    tickMargin={8}
                  />
                  <YAxis tickLine={false} axisLine={false} />
                  <ChartTooltip content={<ChartTooltipContent />} />
                  <Bar dataKey="count" radius={[4, 4, 0, 0]}>
                    {channels.map((c, i) => (
                      <Cell key={`cell-${i}`} fill={c.fill} />
                    ))}
                  </Bar>
                </BarChart>
              </ChartContainer>
            )}
            <div className="mt-4 flex gap-4 text-xs text-muted-foreground">
              <div className="flex items-center gap-1.5">
                <span className="h-2 w-2 rounded-full bg-[var(--color-online)]" />
                2.4 GHz
              </div>
              <div className="flex items-center gap-1.5">
                <span className="h-2 w-2 rounded-full bg-[var(--color-warning)]" />
                5 GHz
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Rango de señal
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 gap-4">
              <div>
                <div className="text-[10px] uppercase text-muted-foreground">
                  Más fuerte
                </div>
                <div className="text-xl font-semibold tabular-nums">
                  {latest?.strongest_signal != null
                    ? `${latest.strongest_signal} dBm`
                    : "—"}
                </div>
              </div>
              <div>
                <div className="text-[10px] uppercase text-muted-foreground">
                  Más débil
                </div>
                <div className="text-xl font-semibold tabular-nums">
                  {latest?.weakest_signal != null
                    ? `${latest.weakest_signal} dBm`
                    : "—"}
                </div>
              </div>
              <div>
                <div className="text-[10px] uppercase text-muted-foreground">
                  Media
                </div>
                <div className="text-xl font-semibold tabular-nums">
                  {latest?.average_signal != null
                    ? `${latest.average_signal} dBm`
                    : "—"}
                </div>
              </div>
              <div>
                <div className="text-[10px] uppercase text-muted-foreground">
                  Varianza RSSI
                </div>
                <div className="text-xl font-semibold tabular-nums">
                  {latest?.rssi_variance ?? "—"}
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Top APs recientes
          </CardTitle>
        </CardHeader>
        <CardContent>
          {latest?.top_aps ? (
            <div className="space-y-2">
              {(typeof latest.top_aps === "string"
                ? (JSON.parse(latest.top_aps) as { ap_id?: string; ssid?: string; signal?: number }[])
                : (latest.top_aps as { ap_id?: string; ssid?: string; signal?: number }[])
              ).map((ap, i) => (
                <div
                  key={i}
                  className="flex items-center justify-between rounded-md bg-muted/40 px-3 py-2 text-sm"
                >
                  <span className="font-mono text-xs">
                    {ap.ssid || ap.ap_id?.slice(0, 18) || "unknown"}
                  </span>
                  <Badge variant="outline" className="text-[10px]">
                    {ap.signal != null ? `${ap.signal} dBm` : "—"}
                  </Badge>
                </div>
              ))}
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">No top APs data</p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
