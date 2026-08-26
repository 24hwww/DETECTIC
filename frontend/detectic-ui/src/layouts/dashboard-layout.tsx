import { useState } from "react";
import { Outlet } from "@tanstack/react-router";
import { Sidebar } from "@/components/sidebar";
import { Topbar } from "@/components/topbar";
import { cn } from "@/lib/utils";

export function DashboardLayout() {
  const [show, setShow] = useState(false);

  return (
    <div className="flex h-screen w-full overflow-hidden bg-background text-foreground">
      <aside
        className={cn(
          "fixed inset-y-0 left-0 z-40 transform border-r border-border bg-card transition-transform md:static md:translate-x-0",
          show ? "translate-x-0" : "-translate-x-full"
        )}
      >
        <Sidebar />
      </aside>

      {show && (
        <div
          className="fixed inset-0 z-30 bg-black/50 md:hidden"
          onClick={() => setShow(false)}
        />
      )}

      <div className="flex flex-1 flex-col overflow-hidden">
        <Topbar onToggleSidebar={() => setShow((s) => !s)} />
        <main className="flex-1 overflow-y-auto p-4 md:p-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
