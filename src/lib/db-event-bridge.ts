import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { onDbEvent, type Entity } from "@/lib/events";
import { qk } from "@/query";
import { subscribePtyExits } from "@/stores/pty_exits";

/**
 * Listens to the Rust `db_event` channel and fans out to
 * `queryClient.invalidateQueries(...)`. Renders nothing.
 *
 * Coalesces bursts: a task-create can emit 5+ events in <10ms (1 task
 * + N worktree rows + M ticket rows). Without batching, each event
 * triggers its own round of refetches. We collect entity names for ~16ms
 * (one frame at 60fps) and flush once.
 *
 * The entity → query-key mapping is the one choke-point where the
 * Rust event vocabulary meets the frontend cache keys. Adding a new
 * entity = one new arm here. No per-store glue.
 */
export function DbEventBridge() {
  const qc = useQueryClient();

  useEffect(() => {
    let frame: number | null = null;
    const pending = new Set<Entity>();

    const flush = () => {
      frame = null;
      const entities = Array.from(pending);
      pending.clear();

      const has = (e: Entity) => entities.includes(e);

      if (has("project")) {
        void qc.invalidateQueries({ queryKey: qk.projects() });
        void qc.invalidateQueries({ queryKey: qk.workspaceReposAll() });
      }
      if (has("workspace")) {
        void qc.invalidateQueries({ queryKey: qk.workspaces() });
      }
      if (has("workspace_repo")) {
        void qc.invalidateQueries({ queryKey: qk.workspaceReposAll() });
      }
      if (has("task") || has("task_worktree")) {
        void qc.invalidateQueries({ queryKey: qk.tasksAll() });
      }
      if (has("task_worktree")) {
        void qc.invalidateQueries({ queryKey: qk.taskWorktreesAll() });
        void qc.invalidateQueries({ queryKey: qk.changesAll() });
      }
      // `task_ticket` events come in as kind "task" from the Rust side
      // (see db/repo/task_ticket.rs — composite id, entity = Task).
      // They already invalidate taskTicketsAll via the task branch
      // above. If we ever split them out, add the arm here.
      if (has("task")) {
        void qc.invalidateQueries({ queryKey: qk.taskTicketsAll() });
        void qc.invalidateQueries({
          queryKey: qk.taskTicketsByProviderAll(),
        });
      }
      if (has("project_link")) {
        void qc.invalidateQueries({ queryKey: qk.projectLinksAll() });
      }
    };

    const unlisten = onDbEvent((e) => {
      pending.add(e.entity);
      if (frame === null) {
        frame = window.setTimeout(flush, 16) as unknown as number;
      }
    });
    // Co-located with db_event on purpose: pty_exit + db_event are the
    // two "things Rust tells the UI about" channels and subscribing
    // once keeps the wiring in one place.
    const unlistenPty = subscribePtyExits();
    return () => {
      if (frame !== null) window.clearTimeout(frame);
      unlisten.then((fn) => fn());
      unlistenPty.then((fn) => fn());
    };
  }, [qc]);

  return null;
}
