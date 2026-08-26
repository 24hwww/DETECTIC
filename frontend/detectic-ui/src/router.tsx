import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { DashboardLayout } from "@/layouts/dashboard-layout";
import { DashboardView, MapView } from "@/App";
import { DevicesView } from "@/components/devices-view";
import { DeviceDetailView } from "@/components/device-detail-view";
import { AccessPointsView } from "@/components/access-points-view";
import { APDetailView } from "@/components/ap-detail-view";
import { RFEnvironmentView } from "@/components/rf-environment-view";

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

const accessPointsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/access-points",
  component: AccessPointsView,
});

const apDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/access-points/$apId",
  component: APDetailView,
});

const rfRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/rf",
  component: RFEnvironmentView,
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  mapRoute,
  devicesRoute,
  deviceDetailRoute,
  accessPointsRoute,
  apDetailRoute,
  rfRoute,
]);

export const router = createRouter({ routeTree });
