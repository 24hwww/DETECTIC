import { Smartphone, Wifi } from "lucide-react";

export function HeroKpis({
  connected,
  nearby,
}: {
  connected: number;
  nearby: number;
}) {
  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
      <div className="relative overflow-hidden rounded-2xl border border-border bg-card p-6">
        <div className="flex items-start justify-between">
          <div>
            <p className="text-sm font-medium uppercase tracking-wide text-muted-foreground">
              Dispositivos conectados
            </p>
            <div className="mt-2 text-5xl font-bold tracking-tight text-foreground tabular-nums sm:text-6xl">
              {connected}
            </div>
          </div>
          <div className="rounded-xl bg-[var(--color-online)]/10 p-3 text-[var(--color-online)]">
            <Smartphone className="h-6 w-6" />
          </div>
        </div>
        <div className="mt-4 text-xs text-muted-foreground">
          En línea ahora
        </div>
      </div>

      <div className="relative overflow-hidden rounded-2xl border border-border bg-card p-6">
        <div className="flex items-start justify-between">
          <div>
            <p className="text-sm font-medium uppercase tracking-wide text-muted-foreground">
              Dispositivos cercanos
            </p>
            <div className="mt-2 text-5xl font-bold tracking-tight text-foreground tabular-nums sm:text-6xl">
              {nearby}
            </div>
          </div>
          <div className="rounded-xl bg-[var(--color-primary)]/10 p-3 text-[var(--color-primary)]">
            <Wifi className="h-6 w-6" />
          </div>
        </div>
        <div className="mt-4 text-xs text-muted-foreground">
          Proximidad inmediata o cercana
        </div>
      </div>
    </div>
  );
}
