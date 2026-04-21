import { create } from "zustand";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toast } from "sonner";

/** Matches the Rust `PtyExitEvent` shape in `terminal/session.rs`. */
export interface PtyExit {
  session_id: string;
  code: number | null;
  success: boolean;
  at: number; // perf.now() on receipt
}

interface PtyExitsState {
  /** Latest exit info per session id. Entries accumulate — we don't
   *  prune because the frontend tab lifetime is bounded by task
   *  unmount, and memory cost is trivial. */
  bySessionId: Record<string, PtyExit>;
  /** Mark a session as exited (used by the event bridge below). */
  record: (e: PtyExit) => void;
}

export const usePtyExits = create<PtyExitsState>((set) => ({
  bySessionId: {},
  record: (e) =>
    set((s) => ({
      bySessionId: { ...s.bySessionId, [e.session_id]: e },
    })),
}));

/**
 * Attach the `pty_exit` Tauri event listener. Mount once at app root.
 * Fires a passive toast on unexpected (non-zero / signal) exits so the
 * user notices when an agent crashes in a background tab.
 */
export function subscribePtyExits(
  onExit?: (e: PtyExit) => void,
): Promise<UnlistenFn> {
  return listen<Omit<PtyExit, "at">>("pty_exit", (evt) => {
    const payload: PtyExit = { ...evt.payload, at: performance.now() };
    usePtyExits.getState().record(payload);
    onExit?.(payload);
    if (!payload.success) {
      toast.error(
        payload.code !== null
          ? `Agent exited (code ${payload.code})`
          : "Agent exited unexpectedly",
        {
          description: `Session ${payload.session_id.slice(0, 8)}…`,
        },
      );
    }
  });
}
