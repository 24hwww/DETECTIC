import { useMemo } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Wifi, Smartphone } from "lucide-react";
import { proximityText, deviceName, networkName, signalWord } from "@/lib/labels";
import type { Device, DetailedDevice, Network } from "@/lib/api";

function isOnline(d: Device) {
  return d.connected || (d.state && d.state !== "ABSENT" && d.state !== "DISCONNECTED");
}

export function LiveNetwork({
  devices,
  networks,
  identity,
}: {
  devices: Device[];
  networks: Network[];
  identity?: Map<string, DetailedDevice>;
}) {
  const online = useMemo(() => devices.filter(isOnline).length, [devices]);
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
                  <span className="truncate">
                    {deviceName(d, identity?.get(d.device_id))}
                  </span>
                  <div className="flex items-center gap-2">
                    <Badge
                      variant="outline"
                      className={`text-[10px] ${
                        isOnline(d)
                          ? "bg-[var(--color-online)]/10 text-[var(--color-online)]"
                          : "bg-[var(--color-offline)]/10 text-[var(--color-offline)]"
                      }`}
                    >
                      {isOnline(d) ? "conectado" : "no está"}
                    </Badge>
                    <span className="w-20 text-right text-muted-foreground">
                      {signalWord(d.last_signal)}
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
                  <span className="truncate">{networkName(n)}</span>
                  <div className="flex items-center gap-2">
                    {n.proximity ? (
                      <span className="text-[10px] text-[var(--color-warning)]">
                        {proximityText(n.proximity, n.proximity_detail)}
                      </span>
                    ) : null}
                    <Badge
                      variant="outline"
                      className={`text-[10px] ${
                        n.status === "ONLINE"
                          ? "bg-[var(--color-online)]/10 text-[var(--color-online)]"
                          : "bg-[var(--color-offline)]/10 text-[var(--color-offline)]"
                      }`}
                    >
                      {n.status === "ONLINE" ? "online" : "sin señal"}
                    </Badge>
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
