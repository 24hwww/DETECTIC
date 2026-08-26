import { Bell, Menu, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { RealtimeIndicator } from "@/components/realtime-indicator";
import { cn } from "@/lib/utils";

const breadcrumbMap: Record<string, string> = {
  "/": "Overview",
  "/map": "Map",
  "/devices": "Devices",
  "/access-points": "Access Points",
  "/rf": "RF Environment",
};

export function Topbar({ onToggleSidebar }: { onToggleSidebar?: () => void }) {
  const path = typeof window !== "undefined" ? window.location.pathname : "/";
  const page = breadcrumbMap[path] || "Overview";

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
        <Button variant="ghost" size="icon" className="relative">
          <Bell className="h-4 w-4" />
          <span
            className={cn(
              "absolute right-2 top-2 h-1.5 w-1.5 rounded-full bg-primary",
              "hidden"
            )}
          />
        </Button>
      </div>
    </header>
  );
}
