import { createRootRoute, createRoute, createRouter, Outlet } from "@tanstack/react-router";
import { AppSidebar } from "@/components/app-sidebar";
import { DashboardView, MapView } from "@/App";

const rootRoute = createRootRoute({
  component: () => (
    <div className="flex h-screen w-full overflow-hidden bg-background text-foreground">
      <AppSidebar />
      <main className="flex-1 overflow-y-auto p-6">
        <Outlet />
      </main>
    </div>
  ),
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: DashboardView,
});

const mapRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/map",
  component: MapView,
});

const routeTree = rootRoute.addChildren([indexRoute, mapRoute]);

export const router = createRouter({ routeTree });
