import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { DeviceTable } from "@/components/device-table";
import { PageHeader } from "@/components/page-header";
import { useRealtime } from "@/lib/realtime";
import { mergeLive } from "@/lib/merge";
import { fetchDevices, type Device } from "@/lib/api";

export function DevicesView() {
  const live = useRealtime();
  const navigate = useNavigate();
  const { data, isLoading, error } = useQuery<Device[]>({
    queryKey: ["devices"],
    queryFn: fetchDevices,
  });

  const liveDevices = useMemo(
    () => mergeLive(data || [], live.devices),
    [data, live.devices]
  );

  if (isLoading) return <div className="py-12 text-center text-muted-foreground">Cargando…</div>;
  if (error) return <div className="py-12 text-center text-destructive">{error.message}</div>;

  return (
    <div className="space-y-4 md:space-y-6">
      <PageHeader
        title="Devices"
        description="Dispositivos Wi-Fi observados"
      />
      <DeviceTable
        devices={liveDevices}
        onRowClick={(d) => navigate({ to: `/devices/${d.device_id}` })}
      />
    </div>
  );
}
