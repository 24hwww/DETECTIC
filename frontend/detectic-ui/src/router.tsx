import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { DashboardLayout } from "@/layouts/dashboard-layout";
import { DashboardView, MapView } from "@/App";
import { DevicesView } from "@/components/devices-view";

const rootRoute = createRootRoute({
  component: DashboardLayout,
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

const devicesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/devices",
  component: DevicesView,
});

const routeTree = rootRoute.addChildren([indexRoute, mapRoute, devicesRoute]);

export const router = createRouter({ routeTree });
