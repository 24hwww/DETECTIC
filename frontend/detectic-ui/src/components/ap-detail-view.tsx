import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate, useParams } from "@tanstack/react-router";
import { ArrowLeft, Wifi } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useRealtime } from "@/lib/realtime";
import { fetchAllNetworks } from "@/lib/api";

function timeAgo(ms?: number | null) {
  if (ms == null) return "—";
  const diff = Math.floor(Date.now() - ms) / 1000;
  if (diff < 60) return `${Math.floor(diff)}s`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h`;
  return `${Math.floor(diff / 86400)}d`;
}

function fmtDate(ms?: number | null) {
  if (ms == null) return "—";
  return new Date(ms).toLocaleString();
}

function Field({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
        {label}
      </span>
      <span className="text-sm font-medium text-foreground">{value}</span>
    </div>
  );
}

export function APDetailView() {
  const { apId } = useParams({ from: "/access-points/$apId" });
  const navigate = useNavigate();
  const live = useRealtime();

  const { data, isLoading, error } = useQuery({
    queryKey: ["all-networks"],
    queryFn: fetchAllNetworks,
  });

  const liveAp = useMemo(() => live.networks.get(apId), [live.networks, apId]);
  const detail = useMemo(
    () => data?.aps.find((n) => n.ap_id === apId),
    [data, apId]
  );

  if (isLoading) return <div className="py-12 text-center text-muted-foreground">Cargando…</div>;
  if (error) return <div className="py-12 text-center text-destructive">{error.message}</div>;

  if (!liveAp && !detail) {
    return (
      <div className="space-y-4 md:space-y-6">
        <Button
          variant="outline"
          size="sm"
          className="gap-2"
          onClick={() => navigate({ to: "/access-points" })}
        >
          <ArrowLeft className="h-4 w-4" />
          Volver
        </Button>
        <div className="rounded-lg border border-destructive bg-destructive/10 p-4 text-sm text-destructive">
          AP no encontrado
        </div>
      </div>
    );
  }

  const name = detail?.ssid || liveAp?.ssid || apId;
  const online = detail?.status === "ONLINE" || liveAp?.status === "ONLINE";

  return (
    <div className="space-y-4 md:space-y-6">
      <Button
        variant="outline"
        size="sm"
        className="gap-2"
        onClick={() => navigate({ to: "/access-points" })}
      >
        <ArrowLeft className="h-4 w-4" />
        Volver
      </Button>

      <div className="flex items-start gap-4">
        <div className="rounded-lg border border-border bg-card p-3">
          <Wifi className="h-6 w-6 text-primary" />
        </div>
        <div>
          <h2 className="text-2xl font-semibold tracking-tight">{name}</h2>
          <p className="font-mono text-sm text-muted-foreground">{apId}</p>
        </div>
        <Badge
          variant={online ? "default" : "secondary"}
          className={
            online
              ? "ml-auto bg-green-500/10 text-green-500"
              : "ml-auto bg-red-500/10 text-red-500"
          }
        >
          {online ? "online" : "offline"}
        </Badge>
      </div>

      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Current State
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4">
            <Field
              label="Status"
              value={
                <Badge
                  variant="outline"
                  className={
                    online
                      ? "bg-green-500/10 text-green-500"
                      : "bg-red-500/10 text-red-500"
                  }
                >
                  {online ? "ONLINE" : "OFFLINE"}
                </Badge>
              }
            />
            <Field
              label="Signal (current)"
              value={
                liveAp?.last_signal != null || detail?.current_signal != null
                  ? `${liveAp?.last_signal ?? detail?.current_signal} dBm`
                  : "—"
              }
            />
            <Field
              label="Average Signal"
              value={
                detail?.average_signal != null
                  ? `${detail.average_signal} dBm`
                  : "—"
              }
            />
            <Field
              label="Signal Range"
              value={
                detail?.min_signal != null && detail?.max_signal != null
                  ? `${detail.min_signal} … ${detail.max_signal} dBm`
                  : "—"
              }
            />
            <Field label="Band" value={liveAp?.band || detail?.band || "—"} />
            <Field
              label="Channel"
              value={detail?.channel != null ? String(detail.channel) : liveAp?.channel != null ? String(liveAp.channel) : "—"}
            />
            <Field
              label="Security"
              value={liveAp?.security || detail?.security || "—"}
            />
            <Field label="Mode" value={liveAp?.w_mode || detail?.w_mode || "—"} />
            <Field label="Sensor" value={liveAp?.sensor_id || detail?.sensor_id || "—"} />
            <Field label="First Seen" value={fmtDate(detail?.first_seen)} />
            <Field label="Last Seen" value={timeAgo(liveAp?.last_seen || detail?.last_seen)} />
            <Field
              label="Observations"
              value={liveAp?.event_count ?? detail?.observation_count ?? "—"}
            />
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
