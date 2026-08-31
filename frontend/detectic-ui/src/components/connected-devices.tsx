import { useMemo } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  deviceName,
  deviceSubtitle,
  proximityText,
  signalBars,
  signalWord,
  timeAgo,
} from "@/lib/labels";
import type { Device, DetailedDevice } from "@/lib/api";

function isOnline(d: Device) {
  return d.connected || (d.state && d.state !== "ABSENT" && d.state !== "DISCONNECTED");
}

export function ConnectedDevices({
  devices,
  identity,
}: {
  devices: Device[];
  identity?: Map<string, DetailedDevice>;
}) {
  const connected = useMemo(
    () =>
      devices
        .filter(isOnline)
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
          {connected.map((d) => {
            const id = identity?.get(d.device_id);
            return (
              <div
                key={d.device_id}
                className="rounded-lg border border-border bg-card p-3"
              >
                <div className="mb-1 truncate font-semibold text-foreground">
                  {deviceName(d, id)}
                </div>
                <div className="mb-2 truncate text-[11px] text-muted-foreground">
                  {deviceSubtitle(d, id) || "Dispositivo"}
                </div>
                <div className="flex items-center justify-between gap-2">
                  <Badge variant="outline" className="text-[10px]">
                    {proximityText(d.proximity)}
                  </Badge>
                  <span className="text-[10px] text-muted-foreground">
                    {signalWord(d.last_signal)}
                  </span>
                </div>
                <div className="mt-1 flex items-center justify-between text-[10px] text-muted-foreground">
                  <span>{signalBars(d.last_signal)}</span>
                  <span>{timeAgo(d.last_seen)}</span>
                </div>
              </div>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
}
