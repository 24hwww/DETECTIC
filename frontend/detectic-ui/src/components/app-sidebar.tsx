import { LayoutDashboard, Map as MapIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useLocation, useNavigate } from "@tanstack/react-router";

export function AppSidebar() {
  const location = useLocation();
  const navigate = useNavigate();

  return (
    <aside className="flex h-screen w-60 flex-col border-r border-border bg-card p-4">
      <div className="mb-6">
        <h1 className="text-lg font-semibold tracking-tight">Detectic</h1>
        <p className="text-xs text-muted-foreground">Huella Wi-Fi</p>
      </div>
      <nav className="flex flex-col gap-2">
        <Button
          variant={location.pathname === "/" ? "default" : "ghost"}
          className="w-full justify-start gap-3"
          onClick={() => navigate({ to: "/" })}
        >
          <LayoutDashboard className="h-4 w-4" />
          Dashboard
        </Button>
        <Button
          variant={location.pathname === "/map" ? "default" : "ghost"}
          className="w-full justify-start gap-3"
          onClick={() => navigate({ to: "/map" })}
        >
          <MapIcon className="h-4 w-4" />
          Mapa RF
        </Button>
      </nav>
    </aside>
  );
}
