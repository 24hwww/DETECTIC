import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { PageHeader } from "@/components/page-header";
import { AlertTriangle, Check, EyeOff, Smartphone } from "lucide-react";
import { fetchUnknownDevices, updateDeviceTrust, type UnknownDevice } from "@/lib/api";

function timeAgo(ms?: number | null) {
  if (ms == null) return "—";
  const diff = Math.floor(Date.now() - ms) / 1000;
  if (diff < 60) return `${Math.floor(diff)}s`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h`;
  return `${Math.floor(diff / 86400)}d`;
}

export function UnknownDevicesView() {
  const queryClient = useQueryClient();
  const { data, isLoading } = useQuery<UnknownDevice[]>({
    queryKey: ["unknown-devices"],
    queryFn: () => fetchUnknownDevices(168),
  });

  const setTrust = useMutation({
    mutationFn: ({ pseudonym, status }: { pseudonym: string; status: 'known' | 'ignored' }) =>
      updateDeviceTrust(pseudonym, status),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["unknown-devices"] });
      queryClient.invalidateQueries({ queryKey: ["all-devices"] });
    },
  });

  return (
    <div className="space-y-4 md:space-y-6">
      <PageHeader
        title="Dispositivos desconocidos"
        description="Dispositivos nuevos en la red. Reconócelos o ignóralos."
      />

      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="flex items-center gap-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
            <AlertTriangle className="h-4 w-4" />
            Nuevos dispositivos detectados
          </CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          {isLoading ? (
            <p className="p-6 text-center text-sm text-muted-foreground">Cargando…</p>
          ) : !data || data.length === 0 ? (
            <p className="p-6 text-center text-sm text-muted-foreground">
              No hay dispositivos desconocidos recientes.
            </p>
          ) : (
            <div className="max-h-[70vh] overflow-auto">
              <table className="w-full text-sm">
                <thead className="bg-muted text-muted-foreground">
                  <tr>
                    <th className="p-3 text-left font-medium">Dispositivo</th>
                    <th className="p-3 text-left font-medium">Clase</th>
                    <th className="p-3 text-left font-medium">Sensor</th>
                    <th className="p-3 text-left font-medium">Primera vez</th>
                    <th className="p-3 text-left font-medium">Alertas</th>
                    <th className="p-3 text-left font-medium">Acciones</th>
                  </tr>
                </thead>
                <tbody>
                  {data.map((d) => (
                    <tr key={d.pseudonym} className="border-b border-border last:border-0">
                      <td className="p-3">
                        <div className="flex items-center gap-2">
                          <Smartphone className="h-4 w-4 text-muted-foreground" />
                          <div>
                            <div className="font-mono text-xs">{d.alias || d.pseudonym.slice(0, 16)}</div>
                            {d.alias && <div className="text-[10px] text-muted-foreground">{d.pseudonym.slice(0, 16)}</div>}
                          </div>
                        </div>
                      </td>
                      <td className="p-3">
                        <Badge variant="secondary" className="text-[10px]">
                          {d.device_class || "Desconocido"}
                        </Badge>
                      </td>
                      <td className="p-3 text-xs">{d.sensor_id || "—"}</td>
                      <td className="p-3 text-xs tabular-nums">{timeAgo(d.first_seen)}</td>
                      <td className="p-3 text-xs tabular-nums">{d.alert_count}</td>
                      <td className="p-3">
                        <div className="flex items-center gap-2">
                          <Button
                            size="sm"
                            variant="outline"
                            className="h-7 gap-1 text-xs"
                            onClick={() => setTrust.mutate({ pseudonym: d.pseudonym, status: 'known' })}
                            disabled={setTrust.isPending}
                          >
                            <Check className="h-3 w-3" />
                            Conocido
                          </Button>
                          <Button
                            size="sm"
                            variant="outline"
                            className="h-7 gap-1 text-xs"
                            onClick={() => setTrust.mutate({ pseudonym: d.pseudonym, status: 'ignored' })}
                            disabled={setTrust.isPending}
                          >
                            <EyeOff className="h-3 w-3" />
                            Ignorar
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
