import { useQuery } from "@tanstack/react-query";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { PageHeader } from "@/components/page-header";
import { RealtimeIndicator } from "@/components/realtime-indicator";
import { LiveFeed } from "@/components/live-feed";
import { useRealtime } from "@/lib/realtime";
import { fetchStats, type Stats } from "@/lib/api";

export function ConnectivityView() {
  const { status, events } = useRealtime();
  const { data: stats, isLoading } = useQuery<Stats>({
    queryKey: ["stats"],
    queryFn: fetchStats,
  });

  const transport = status === "en línea" ? "WebSocket OK" : `WebSocket ${status}`;

  if (isLoading) return <div className="py-12 text-center text-muted-foreground">Cargando…</div>;

  return (
    <div className="space-y-4 md:space-y-6">
      <PageHeader
        title="Connectivity"
        description="Estado de transporte y eventos en vivo"
      />

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Transporte
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex items-center gap-2">
              <Badge
                variant="outline"
                className={
                  status === "en línea"
                    ? "bg-[var(--color-online)]/10 text-[var(--color-online)]"
                    : "bg-[var(--color-offline)]/10 text-[var(--color-offline)]"
                }
              >
                {transport}
              </Badge>
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Realtime
            </CardTitle>
          </CardHeader>
          <CardContent>
            <RealtimeIndicator />
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Eventos en cola
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold tabular-nums">
              {events.length}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Observaciones
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold tabular-nums">
              {stats?.total_snapshots ?? "—"}
            </div>
          </CardContent>
        </Card>
      </div>

      <LiveFeed />
    </div>
  );
}
