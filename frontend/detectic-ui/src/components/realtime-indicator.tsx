import { useMemo } from "react";
import { Badge } from "@/components/ui/badge";
import { useRealtime } from "@/lib/realtime";
import { Activity, CloudOff, Loader2 } from "lucide-react";

export function RealtimeIndicator() {
  const { status, events } = useRealtime();

  const last = events[0]?.server_time;
  const ago = useMemo(() => {
    if (!last) return null;
    const s = Math.floor((Date.now() - last) / 1000);
    if (s < 60) return `${s}s`;
    return `${Math.floor(s / 60)}m`;
  }, [last]);

  if (status === "conectando") {
    return (
      <Badge
        variant="outline"
        className="gap-1.5 text-xs text-muted-foreground"
      >
        <Loader2 className="h-3 w-3 animate-spin" />
        <span>Conectando</span>
      </Badge>
    );
  }

  if (status === "desconectado") {
    return (
      <Badge
        variant="outline"
        className="gap-1.5 text-xs text-red-500"
      >
        <CloudOff className="h-3 w-3" />
        <span>Offline</span>
      </Badge>
    );
  }

  return (
    <Badge
      variant="outline"
      className="gap-1.5 text-xs text-green-500"
      title={last ? `Último evento hace ${ago}` : "Sin eventos aún"}
    >
      <Activity className="h-3 w-3" />
      <span>LIVE</span>
      {ago && <span className="text-muted-foreground">· {ago}</span>}
    </Badge>
  );
}
