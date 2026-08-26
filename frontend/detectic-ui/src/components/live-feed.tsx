import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { useRealtime } from "@/lib/realtime";

export function LiveFeed() {
  const { events, status } = useRealtime();

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Eventos en tiempo real
        </CardTitle>
        <div className="text-[10px] text-muted-foreground">
          {status === "en línea" ? (
            <Badge variant="default" className="text-[10px]">
              {status}
            </Badge>
          ) : (
            <Badge variant="secondary" className="text-[10px]">
              {status}
            </Badge>
          )}
        </div>
      </CardHeader>
      <CardContent>
        <div className="max-h-[260px] space-y-2 overflow-y-auto pr-2">
          {events.length === 0 && (
            <div className="py-6 text-center text-sm text-muted-foreground">
              Esperando conexión WebSocket…
            </div>
          )}
          {events.map((e, i) => (
            <div
              key={i}
              className="rounded-md border border-border bg-muted/40 p-2 text-xs"
            >
              <div className="mb-1 flex items-center gap-2">
                <Badge variant="outline" className="text-[10px]">
                  {e.type}
                </Badge>
                {e.sensor_id && (
                  <span className="text-muted-foreground">{e.sensor_id}</span>
                )}
                {e.server_time && (
                  <span className="ml-auto text-muted-foreground">
                    {new Date(e.server_time).toLocaleTimeString([], {
                      hour: "2-digit",
                      minute: "2-digit",
                      second: "2-digit",
                    })}
                  </span>
                )}
              </div>
              <pre className="max-h-[80px] overflow-x-auto text-[10px] text-muted-foreground">
                {JSON.stringify(e.payload, null, 2)}
              </pre>
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}
