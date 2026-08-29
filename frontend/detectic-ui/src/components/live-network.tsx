import { useMemo } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Wifi, Smartphone } from "lucide-react";
import { ProximityBadge } from "@/components/proximity-badge";
import type { Device, Network } from "@/lib/api";

export function LiveNetwork({
  devices,
  networks,
}: {
  devices: Device[];
  networks: Network[];
}) {
  const online = useMemo(
    () => devices.filter((d) => d.connected || (d.state && d.state !== "ABSENT" && d.state !== "DISCONNECTED")).length,
    [devices]
  );
  const observed = devices.length;
  const aps = networks.length;

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Red en vivo
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-3 gap-4 border-b border-border pb-4">
          <div>
            <div className="text-2xl font-semibold tabular-nums">{online}</div>
            <div className="text-xs text-muted-foreground">En línea</div>
          </div>
          <div>
            <div className="text-2xl font-semibold tabular-nums">{observed}</div>
            <div className="text-xs text-muted-foreground">Observados</div>
          </div>
          <div>
            <div className="text-2xl font-semibold tabular-nums">{aps}</div>
            <div className="text-xs text-muted-foreground">Puntos de acceso</div>
          </div>
        </div>

        <div className="mt-4 grid grid-cols-1 gap-4 md:grid-cols-2">
          <div>
            <h4 className="mb-2 flex items-center gap-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
              <Smartphone className="h-3.5 w-3.5" />
              Dispositivos recientes
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
                        d.connected || (d.state && d.state !== "ABSENT" && d.state !== "DISCONNECTED")
                          ? "bg-[var(--color-online)]/10 text-[var(--color-online)]"
                          : "bg-[var(--color-offline)]/10 text-[var(--color-offline)]"
                      }`}
                    >
                      {d.state === "RF_PRESENT" ? "RF presente" : d.connected || (d.state && d.state !== "ABSENT" && d.state !== "DISCONNECTED") ? "conectado" : d.state ? d.state.toLowerCase() : "desconectado"}
                    </Badge>
                    <span className="w-12 text-right tabular-nums text-muted-foreground">
                      {d.last_signal != null ? `${d.last_signal}` : "—"}
                    </span>
                  </div>
                </div>
              ))}
              {devices.length === 0 && (
                <p className="text-xs text-muted-foreground">Aún no hay dispositivos</p>
              )}
            </div>
          </div>

          <div>
            <h4 className="mb-2 flex items-center gap-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
              <Wifi className="h-3.5 w-3.5" />
              Puntos de acceso recientes
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
                    {n.proximity ? (
                      <ProximityBadge proximity={n.proximity} detail={n.proximity_detail} />
                    ) : null}
                    <Badge
                      variant="outline"
                      className={`text-[10px] ${
                        n.status === "ONLINE"
                          ? "bg-[var(--color-online)]/10 text-[var(--color-online)]"
                          : "bg-[var(--color-offline)]/10 text-[var(--color-offline)]"
                      }`}
                    >
                      {n.status === "ONLINE" ? "online" : "offline"}
                    </Badge>
                    <span className="w-12 text-right tabular-nums text-muted-foreground">
                      {n.last_signal != null ? `${n.last_signal}` : "—"}
                    </span>
                  </div>
                </div>
              ))}
              {networks.length === 0 && (
                <p className="text-xs text-muted-foreground">Aún no hay APs</p>
              )}
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
