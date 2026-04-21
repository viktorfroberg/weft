import { useEffect } from "react";
import { useUi } from "@/stores/ui";
import { useNavigateRoute } from "@/lib/active-route";
import { TaskComposeCard } from "./TaskComposeCard";

/**
 * Mounts the floating compose card. Controlled by `ui.composeOpen`
 * (toggled via ⌘⇧N in `lib/shortcuts.ts`). Backdrop dims, ESC closes,
 * click outside closes. On successful create, navigates to the new
 * task and closes itself.
 */
export function TaskComposeOverlay() {
  const open = useUi((s) => s.composeOpen);
  const setComposeOpen = useUi((s) => s.setComposeOpen);
  const navigate = useNavigateRoute();

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setComposeOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setComposeOpen]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-[20vh] animate-in fade-in duration-150"
      onClick={(e) => {
        if (e.target === e.currentTarget) setComposeOpen(false);
      }}
    >
      <div className="animate-in zoom-in-95 slide-in-from-top-2 duration-150">
        <TaskComposeCard
          variant="floating"
          onCreated={(task) => {
            setComposeOpen(false);
            navigate({ kind: "task", id: task.id });
          }}
          onCancel={() => setComposeOpen(false)}
        />
      </div>
    </div>
  );
}
