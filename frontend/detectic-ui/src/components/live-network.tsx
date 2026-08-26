import { useMemo } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Wifi, Smartphone } from "lucide-react";
import type { Device, Network } from "@/lib/api";

export function LiveNetwork({
  devices,
  networks,
}: {
  devices: Device[];
  networks: Network[];
}) {
  const connected = useMemo(
    () => devices.filter((d) => d.connected).length,
    [devices]
  );
  const observed = devices.length;
  const aps = networks.length;

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Live Network
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-3 gap-4 border-b border-border pb-4">
          <div>
            <div className="text-2xl font-semibold tabular-nums">{connected}</div>
            <div className="text-xs text-muted-foreground">Connected</div>
          </div>
          <div>
            <div className="text-2xl font-semibold tabular-nums">{observed}</div>
            <div className="text-xs text-muted-foreground">Observed</div>
          </div>
          <div>
            <div className="text-2xl font-semibold tabular-nums">{aps}</div>
            <div className="text-xs text-muted-foreground">APs</div>
          </div>
        </div>

        <div className="mt-4 grid grid-cols-1 gap-4 md:grid-cols-2">
          <div>
            <h4 className="mb-2 flex items-center gap-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
              <Smartphone className="h-3.5 w-3.5" />
              Recent Devices
            </h4>
            <div className="space-y-1.5">
              {devices.slice(0, 8).map((d) => (
                <div
                  key={d.device_id}
                  className="flex items-center justify-between rounded-md bg-muted/40 px-2 py-1.5 text-xs"
                >
                  <span className="truncate font-mono">
                    {d.hostname || d.device_id.slice(0, 16)}
                  </span>
                  <div className="flex items-center gap-2">
                    <Badge
                      variant="outline"
                      className={`text-[10px] ${
                        d.connected
                          ? "bg-green-500/10 text-green-500"
                          : "bg-red-500/10 text-red-500"
                      }`}
                    >
                      {d.connected ? "conn" : "disc"}
                    </Badge>
                    <span className="w-12 text-right tabular-nums text-muted-foreground">
                      {d.last_signal != null ? `${d.last_signal}` : "—"}
                    </span>
                  </div>
                </div>
              ))}
              {devices.length === 0 && (
                <p className="text-xs text-muted-foreground">No devices yet</p>
              )}
            </div>
          </div>

          <div>
            <h4 className="mb-2 flex items-center gap-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
              <Wifi className="h-3.5 w-3.5" />
              Recent APs
            </h4>
            <div className="space-y-1.5">
              {networks.slice(0, 8).map((n) => (
                <div
                  key={n.ap_id}
                  className="flex items-center justify-between rounded-md bg-muted/40 px-2 py-1.5 text-xs"
                >
                  <span className="truncate font-mono">
                    {n.ssid || n.ap_id.slice(0, 16)}
                  </span>
                  <div className="flex items-center gap-2">
                    <Badge
                      variant="outline"
                      className={`text-[10px] ${
                        n.status === "ONLINE"
                          ? "bg-green-500/10 text-green-500"
                          : "bg-red-500/10 text-red-500"
                      }`}
                    >
                      {n.status || "—"}
                    </Badge>
                    <span className="w-12 text-right tabular-nums text-muted-foreground">
                      {n.last_signal != null ? `${n.last_signal}` : "—"}
                    </span>
                  </div>
                </div>
              ))}
              {networks.length === 0 && (
                <p className="text-xs text-muted-foreground">No APs yet</p>
              )}
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
