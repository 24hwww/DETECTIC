import { Toast } from "@base-ui/react/toast";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";

/**
 * Lightweight toast system built on the Base UI Toast primitives.
 *
 * Mount <Toaster> once at the app root. Any descendant component can call
 * `useToast().add({ title, description, type })` to push an in-app notification.
 */

function ToastList() {
  const { toasts } = Toast.useToastManager();
  return (
    <Toast.Viewport className="pointer-events-none fixed inset-x-0 bottom-0 z-50 flex flex-col items-center gap-2 p-4 sm:items-end">
      {toasts.map((t) => (
        <Toast.Root
          key={t.id}
          toast={t}
          className={cn(
            "pointer-events-auto flex w-full items-start gap-3 rounded-xl border bg-card p-3 shadow-lg outline-none [&:where([data-transition-status='ending'])]:opacity-0 sm:w-[380px]",
            t.type === "network"
              ? "border-[var(--color-primary)]/50"
              : t.type === "offline"
              ? "border-[var(--color-offline)]/50"
              : "border-[var(--color-online)]/50"
          )}
        >
          <div className="flex-1">
            <Toast.Title className="text-sm font-semibold text-foreground">
              {t.title}
            </Toast.Title>
            {t.description != null && (
              <Toast.Description className="mt-0.5 text-xs text-muted-foreground">
                {t.description}
              </Toast.Description>
            )}
          </div>
          <Toast.Close
            aria-label="Cerrar"
            className="flex h-5 w-5 flex-none items-center justify-center rounded text-muted-foreground outline-none transition-colors hover:bg-muted hover:text-foreground"
          >
            <X className="h-4 w-4" />
          </Toast.Close>
        </Toast.Root>
      ))}
    </Toast.Viewport>
  );
}

export function Toaster({ children }: { children: React.ReactNode }) {
  return (
    <Toast.Provider timeout={7000} limit={6}>
      <ToastList />
      {children}
    </Toast.Provider>
  );
}

export function useToast() {
  return Toast.useToastManager();
}
