import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { useRealtime } from "@/lib/realtime";
import { fetchTimeline } from "@/lib/api";
import { PageHeader } from "@/components/page-header";
import { ActivityTimelineChart } from "@/components/activity-timeline-chart";
import { RssiTimelineChart } from "@/components/rssi-timeline-chart";

export function HistoryView() {
  const live = useRealtime();
  const { data, isLoading, error } = useQuery({
    queryKey: ["timeline"],
    queryFn: fetchTimeline,
  });

  const points = useMemo(
    () =>
      [...(data?.points || []), ...live.points]
        .sort((a, b) => a.ts - b.ts)
        .slice(-500),
    [data, live.points]
  );

  if (isLoading) return <div className="py-12 text-center text-muted-foreground">Cargando…</div>;
  if (error) return <div className="py-12 text-center text-destructive">{error.message}</div>;

  return (
    <div className="space-y-4 md:space-y-6">
      <PageHeader
        title="History"
        description="Observaciones históricas de RSSI y actividad"
      />
      <RssiTimelineChart points={points} />
      <ActivityTimelineChart points={points} />
    </div>
  );
}
