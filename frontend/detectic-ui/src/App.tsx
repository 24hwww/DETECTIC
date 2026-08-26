import { useQuery } from "@tanstack/react-query";
import {
  Map,
  MapControls,
  MapMarker,
  MarkerContent,
  MarkerPopup,
} from "@/components/ui/map";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { DashboardCharts } from "@/components/charts";
import { DeviceTable } from "@/components/device-table";
import { sourceColor } from "@/lib/location";
import { fetchSensors, fetchDevices, fetchNetworks } from "@/lib/api";

function Loading() {
  return (
    <div className="flex h-full items-center justify-center text-muted-foreground">
      Cargando…
    </div>
  );
}

function ErrorMessage({ error }: { error?: Error | null }) {
  return (
    <div className="rounded-lg border border-destructive bg-destructive/10 p-4 text-sm text-destructive">
      {error?.message ?? "Error desconocido"}
    </div>
  );
}

function useDashboardData() {
  const sensors = useQuery({ queryKey: ["sensors"], queryFn: fetchSensors });
  const devices = useQuery({ queryKey: ["devices"], queryFn: fetchDevices });
  const networks = useQuery({ queryKey: ["networks"], queryFn: fetchNetworks });
  return { sensors, devices, networks };
}

export function DashboardView() {
  const { sensors, devices, networks } = useDashboardData();

  if (sensors.isLoading || devices.isLoading || networks.isLoading) {
    return <Loading />;
  }

  const error = sensors.error || devices.error || networks.error;
  if (error) {
    return <ErrorMessage error={error as Error} />;
  }

  const allSensors = sensors.data || [];
  const allDevices = devices.data || [];
  const allNetworks = networks.data || [];

  const knownSensors = allSensors.filter((s) => s.location?.latitude != null);
  const onlineDevices = allDevices.filter((d) => d.connected).length;
  const offlineDevices = allDevices.length - onlineDevices;

  return (
    <div className="space-y-6">
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Sensores
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold">{allSensors.length}</div>
            <p className="text-xs text-muted-foreground">
              {knownSensors.length} con ubicación
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Dispositivos online
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold">{onlineDevices}</div>
            <p className="text-xs text-muted-foreground">
              {allDevices.length} vistos en 24h
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Redes Wi-Fi
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold">{allNetworks.length}</div>
            <p className="text-xs text-muted-foreground">APs observados</p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Dispositivos offline
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold">{offlineDevices}</div>
            <p className="text-xs text-muted-foreground">últimas 24h</p>
          </CardContent>
        </Card>
      </div>

      <DashboardCharts
        devices={allDevices}
        networks={allNetworks}
        sensors={allSensors}
      />

      <DeviceTable devices={allDevices} />
    </div>
  );
}

export function MapView() {
  const { data, isLoading, error } = useQuery({
    queryKey: ["sensors"],
    queryFn: fetchSensors,
  });

  if (isLoading) return <Loading />;
  if (error) return <ErrorMessage error={error as Error} />;

  const knownSensors = (data || []).filter((s) => s.location?.latitude != null);

  return (
    <Card className="overflow-hidden">
      <CardHeader>
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Mapa RF
        </CardTitle>
      </CardHeader>
      <CardContent className="p-0">
        <div className="h-[70vh] w-full">
          <Map
            theme="dark"
            className="h-full w-full"
            viewport={{
              center: [-49.35, -28.68],
              zoom: 14,
            }}
          >
            <MapControls />
            {knownSensors.map((s) =>
              s.location?.latitude != null && s.location.longitude != null ? (
                <MapMarker
                  key={s.id}
                  latitude={s.location.latitude}
                  longitude={s.location.longitude}
                >
                  <MarkerContent>
                    <div className="flex items-center gap-2">
                      <div
                        className={`h-3 w-3 rounded-full border border-white ${sourceColor(
                          s.location.source
                        )}`}
                      />
                      <span className="text-xs font-medium text-foreground">
                        {s.name || s.id}
                      </span>
                    </div>
                  </MarkerContent>
                  <MarkerPopup>
                    <div className="min-w-[180px] p-2">
                      <div className="mb-1 font-semibold">{s.name || s.id}</div>
                      <div className="text-xs text-muted-foreground">
                        Fuente: <Badge variant="secondary">{s.location.source}</Badge>
                      </div>
                      <div className="text-xs text-muted-foreground">
                        Precisión: ~{s.location.accuracy_m ?? "?"} m
                      </div>
                      {s.public_ip && (
                        <div className="text-xs text-muted-foreground">IP: {s.public_ip}</div>
                      )}
                    </div>
                  </MarkerPopup>
                </MapMarker>
              ) : null
            )}
          </Map>
        </div>
      </CardContent>
    </Card>
  );
}

// Keep a default export for backward compatibility; it renders the dashboard.
export default function App() {
  return <DashboardView />;
}
