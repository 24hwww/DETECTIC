import {
  Activity,
  AlertTriangle,
  Bell,
  LayoutDashboard,
  // Map as MapIcon,
  Monitor,
  Radio,
  Router,
  Scan,
  Server,
  Settings,
  Smartphone,
  Wifi,
} from "lucide-react";
import { useLocation, useNavigate } from "@tanstack/react-router";
import { cn } from "@/lib/utils";

function NavItem({
  to,
  icon: Icon,
  label,
}: {
  to: string;
  icon: React.ElementType;
  label: string;
}) {
  const location = useLocation();
  const navigate = useNavigate();
  const active = location.pathname === to;

  return (
    <button
      onClick={() => navigate({ to })}
      className={cn(
        "flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm transition-colors",
        active
          ? "bg-primary/10 text-foreground"
          : "text-muted-foreground hover:bg-muted hover:text-foreground"
      )}
    >
      <Icon className="h-4 w-4" />
      <span>{label}</span>
      {active && <span className="ml-auto h-1.5 w-1.5 rounded-full bg-primary" />}
    </button>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="mb-4">
      <h3 className="mb-2 px-3 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
        {title}
      </h3>
      <div className="flex flex-col gap-1">{children}</div>
    </div>
  );
}

export function Sidebar() {
  return (
    <aside className="flex h-full w-60 flex-col border-r border-border bg-card p-4">
      <div className="mb-6 flex items-center gap-2 px-2">
        <Scan className="h-5 w-5 text-primary" />
        <div>
          <h1 className="text-base font-semibold tracking-tight">DETECTIC</h1>
          <p className="text-[10px] text-muted-foreground">Huella Wi-Fi</p>
        </div>
      </div>

      <nav className="flex-1 overflow-y-auto">
        <Section title="">
          <NavItem to="/" icon={LayoutDashboard} label="Panel" />
        </Section>

        <Section title="Monitoreo">
          <NavItem to="/" icon={Activity} label="Monitor en vivo" />
          <NavItem to="/devices" icon={Smartphone} label="Dispositivos" />
          <NavItem to="/unknown-devices" icon={AlertTriangle} label="Desconocidos" />
          <NavItem to="/access-points" icon={Wifi} label="Puntos de acceso" />
          <NavItem to="/rf" icon={Radio} label="Entorno RF" />
        </Section>

        <Section title="Inteligencia">
          <NavItem to="/sessions" icon={Monitor} label="Sesiones" />
          <NavItem to="/events" icon={Activity} label="Eventos" />
          <NavItem to="/history" icon={Server} label="Historial" />
          { /* <NavItem to="/map" icon={MapIcon} label="Mapa" /> */ }
        </Section>

        <Section title="Red">
          { /* <NavItem to="/sensors" icon={Server} label="Sensor" /> */ }
          <NavItem to="/router" icon={Router} label="Router" />
          <NavItem to="/connectivity" icon={Wifi} label="Conectividad" />
        </Section>

        <Section title="Sistema">
          <NavItem to="/notifications" icon={Bell} label="Notificaciones" />
          <NavItem to="/reports" icon={Monitor} label="Reportes" />
          <NavItem to="/settings" icon={Settings} label="Configuración" />
        </Section>
      </nav>
    </aside>
  );
}
