import { useQuery } from "@tanstack/react-query";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { PageHeader } from "@/components/page-header";
import { fetchSensors, type Sensor } from "@/lib/api";
import { sourceColor } from "@/lib/location";

function SensorCard({ s }: { s: Sensor }) {
  return (
    <Card>
      <CardContent className="p-4">
        <div className="mb-2 flex items-center justify-between">
          <div className="font-mono text-sm font-semibold text-foreground">
            {s.name || s.id}
          </div>
          <Badge
            variant="outline"
            className="bg-[var(--color-online)]/10 text-[var(--color-online)] text-[10px]"
          >
            reachable
          </Badge>
        </div>
        <div className="grid grid-cols-2 gap-2 text-xs text-muted-foreground">
          <div>
            <span className="block text-[10px] uppercase">ID</span>
            <span className="font-mono text-foreground">{s.id.slice(0, 18)}</span>
          </div>
          <div>
            <span className="block text-[10px] uppercase">Public IP</span>
            <span className="text-foreground">{s.public_ip || "—"}</span>
          </div>
          <div>
            <span className="block text-[10px] uppercase">Location</span>
            <span className="text-foreground">
              {s.location?.latitude != null && s.location?.longitude != null
                ? `${s.location.latitude.toFixed(4)}, ${s.location.longitude.toFixed(4)}`
                : "—"}
            </span>
          </div>
          <div>
            <span className="block text-[10px] uppercase">Source</span>
            <div className="flex items-center gap-1.5">
              <div
                className={`h-2 w-2 rounded-full ${sourceColor(
                  s.location?.source
                )}`}
              />
              <span className="text-foreground">
                {s.location?.source || "—"}
              </span>
            </div>
          </div>
          {s.location?.accuracy_m != null && (
            <div className="col-span-2">
              <span className="block text-[10px] uppercase">Accuracy</span>
              <span className="text-foreground">~{s.location.accuracy_m} m</span>
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

export function SensorsView() {
  const { data, isLoading, error } = useQuery({
    queryKey: ["sensors"],
    queryFn: fetchSensors,
  });

  if (isLoading) return <div className="py-12 text-center text-muted-foreground">Cargando…</div>;
  if (error) return <div className="py-12 text-center text-destructive">{error.message}</div>;

  const sensors = data || [];

  return (
    <div className="space-y-4 md:space-y-6">
      <PageHeader
        title="Sensors"
        description="Nodos Detectic reportando telemetry"
      />
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {sensors.map((s) => (
          <SensorCard key={s.id} s={s} />
        ))}
        {sensors.length === 0 && (
          <p className="col-span-full text-center text-sm text-muted-foreground">
            No hay sensores configurados
          </p>
        )}
      </div>
    </div>
  );
}
