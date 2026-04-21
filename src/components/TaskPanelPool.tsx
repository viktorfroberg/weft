import { useEffect, useRef, useState } from "react";
import { TaskView } from "./TaskView";

interface Props {
  /** The currently-active task id, or `null` when the user is on a
   *  non-task route (Home, Settings, Project). */
  currentTaskId: string | null;
}

/**
 * Mount-persistent pool of `<TaskView />` instances. Every task the user
 * has visited this session stays mounted (hidden via `display: none`)
 * until the app reloads or the task row is deleted. This is the fix for:
 *
 *   user opens Task A → Claude Code spawns
 *   user navigates to Home
 *   user comes back to Task A
 *   ❌ before:  TaskView remounted, Terminal.tsx's unmount-cleanup killed
 *               the PTY (`terminalKill`), so a NEW Claude session spawned
 *               with empty scrollback
 *   ✅ now:    TaskView stayed mounted in the background, the existing
 *               PTY + xterm.js scrollback are right where you left them
 *
 * Trade-off: each visited task holds an xterm.js renderer + the Rust
 * PTY in memory until the app closes. For typical use (1–5 tasks open
 * at once) this is fine; if it ever becomes a memory issue we can add
 * an LRU cap or evict on `task_delete` db events.
 *
 * Pairs with `router.tsx` where `taskRoute.component` is now a no-op —
 * the URL still drives `currentTaskId` via `useActiveRoute()` in Shell,
 * but the actual TaskView lives here, not under Outlet.
 */
export function TaskPanelPool({ currentTaskId }: Props) {
  // Use state (not a ref) so adding a new id triggers a re-render that
  // mounts the new panel. Refs would let the set grow silently without
  // the JSX learning about it.
  const [visited, setVisited] = useState<string[]>(() =>
    currentTaskId ? [currentTaskId] : [],
  );
  const visitedSetRef = useRef(new Set(visited));

  useEffect(() => {
    if (!currentTaskId) return;
    if (visitedSetRef.current.has(currentTaskId)) return;
    visitedSetRef.current.add(currentTaskId);
    setVisited((prev) => [...prev, currentTaskId]);
  }, [currentTaskId]);

  if (visited.length === 0) return null;

  return (
    <>
      {visited.map((id) => (
        <div
          key={id}
          // `flex flex-col` matches the Outlet wrapper so the inner
          // TaskView's resizable panes get the right height. Hidden
          // panels use `hidden` (display: none) — xterm.js gracefully
          // pauses fitAddon work on zero-size hosts, so this is cheap.
          className={`flex flex-col overflow-hidden ${
            id === currentTaskId ? "flex-1" : "hidden"
          }`}
        >
          <TaskView taskId={id} />
        </div>
      ))}
    </>
  );
}
