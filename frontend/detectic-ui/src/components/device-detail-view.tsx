import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate, useParams } from "@tanstack/react-router";
import { ArrowLeft, Smartphone } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { DeviceSignalChart } from "@/components/device-signal-chart";
import { useRealtime } from "@/lib/realtime";
import {
  fetchAllDevices,
  fetchTimeline,
  type DetailedDevice,
} from "@/lib/api";

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

export function DeviceDetailView() {
  const { deviceId } = useParams({ from: "/devices/$deviceId" });
  const navigate = useNavigate();
  const live = useRealtime();

  const all = useQuery<DetailedDevice[]>({
    queryKey: ["all-devices"],
    queryFn: fetchAllDevices,
  });
  const timeline = useQuery({
    queryKey: ["timeline"],
    queryFn: fetchTimeline,
  });

  const liveDev = useMemo(() => live.devices.get(deviceId), [live.devices, deviceId]);
  const detail = useMemo(
    () => all.data?.find((d) => d.pseudonym === deviceId),
    [all.data, deviceId]
  );

  const points = useMemo(
    () =>
      [
        ...(timeline.data?.points || []),
        ...live.points,
      ]
        .filter((p) => p.pseudonym === deviceId)
        .sort((a, b) => a.ts - b.ts),
    [timeline.data, live.points, deviceId]
  );

  if (all.isLoading || timeline.isLoading) {
    return <div className="py-12 text-center text-muted-foreground">Cargando…</div>;
  }

  if (!liveDev && !detail) {
    return (
      <div className="space-y-4 md:space-y-6">
        <Button
          variant="outline"
          size="sm"
          className="gap-2"
          onClick={() => navigate({ to: "/devices" })}
        >
          <ArrowLeft className="h-4 w-4" />
          Volver
        </Button>
        <div className="rounded-lg border border-destructive bg-destructive/10 p-4 text-sm text-destructive">
          Dispositivo no encontrado
        </div>
      </div>
    );
  }

  const name = detail?.hostname || liveDev?.hostname || deviceId;
  const isConnected = liveDev?.connected ?? detail?.status === "connected";

  return (
    <div className="space-y-4 md:space-y-6">
      <Button
        variant="outline"
        size="sm"
        className="gap-2"
        onClick={() => navigate({ to: "/devices" })}
      >
        <ArrowLeft className="h-4 w-4" />
        Volver
      </Button>

      <div className="flex items-start gap-4">
        <div className="rounded-lg border border-border bg-card p-3">
          <Smartphone className="h-6 w-6 text-primary" />
        </div>
        <div>
          <h2 className="text-2xl font-semibold tracking-tight">{name}</h2>
          <p className="font-mono text-sm text-muted-foreground">
            {deviceId}
          </p>
        </div>
        <Badge
          variant={isConnected ? "default" : "secondary"}
          className={
            isConnected
              ? "ml-auto bg-[var(--color-online)]/10 text-[var(--color-online)]"
              : "ml-auto bg-[var(--color-offline)]/10 text-[var(--color-offline)]"
          }
        >
          {isConnected ? "connected" : "disconnected"}
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
                    isConnected
                      ? "bg-[var(--color-online)]/10 text-[var(--color-online)]"
                      : "bg-[var(--color-offline)]/10 text-[var(--color-offline)]"
                  }
                >
                  {isConnected ? "Connected" : "Disconnected"}
                </Badge>
              }
            />
            <Field
              label="Signal"
              value={
                liveDev?.last_signal != null
                  ? `${liveDev.last_signal} dBm`
                  : detail?.signal_strength != null
                  ? `${detail.signal_strength} dBm`
                  : "—"
              }
            />
            <Field
              label="Band"
              value={liveDev?.band || detail?.band || "—"}
            />
            <Field
              label="AP"
              value={detail?.bssid_pseudonym || detail?.bssid_manufacturer || "—"}
            />
            <Field label="First Seen" value={fmtDate(detail?.first_seen)} />
            <Field label="Last Seen" value={timeAgo(liveDev?.last_seen || detail?.last_seen)} />
            <Field
              label="Observations"
              value={liveDev?.event_count ?? detail?.observations ?? "—"}
            />
            <Field
              label="Manufacturer"
              value={detail?.manufacturer || detail?.brand || "Unknown"}
            />
            <Field label="Class" value={detail?.device_class || "Unknown"} />
            <Field label="MAC type" value={detail?.mac_type || "—"} />
            <Field
              label="Fingerprint"
              value={detail?.fingerprint_model || "—"}
            />
            <Field
              label="Confidence"
              value={detail?.confidence_label || "—"}
            />
          </div>
        </CardContent>
      </Card>

      <DeviceSignalChart pseudonym={deviceId} points={points} />

      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Session History
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground">
            Session history requires backend support. Currently showing signal
            history as a proxy for observed activity.
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
