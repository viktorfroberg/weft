import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Keep in sync with src-tauri/src/db/events.rs : DB_EVENT_CHANNEL
export const DB_EVENT_CHANNEL = "db_event";

export type Entity =
  | "project"
  | "workspace"
  | "workspace_repo"
  | "task"
  | "task_worktree"
  | "workspace_section"
  | "settings"
  | "project_link"
  | "preset";

export type Op = "insert" | "update" | "delete";

export interface DbEvent {
  entity: Entity;
  id: string;
  op: Op;
}

/**
 * Subscribe to DB writes from the Rust side. Returns an unlisten fn.
 * Usage: `useEffect(() => { const u = onDbEvent(e => ...); return () => u.then(f => f()); }, [])`
 */
export function onDbEvent(
  handler: (event: DbEvent) => void,
): Promise<UnlistenFn> {
  return listen<DbEvent>(DB_EVENT_CHANNEL, (e) => handler(e.payload));
}
