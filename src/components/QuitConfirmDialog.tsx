import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import {
  terminalAliveSessionsWorthWarning,
  terminalShutdownGraceful,
  type AliveSessionView,
} from "@/lib/commands";
import { usePtyExits } from "@/stores/pty_exits";

/**
 * Listens for `weft://quit-requested` (emitted from Rust's
 * `WindowEvent::CloseRequested` / `RunEvent::ExitRequested` handlers)
 * and presents a confirmation dialog listing the active sessions. On
 * confirm: fires parallel graceful shutdowns and then `app.exit(0)`.
 *
 * The Rust handlers only fire this event when there ARE worth-warning
 * sessions, so the dialog always has content to show — we don't need
 * to re-check for "is it really worth prompting".
 */
export function QuitConfirmDialog() {
  const [open, setOpen] = useState(false);
  const [sessions, setSessions] = useState<AliveSessionView[]>([]);
  const [shuttingDown, setShuttingDown] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    listen("weft://quit-requested", async () => {
      if (disposed) return;
      try {
        const alive = await terminalAliveSessionsWorthWarning();
        if (disposed) return;
        setSessions(alive);
        setOpen(true);
      } catch (e) {
        console.warn("failed to load alive sessions", e);
        // Rust already prevented the quit; fail-open and quit anyway
        // rather than leaving the user stuck.
        await invoke("app_exit", { code: 0 });
      }
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((e) => console.warn("quit-requested listen failed", e));

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const doQuit = async () => {
    setShuttingDown(true);
    try {
      const ptyExits = usePtyExits.getState();
      for (const s of sessions) ptyExits.markExpectedExit(s.session_id);
      await Promise.allSettled(
        sessions.map((s) =>
          terminalShutdownGraceful(s.session_id).catch((e) =>
            console.warn("shutdown_graceful failed", s.session_id, e),
          ),
        ),
      );
    } finally {
      await invoke("app_exit", { code: 0 });
    }
  };

  const cancel = () => {
    setOpen(false);
    setSessions([]);
  };

  return (
    <AlertDialog open={open} onOpenChange={(next) => !next && cancel()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {sessions.length === 1
              ? "1 active session"
              : `${sessions.length} active sessions`}
          </AlertDialogTitle>
          <AlertDialogDescription>
            Quitting will gracefully stop these processes. Are you sure?
          </AlertDialogDescription>
        </AlertDialogHeader>
        <ul className="text-muted-foreground space-y-1 text-sm">
          {sessions.map((s) => (
            <li key={s.session_id} className="flex items-center gap-2">
              <span className="text-xs">
                {s.kind === "agent" ? "✨" : "▸"}
              </span>
              <span className="text-foreground">
                {s.label ?? "unnamed session"}
              </span>
            </li>
          ))}
        </ul>
        <AlertDialogFooter>
          <AlertDialogCancel onClick={cancel} disabled={shuttingDown}>
            Cancel
          </AlertDialogCancel>
          <AlertDialogAction
            onClick={doQuit}
            disabled={shuttingDown}
            className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
          >
            {shuttingDown ? "Stopping…" : "Quit anyway"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
