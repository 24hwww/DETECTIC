import { useQuery } from "@tanstack/react-query";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { PageHeader } from "@/components/page-header";
import { fetchStats, fetchSensors, type Stats } from "@/lib/api";

export function RouterView() {
  const { data: stats, isLoading: s1 } = useQuery<Stats>({
    queryKey: ["stats"],
    queryFn: fetchStats,
  });
  const { data: sensors, isLoading: s2 } = useQuery({
    queryKey: ["sensors"],
    queryFn: fetchSensors,
  });

  if (s1 || s2) return <div className="py-12 text-center text-muted-foreground">Cargando…</div>;

  return (
    <div className="space-y-4 md:space-y-6">
      <PageHeader
        title="Router"
        description="Estado del nodo sensor / router"
      />
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Sensores activos
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold tabular-nums">
              {stats?.total_sensors ?? "—"}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Detecciones
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold tabular-nums">
              {stats?.total_detections ?? "—"}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Snapshots
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold tabular-nums">
              {stats?.total_snapshots ?? "—"}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Redes vistas
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold tabular-nums">
              {stats?.total_networks ?? "—"}
            </div>
          </CardContent>
        </Card>
      </div>
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Detalles del router
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground">
            El backend aún no expone detalles específicos del firmware/modelo del
            router. Esta sección mostrará versión, uptime, interfaces Wi-Fi y
            carga cuando estén disponibles.
          </p>
          <div className="mt-3 grid grid-cols-2 gap-4 text-sm">
            <div>
              <span className="text-[10px] uppercase text-muted-foreground">
                Modelo
              </span>
              <p className="font-medium text-foreground">TP-Link EX520V</p>
            </div>
            <div>
              <span className="text-[10px] uppercase text-muted-foreground">
                Sensors
              </span>
              <p className="font-medium text-foreground">
                {sensors?.length ?? 0} registrados
              </p>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
