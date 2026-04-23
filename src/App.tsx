import { RouterProvider } from "@tanstack/react-router";
import { QueryClientProvider } from "@tanstack/react-query";
import { router } from "@/router";
import { queryClient } from "@/query";
import { ConfirmDialogHost } from "@/components/ConfirmDialog";
import { QuitConfirmDialog } from "@/components/QuitConfirmDialog";
import { Toaster } from "@/components/ui/sonner";
import { DbEventBridge } from "@/lib/db-event-bridge";
import { CustomFontsBootstrap } from "@/lib/custom-fonts-bootstrap";

/**
 * Outermost providers. Kept thin — everything app-specific lives in
 * Shell (the route tree's `__root__`). Order:
 *
 * 1. QueryClientProvider — must wrap anything that calls `useQuery`,
 *    including the Rust `db_event` bridge.
 * 2. DbEventBridge — listens once to the `db_event` channel and fans
 *    out to `queryClient.invalidateQueries(...)`. Renders nothing.
 * 3. RouterProvider — owns the route tree; `<Shell />` is its root
 *    component (see `src/router.tsx`).
 * 4. ConfirmDialogHost + Toaster — global overlays that don't need
 *    router context and shouldn't remount on route change.
 */
export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <DbEventBridge />
      <CustomFontsBootstrap />
      <RouterProvider router={router} />
      <ConfirmDialogHost />
      <QuitConfirmDialog />
      <Toaster />
    </QueryClientProvider>
  );
}
