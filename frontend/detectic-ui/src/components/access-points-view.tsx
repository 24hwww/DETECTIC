import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { NetworkTable } from "@/components/network-table";
import { PageHeader } from "@/components/page-header";
import { useRealtime } from "@/lib/realtime";
import { mergeLive } from "@/lib/merge";
import { fetchAllNetworks } from "@/lib/api";

export function AccessPointsView() {
  const live = useRealtime();
  const navigate = useNavigate();
  const { data, isLoading, error } = useQuery({
    queryKey: ["all-networks"],
    queryFn: fetchAllNetworks,
  });

  const liveNetworks = useMemo(
    () => mergeLive(data?.aps || [], live.networks),
    [data?.aps, live.networks]
  );

  if (isLoading) return <div className="py-12 text-center text-muted-foreground">Cargando…</div>;
  if (error) return <div className="py-12 text-center text-destructive">{error.message}</div>;

  return (
    <div className="space-y-4 md:space-y-6">
      <PageHeader
        title="Puntos de acceso"
        description="APs y redes Wi-Fi observadas"
      />
      <NetworkTable
        networks={liveNetworks}
        onRowClick={(n) => navigate({ to: `/access-points/${n.ap_id}` })}
      />
    </div>
  );
}
