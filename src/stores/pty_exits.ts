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
  /** Session ids whose exit the frontend itself triggered (graceful
   *  close, quit-dialog shutdown). Used to suppress the
   *  "agent exited (code N)" toast — when WE killed it, code 1 is
   *  the expected outcome of SIGHUP/SIGTERM, not a crash to surface. */
  expectedExits: Set<string>;
  /** When `Terminal.tsx` finished `spawn()` for each session id —
   *  used to distinguish "spawn failure / resume failure" (exits
   *  within a few seconds) from "real mid-session crash" (exits
   *  later). Spawn-time failures already surface their reason in
   *  the terminal output, so we don't also toast. */
  spawnedAt: Record<string, number>;
  /** Mark a session as exited (used by the event bridge below). */
  record: (e: PtyExit) => void;
  /** Caller marks a session as user-initiated before calling
   *  `terminal_shutdown_graceful` / `terminal_kill`. */
  markExpectedExit: (sessionId: string) => void;
  /** Caller (Terminal.tsx) records when the PTY was successfully
   *  spawned, so the exit-time-vs-spawn-time heuristic can fire. */
  markSpawned: (sessionId: string) => void;
}

export const usePtyExits = create<PtyExitsState>((set) => ({
  bySessionId: {},
  expectedExits: new Set<string>(),
  spawnedAt: {},
  record: (e) =>
    set((s) => ({
      bySessionId: { ...s.bySessionId, [e.session_id]: e },
    })),
  markExpectedExit: (sessionId) =>
    set((s) => {
      if (s.expectedExits.has(sessionId)) return {};
      const next = new Set(s.expectedExits);
      next.add(sessionId);
      return { expectedExits: next };
    }),
  markSpawned: (sessionId) =>
    set((s) => ({
      spawnedAt: { ...s.spawnedAt, [sessionId]: performance.now() },
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
    const state = usePtyExits.getState();
    state.record(payload);
    onExit?.(payload);
    // Suppress the toast for user-initiated shutdowns. Consume the
    // marker so a subsequent reuse of the same session id (shouldn't
    // happen — uuid v7 — but cheap insurance) doesn't silently swallow
    // a real crash.
    const expected = state.expectedExits.has(payload.session_id);
    if (expected) {
      const next = new Set(state.expectedExits);
      next.delete(payload.session_id);
      usePtyExits.setState({ expectedExits: next });
      return;
    }
    // Suppress for early-exit "spawn / resume failed" cases. The
    // terminal already shows whatever the failing process printed;
    // a generic "Agent exited (code 1)" toast on top of that is
    // duplicate noise. Threshold = 5 s; long enough to cover slow
    // CLIs that print + exit, short enough that a real mid-session
    // crash still surfaces. Real crashes (after the user has had
    // time to interact) still toast.
    const spawnedAt = state.spawnedAt[payload.session_id];
    const earlyExit =
      spawnedAt !== undefined && payload.at - spawnedAt < 5000;
    if (earlyExit) return;
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
