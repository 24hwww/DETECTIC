import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { RealtimeProvider } from "@/lib/realtime";
import { NotificationsProvider } from "@/lib/notifications";
import { Toaster } from "@/components/ui/toast";
import { router } from "./router";
import "./index.css";

const queryClient = new QueryClient();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RealtimeProvider>
        <NotificationsProvider>
          <Toaster>
            <RouterProvider router={router} />
          </Toaster>
        </NotificationsProvider>
      </RealtimeProvider>
    </QueryClientProvider>
  </StrictMode>
);

if ("serviceWorker" in navigator) {
  window.addEventListener("load", () => {
    navigator.serviceWorker
      .register("/sw.js")
      .catch((err) => console.warn("SW registration failed", err));
  });
}
