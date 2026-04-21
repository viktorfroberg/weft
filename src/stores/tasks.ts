import { useQueries, useQuery } from "@tanstack/react-query";
import {
  taskProjectIds,
  tasksList,
  tasksListAll,
  taskWorktreesList,
  type Task,
  type TaskStatus,
  type TaskWorktree,
} from "@/lib/commands";
import { qk } from "@/query";
import { useWorkspaces } from "@/stores/workspaces";

/** Tasks in one workspace. */
export function useTasksForWorkspace(workspaceId: string) {
  return useQuery<Task[]>({
    queryKey: qk.tasks(workspaceId),
    queryFn: () => tasksList(workspaceId),
    enabled: !!workspaceId,
  });
}

/**
 * Tasks across every known workspace as `Record<wsId, Task[]>`. Used
 * by the Shell (for `waitingCount`), Sidebar (nested task rows),
 * Toolbar (active-task lookup), CommandPalette (fuzzy match over all
 * tasks). Depends on `useWorkspaces()` resolving first — empty map
 * until the workspace list lands.
 */
export function useAllTasks(): Record<string, Task[]> {
  const { data: workspaces = [] } = useWorkspaces();
  return useQueries({
    queries: workspaces.map((ws) => ({
      queryKey: qk.tasks(ws.id),
      queryFn: () => tasksList(ws.id),
    })),
    combine: (results) => {
      const byId: Record<string, Task[]> = {};
      workspaces.forEach((ws, i) => {
        byId[ws.id] = results[i]?.data ?? [];
      });
      return byId;
    },
  });
}

/** v1.0.7: flat list of every task, sorted by status rank (Working →
 * Waiting → Idle → Done) then by `created_at DESC` within each bucket.
 * Drives the sidebar + Home dashboard + ⌘1-9 task jumping. */
const STATUS_RANK: Record<TaskStatus, number> = {
  working: 0,
  waiting: 1,
  error: 2,
  idle: 3,
  done: 4,
};

export function useAllTasksFlat(): Task[] {
  const { data = [] } = useQuery<Task[]>({
    // Nested under `tasksAll` so the db-event-bridge's task invalidation
    // covers it (the bridge calls `invalidateQueries({ queryKey: qk.tasksAll() })`
    // which matches any key starting with `["tasks"]`).
    queryKey: [...qk.tasksAll(), "flat"] as const,
    queryFn: () => tasksListAll(),
  });
  // Sort is stable; primary key = status rank, secondary = created_at desc.
  return [...data].sort((a, b) => {
    const rank = STATUS_RANK[a.status] - STATUS_RANK[b.status];
    if (rank !== 0) return rank;
    return b.created_at - a.created_at;
  });
}

/** Project ids a single task currently touches. Invalidated when
 * `task_worktree` events fire (via db-event-bridge). */
export function useTaskProjectIds(taskId: string | null) {
  return useQuery<string[]>({
    queryKey: taskId
      ? [...qk.taskWorktrees(taskId), "project_ids"]
      : (["taskProjectIds", "disabled"] as const),
    queryFn: () => taskProjectIds(taskId as string),
    enabled: !!taskId,
  });
}

/** Worktrees attached to a single task. */
export function useTaskWorktrees(taskId: string) {
  return useQuery<TaskWorktree[]>({
    queryKey: qk.taskWorktrees(taskId),
    queryFn: () => taskWorktreesList(taskId),
    enabled: !!taskId,
  });
}

export type { Task, TaskWorktree };
