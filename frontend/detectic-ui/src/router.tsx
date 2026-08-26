import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { DashboardLayout } from "@/layouts/dashboard-layout";
import { DashboardView, MapView } from "@/App";
import { DevicesView } from "@/components/devices-view";
import { DeviceDetailView } from "@/components/device-detail-view";

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

const deviceDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/devices/$deviceId",
  component: DeviceDetailView,
});

const routeTree = rootRoute.addChildren([indexRoute, mapRoute, devicesRoute, deviceDetailRoute]);

export const router = createRouter({ routeTree });
