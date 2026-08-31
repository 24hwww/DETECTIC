import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { DashboardLayout } from "@/layouts/dashboard-layout";
import { DashboardView, MapView } from "@/App";
import { DevicesView } from "@/components/devices-view";
import { DeviceDetailView } from "@/components/device-detail-view";
import { AccessPointsView } from "@/components/access-points-view";
import { APDetailView } from "@/components/ap-detail-view";
import { RFEnvironmentView } from "@/components/rf-environment-view";
import { EventsView } from "@/components/events-view";
import { SessionsView } from "@/components/sessions-view";
import { HistoryView } from "@/components/history-view";
import { SensorsView } from "@/components/sensors-view";
import { RouterView } from "@/components/router-view";
import { ConnectivityView } from "@/components/connectivity-view";
import { ReportsView } from "@/components/reports-view";
import { SettingsView } from "@/components/settings-view";
import { NotificationsView } from "@/components/notifications-view";
import { UnknownDevicesView } from "@/components/unknown-devices-view";
import { SensorHealthView } from "@/components/sensor-health-view";

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

const eventsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/events",
  component: EventsView,
});

const sessionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/sessions",
  component: SessionsView,
});

const historyRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/history",
  component: HistoryView,
});

const sensorsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/sensors",
  component: SensorsView,
});

const routerRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/router",
  component: RouterView,
});

const connectivityRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/connectivity",
  component: ConnectivityView,
});

const reportsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/reports",
  component: ReportsView,
});

const unknownDevicesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/unknown-devices",
  component: UnknownDevicesView,
});

const sensorHealthRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/sensor-health",
  component: SensorHealthView,
});

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: SettingsView,
});

const notificationsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/notifications",
  component: NotificationsView,
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  mapRoute,
  devicesRoute,
  deviceDetailRoute,
  accessPointsRoute,
  apDetailRoute,
  rfRoute,
  eventsRoute,
  sessionsRoute,
  historyRoute,
  sensorsRoute,
  routerRoute,
  connectivityRoute,
  reportsRoute,
  unknownDevicesRoute,
  sensorHealthRoute,
  settingsRoute,
  notificationsRoute,
]);

export const router = createRouter({ routeTree });
