import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Dev diagnostics. Routes structured messages through the Tauri
 * `dev_log` command so they land in the Rust `tracing` stream —
 * same terminal + same timestamps as the command-boundary traces
 * from `debug.rs`. Webview `console.warn` doesn't pipe reliably in
 * Tauri v2, which matters when the UI locks up hard enough that
 * DevTools won't open.
 *
 * Stripped to no-ops in production via `import.meta.env.DEV`.
 *
 * Helpers:
 * - `useRenderCount(name, every)` — counts component renders; logs
 *   every Nth.
 * - `traceEvent(name, extra?)` — one-shot breadcrumb.
 * - `useRateCounter(name)` — per-second rate accumulator (e.g. PTY
 *   channel chunks).
 * - `useLifecycleTrace(name)` — mount/unmount markers.
 */

const ENABLED = import.meta.env.DEV;

function send(scope: string, msg: string, meta?: Record<string, unknown>): void {
  if (!ENABLED) return;
  // Fire-and-forget. If the webview's JS is about to freeze, this
  // still hands the IPC message to the Tauri layer before the main
  // thread blocks — so the last things we see in the log are the
  // last things the frontend tried to do.
  void invoke("dev_log", { input: { scope, msg, meta: meta ?? null } }).catch(
    () => {},
  );
}

export function useRenderCount(name: string, every = 20): void {
  const ref = useRef(0);
  ref.current++;
  if (ENABLED && ref.current % every === 0) {
    send("render", name, {
      count: ref.current,
      t: Math.round(performance.now()),
    });
  }
}

export function traceEvent(name: string, extra?: Record<string, unknown>): void {
  send("event", name, {
    ...(extra ?? {}),
    t: Math.round(performance.now()),
  });
}

/**
 * Counter that accumulates over a 1-second window and logs the rate.
 * Returns a stable function; call on every occurrence (e.g. every
 * PTY chunk) and you'll see N/sec when a flood happens.
 */
export function useRateCounter(name: string): () => void {
  const ref = useRef({ count: 0, windowStart: performance.now() });
  return () => {
    if (!ENABLED) return;
    const state = ref.current;
    state.count++;
    const now = performance.now();
    if (now - state.windowStart >= 1000) {
      if (state.count > 0) {
        send("rate", name, {
          perSec: state.count,
          t: Math.round(now),
        });
      }
      state.count = 0;
      state.windowStart = now;
    }
  };
}

/** Mount/unmount breadcrumb. Useful for tracking remounts of heavy
 *  components during a hang. */
export function useLifecycleTrace(name: string): void {
  useEffect(() => {
    if (!ENABLED) return;
    send("mount", name, { t: Math.round(performance.now()) });
    return () => {
      send("unmount", name, { t: Math.round(performance.now()) });
    };
  }, [name]);
}
