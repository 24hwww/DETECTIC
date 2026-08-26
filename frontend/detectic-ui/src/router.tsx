import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { DashboardLayout } from "@/layouts/dashboard-layout";
import { DashboardView, MapView } from "@/App";

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

const routeTree = rootRoute.addChildren([indexRoute, mapRoute]);

export const router = createRouter({ routeTree });
