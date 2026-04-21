import { useEffect, useMemo, useState } from "react";
import { GitBranch } from "lucide-react";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { useAllTasks } from "@/stores/tasks";
import { useWorkspaces } from "@/stores/workspaces";
import { useNavigateRoute } from "@/lib/active-route";
import { useUi } from "@/stores/ui";
import { usePrefs } from "@/stores/prefs";
import { TaskStatusDot } from "./ui/task-status-dot";
import type { Task } from "@/lib/commands";

const DISPLAY_LIMIT = 7;

/**
 * ⌘⇧O recent-tasks palette. Narrow, focused — just the last 7 tasks
 * the user opened, newest first. Separate from ⌘K so that "get me back
 * to the task I was just on" is one keystroke away without scrolling.
 * Arrow keys navigate, Enter selects, Esc closes (handled by Dialog).
 */
export function RecentTasksPalette() {
  const open = useUi((s) => s.recentTasksOpen);
  const setOpen = useUi((s) => s.setRecentTasksOpen);
  const navigate = useNavigateRoute();
  const recentIds = usePrefs((s) => s.recentTaskIds);
  const byWorkspaceId = useAllTasks();
  const { data: workspaces = [] } = useWorkspaces();
  const [cursor, setCursor] = useState(0);

  useEffect(() => {
    if (open) setCursor(0);
  }, [open]);

  const rows = useMemo(() => {
    const allTasks: Array<{ task: Task; workspaceName: string }> = [];
    for (const ws of workspaces) {
      const list = byWorkspaceId[ws.id] ?? [];
      for (const t of list) allTasks.push({ task: t, workspaceName: ws.name });
    }
    const byId = new Map(allTasks.map((r) => [r.task.id, r]));
    // Preserve MRU order; skip ids that no longer resolve (deleted).
    const ordered = recentIds
      .map((id) => byId.get(id))
      .filter((r): r is NonNullable<typeof r> => !!r)
      .slice(0, DISPLAY_LIMIT);
    return ordered;
  }, [recentIds, workspaces, byWorkspaceId]);

  useEffect(() => {
    if (cursor >= rows.length) setCursor(Math.max(0, rows.length - 1));
  }, [rows.length, cursor]);

  const activate = (taskId: string) => {
    setOpen(false);
    navigate({ kind: "task", id: taskId });
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent
        className="top-[22%] max-w-md translate-y-0 gap-0 overflow-hidden p-0"
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setCursor((c) => Math.min(rows.length - 1, c + 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setCursor((c) => Math.max(0, c - 1));
          } else if (e.key === "Enter" && rows[cursor]) {
            e.preventDefault();
            activate(rows[cursor].task.id);
          }
        }}
        // Radix focuses the first tabbable child on open; we want focus
        // on the Dialog itself so ArrowDown works immediately. This
        // attribute makes the Dialog content focusable.
        tabIndex={-1}
      >
        <div className="border-border flex items-center gap-2 border-b px-3 py-2">
          <span className="text-muted-foreground font-mono text-[10px]">
            ⌘⇧O
          </span>
          <span className="text-sm">Recent tasks</span>
        </div>
        {rows.length === 0 ? (
          <div className="text-muted-foreground/70 px-4 py-3 text-xs">
            No recent tasks yet. Create or open one to populate this list.
          </div>
        ) : (
          <ul className="py-1">
            {rows.map(({ task, workspaceName }, idx) => (
              <li key={task.id}>
                <button
                  type="button"
                  onClick={() => activate(task.id)}
                  onMouseEnter={() => setCursor(idx)}
                  className={`flex w-full items-center gap-3 px-3 py-2 text-left text-sm ${
                    idx === cursor ? "bg-accent text-accent-foreground" : ""
                  }`}
                >
                  <TaskStatusDot status={task.status} size="sm" />
                  <GitBranch size={12} className="text-muted-foreground shrink-0" />
                  <span className="truncate">{task.name}</span>
                  <span className="text-muted-foreground ml-auto truncate font-mono text-[11px]">
                    {workspaceName} · {task.branch_name}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </DialogContent>
    </Dialog>
  );
}
