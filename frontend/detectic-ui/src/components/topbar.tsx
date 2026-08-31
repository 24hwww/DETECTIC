import { Bell, Menu, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { RealtimeIndicator } from "@/components/realtime-indicator";
import { useNotifications } from "@/lib/notifications";
import { useNavigate } from "@tanstack/react-router";

const breadcrumbMap: Record<string, string> = {
  "/": "Panel",
  "/map": "Mapa",
  "/devices": "Dispositivos",
  "/access-points": "Puntos de acceso",
  "/rf": "Entorno RF",
  "/events": "Eventos",
  "/sessions": "Sesiones",
  "/history": "Historial",
  "/sensors": "Sensores",
  "/router": "Router",
  "/connectivity": "Conectividad",
  "/reports": "Reportes",
  "/settings": "Configuración",
  "/notifications": "Notificaciones",
};

export function Topbar({ onToggleSidebar }: { onToggleSidebar?: () => void }) {
  const path = typeof window !== "undefined" ? window.location.pathname : "/";
  const page = breadcrumbMap[path] || "Panel";
  const navigate = useNavigate();
  const { unread } = useNotifications();

  return (
    <header className="flex h-14 items-center gap-4 border-b border-border bg-card px-4">
      <Button
        variant="ghost"
        size="icon"
        className="md:hidden"
        onClick={onToggleSidebar}
      >
        <Menu className="h-5 w-5" />
      </Button>

      <div className="flex items-center gap-2 text-sm">
        <span className="font-semibold tracking-tight text-foreground">
          DETECTIC
        </span>
        <span className="text-muted-foreground">/</span>
        <span className="text-muted-foreground">{page}</span>
      </div>

      <div className="relative ml-auto hidden w-72 sm:block">
        <Search className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
        <input
          type="search"
          placeholder="Buscar dispositivos, APs, eventos..."
          className="h-9 w-full rounded-md border border-input bg-background pl-9 pr-3 text-xs text-foreground outline-none ring-ring placeholder:text-muted-foreground focus:ring-1"
        />
      </div>

      <div className="ml-auto flex items-center gap-3 sm:ml-0">
        <RealtimeIndicator />
        <Button
          variant="ghost"
          size="icon"
          className="relative"
          onClick={() => navigate({ to: "/notifications" })}
        >
          <Bell className="h-4 w-4" />
          {unread > 0 && (
            <span className="absolute -right-0.5 top-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-[var(--color-offline)] px-1 text-[10px] font-semibold text-white">
              {unread > 99 ? "99+" : unread}
            </span>
          )}
        </Button>
      </div>
    </header>
  );
}
