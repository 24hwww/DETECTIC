import { useMemo } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import type { Device } from "@/lib/api";

function timeAgo(ms?: number) {
  if (ms == null) return "—";
  const diff = Math.floor(Date.now() - ms) / 1000;
  if (diff < 60) return `${Math.floor(diff)}s`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h`;
  return `${Math.floor(diff / 86400)}d`;
}

function signalColor(r?: number) {
  if (r == null) return "bg-muted text-muted-foreground";
  if (r >= -50) return "bg-[var(--color-online)]/10 text-[var(--color-online)]";
  if (r >= -70) return "bg-[var(--color-warning)]/10 text-[var(--color-warning)]";
  return "bg-[var(--color-offline)]/10 text-[var(--color-offline)]";
}

export function ConnectedDevices({ devices }: { devices: Device[] }) {
  const connected = useMemo(
    () =>
      devices
        .filter((d) => d.connected)
        .sort((a, b) => (b.last_seen || 0) - (a.last_seen || 0))
        .slice(0, 24),
    [devices]
  );

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Dispositivos conectados
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {connected.length === 0 && (
            <div className="col-span-full py-6 text-center text-sm text-muted-foreground">
              No hay dispositivos conectados
            </div>
          )}
          {connected.map((d) => (
            <div
              key={d.device_id}
              className="rounded-lg border border-border bg-card p-3"
            >
              <div className="mb-1 truncate font-mono text-sm font-semibold text-foreground">
                {d.hostname || d.device_id.slice(0, 16)}
              </div>
              <div className="flex items-center justify-between gap-2">
                <Badge
                  variant="outline"
                  className={`text-[10px] ${signalColor(d.last_signal)}`}
                >
                  {d.last_signal != null ? `${d.last_signal} dBm` : "—"}
                </Badge>
                <span className="text-[10px] text-muted-foreground">
                  {timeAgo(d.last_seen)}
                </span>
              </div>
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}
