import { useMemo, useState } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import {
  ExternalLink,
  FileText,
  PanelRightClose,
  PanelRightOpen,
  Plus,
  Trash2,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { TerminalTabStrip } from "../TerminalTabStrip";
import { ChangesPanel } from "../ChangesPanel";
import { AddRepoToTaskDialog } from "../AddRepoToTaskDialog";
import { useAllTasksFlat, useTaskWorktrees } from "@/stores/tasks";
import { useTerminalTabs } from "@/stores/terminal_tabs";
import { usePtyExits } from "@/stores/pty_exits";
import { useNavigateRoute } from "@/lib/active-route";
import { useUi } from "@/stores/ui";
import { useWorkspaces } from "@/stores/workspaces";
import { useProjects } from "@/stores/projects";
import {
  taskChangesByRepo,
  taskDelete,
  taskOpenInEditor,
  taskRemoveRepo,
} from "@/lib/commands";
import { useLifecycleTrace, useRenderCount } from "@/lib/dev-trace";
import { useConfirm } from "../ConfirmDialog";
import { toast } from "sonner";
import { TicketsStrip } from "./TicketsStrip";
import { ContextDialog } from "./ContextDialog";
import { InlineTaskRename } from "../InlineTaskRename";

// Stable module-level empty array — fallback for Zustand selectors so we
// don't return a new `[]` every render. See CLAUDE.md "Critical patterns"
// for the getSnapshot-should-be-cached infinite-loop rationale.
const EMPTY_WORKTREES: never[] = [];

interface Props {
  taskId: string;
}

export function TaskView({ taskId }: Props) {
  useLifecycleTrace(`TaskView(${taskId})`);
  useRenderCount(`TaskView(${taskId})`, 20);
  // v1.0.7: tasks may be ad-hoc (workspace_id = null) — those land only
  // in the flat `tasksListAll` query, never in any per-workspace bucket.
  // Using `useAllTasks()` (workspace-keyed `useQueries`) here used to
  // strand new ad-hoc tasks as "Task not found" right after creation.
  const allTasks = useAllTasksFlat();
  const task = useMemo(
    () => allTasks.find((t) => t.id === taskId),
    [allTasks, taskId],
  );
  const { data: workspaces = [] } = useWorkspaces();
  const workspace = task
    ? workspaces.find((w) => w.id === task.workspace_id)
    : undefined;
  const { data: worktrees = EMPTY_WORKTREES } = useTaskWorktrees(taskId);
  const navigate = useNavigateRoute();
  const changesPanelVisible = useUi((s) => s.changesPanelVisible);
  const setChangesPanelVisible = useUi((s) => s.setChangesPanelVisible);
  const confirm = useConfirm();
  const { data: projects = [] } = useProjects();
  const [addRepoOpen, setAddRepoOpen] = useState(false);
  const [contextOpen, setContextOpen] = useState(false);

  // Any agent currently alive for this task? Drives the "remove repo"
  // and ticket-Unlink affordances: once Claude has the repo/ticket in
  // its context, silently deleting it here would desync the conversation
  // from disk state. Better to force Reload (↻ on the agent tab) first.
  const agentIsLive = useTerminalTabs((s) => {
    const tabs = s.byTaskId[taskId] ?? [];
    const exits = usePtyExits.getState().bySessionId;
    return tabs.some(
      (t) => t.kind === "agent" && t.sessionId && !exits[t.sessionId],
    );
  });

  const onOpenInEditor = async () => {
    try {
      await taskOpenInEditor(taskId);
    } catch (e) {
      await confirm({
        title: "Couldn't open editor",
        description:
          String(e) +
          "\n\nMake sure `code` (or your chosen editor command) is on PATH. In VS Code / Cursor: Command Palette → 'Install \"code\" / \"cursor\" command in PATH'.",
        confirmText: "OK",
        cancelText: "",
      });
    }
  };

  const onRemoveRepo = async (projectId: string) => {
    const project = projects.find((p) => p.id === projectId);
    const name = project?.name ?? "this repo";
    const ok = await confirm({
      title: `Remove ${name} from this task?`,
      description:
        "The worktree will be deleted from disk. If the agent has an open file there, it'll lose access.",
      confirmText: "Remove",
      destructive: true,
    });
    if (!ok) return;
    try {
      await taskRemoveRepo(taskId, projectId);
    } catch (e) {
      await confirm({
        title: "Couldn't remove repo",
        description: String(e),
        confirmText: "OK",
        cancelText: "",
      });
    }
  };

  // Note: Launch is owned by the app Toolbar (same route-kind === "task"
  // branch). ⌘L shortcut also fires directly via `launchDefaultAgent`
  // in Shell.tsx — this view no longer re-implements the button.

  const primaryWorktree = useMemo(
    () => worktrees.find((w) => w.status === "ready") ?? worktrees[0],
    [worktrees],
  );

  const readyCount = useMemo(
    () => worktrees.filter((w) => w.status === "ready").length,
    [worktrees],
  );

  if (!task) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
        Task not found. Pick another from the sidebar or press ⌘⇧O.
      </div>
    );
  }

  const backTarget = workspace
    ? { kind: "workspace" as const, id: workspace.id }
    : { kind: "home" as const };

  const onDelete = async () => {
    // Check for uncommitted work before destroying worktrees. Costs one
    // Rust call that shells out to `git status --porcelain` per worktree
    // — worth the friction of an extra confirm variant when it prevents
    // stranded work.
    let dirtyDescription: string | null = null;
    try {
      const rows = await taskChangesByRepo(task.id);
      const dirty = rows.filter((r) => r.changes.length > 0);
      if (dirty.length > 0) {
        const summary = dirty
          .map((r) => {
            const p = projects.find((pp) => pp.id === r.project_id);
            return `${p?.name ?? r.project_id}: ${r.changes.length} file${r.changes.length === 1 ? "" : "s"}`;
          })
          .join("\n");
        dirtyDescription =
          `Uncommitted changes will be lost:\n\n${summary}\n\n` +
          "Commit or discard them first, or confirm to throw them away along with the worktrees.";
      }
    } catch {
      // Network / git errors here shouldn't block delete.
    }

    const ok = await confirm({
      title: dirtyDescription
        ? `Delete task "${task.name}" and lose uncommitted work?`
        : `Delete task "${task.name}"?`,
      description:
        dirtyDescription ??
        "This removes its worktrees from disk and stops any running agent sessions.",
      confirmText: dirtyDescription ? "Delete and discard" : "Delete task",
      destructive: true,
    });
    if (!ok) return;
    try {
      const resp = await taskDelete(task.id);
      // No manual refetch: task_delete emits DbEvent("task" delete) +
      // task_worktree deletes, which the App-level event router
      // handles via the coalesced flush.
      navigate(backTarget);
      if (resp.preserved_branches.length > 0) {
        // Branches with unmerged commits were kept on disk — the user
        // can still resume them via CLI. Name the repos so they can
        // find the commits.
        const repos = resp.preserved_branches
          .map((b) => b.project_name)
          .join(", ");
        toast.success(`Deleted task "${task.name}"`, {
          description: `Kept branch in ${repos} (had unmerged commits).`,
          duration: 10_000,
        });
      } else {
        toast.success(`Deleted task "${task.name}"`);
      }
    } catch (err) {
      await confirm({
        title: "Failed to delete task",
        description: String(err),
        confirmText: "OK",
        cancelText: "",
      });
    }
  };

  // Terminal / changes panel split. When the changes panel is
  // hidden, the terminal group becomes single-panel at 100%. When
  // visible, a 60/40 default split plus user-draggable Separator.
  const terminalSize = 60;
  const changesSize = 40;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Title row — the task name used to cram into the app-level
          Toolbar breadcrumb alongside the drag-region / traffic lights,
          which felt chaotic for anything longer than a few words. It
          lives here now as a proper page title, with double-click
          rename wired through `InlineTaskRename` (sets `name_locked_at`
          so the background `claude -p` auto-rename won't overwrite
          user intent). */}
      <div className="border-border flex items-center gap-2 border-b px-6 py-3">
        <InlineTaskRename
          task={task}
          className="text-foreground truncate text-lg font-semibold"
        />
      </div>

      {/* Tickets strip — external context for this task (Linear tickets
          in v1.0.2). Titles fetched live; chip degrades to ID-only when
          offline / deleted / unauthorized. */}
      <TicketsStrip taskId={taskId} />

      {/* Single meta row — worktree pills on the left, secondary
          actions on the right. The app-level Toolbar already shows the
          breadcrumb + Launch, and the shell prompt already shows the
          branch name, so we don't repeat them here. */}
      <div className="border-border flex items-center gap-1.5 overflow-x-auto border-b px-6 py-1.5 text-xs">
        {worktrees.length === 0 ? (
          <span className="text-muted-foreground">
            No repos attached. Add one to start working.
          </span>
        ) : (
          worktrees.map((w, i) => {
            const project = projects.find((p) => p.id === w.project_id);
            const color = project?.color ?? "var(--muted-foreground)";
            const degraded = w.status !== "ready";
            return (
              <span
                key={`${w.task_id}:${w.project_id}`}
                className={`group inline-flex shrink-0 items-center gap-1.5 px-1 py-0.5 ${
                  degraded ? "text-destructive/80" : "text-muted-foreground"
                }`}
                title={`${w.worktree_path}${i < 9 ? ` · ⌘${i + 1} focuses its diff` : ""}`}
              >
                <span
                  className="inline-block h-1.5 w-1.5 rounded-full"
                  style={{ background: color }}
                />
                <span className="text-foreground">
                  {project?.name ?? w.project_id.slice(0, 6)}
                </span>
                {i < 9 && (
                  <span className="text-muted-foreground/50 font-mono text-[10px]">
                    ⌘{i + 1}
                  </span>
                )}
                {degraded && (
                  <span className="text-destructive font-mono text-[10px]">
                    {w.status}
                  </span>
                )}
                {!agentIsLive && (
                  <button
                    type="button"
                    onClick={() => onRemoveRepo(w.project_id)}
                    className="text-muted-foreground hover:text-destructive ml-0.5 opacity-0 transition-opacity group-hover:opacity-70"
                    title={`Remove ${project?.name ?? "repo"} from task`}
                  >
                    <X size={10} />
                  </button>
                )}
              </span>
            );
          })
        )}
        <button
          type="button"
          onClick={() => setAddRepoOpen(true)}
          className="text-muted-foreground hover:bg-accent hover:text-foreground ml-1 inline-flex shrink-0 items-center gap-1 rounded px-2 py-0.5 transition-colors"
          title="Add repo to task"
        >
          <Plus size={12} />
          Add repo
        </button>
        {/* Degraded-only readiness hint. When all worktrees report
            ready, the dots + worktree pills already convey that; the
            "2/2 ready" badge was redundant chrome. */}
        {worktrees.length > 0 && readyCount < worktrees.length && (
          <span className="text-destructive ml-2 shrink-0 font-mono text-[10px]">
            {readyCount}/{worktrees.length} ready
          </span>
        )}
        <div className="ml-auto flex shrink-0 items-center gap-0.5">
          <Button
            size="sm"
            variant="ghost"
            onClick={() => setContextOpen(true)}
            title="Edit .weft/context.md — agent hints seeded into every worktree"
            className="text-muted-foreground hover:text-foreground h-6 w-6 p-0"
            aria-label="Edit agent context"
          >
            <FileText size={12} />
          </Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={onOpenInEditor}
            title="Open in editor (VS Code / Cursor) — multi-root workspace"
            className="text-muted-foreground hover:text-foreground h-6 w-6 p-0"
            aria-label="Open in editor"
          >
            <ExternalLink size={12} />
          </Button>
          {/* Toggle the right-side changes panel. Hiding gives the
              terminal the full workspace while you work with the
              agent; showing restores the 60/40 split. (⌘\) */}
          <Button
            size="sm"
            variant="ghost"
            onClick={() => setChangesPanelVisible(!changesPanelVisible)}
            title={
              changesPanelVisible
                ? "Hide changes panel (⌘\\)"
                : "Show changes panel (⌘\\)"
            }
            aria-label={
              changesPanelVisible ? "Hide changes panel" : "Show changes panel"
            }
            className="text-muted-foreground hover:text-foreground h-6 w-6 p-0"
          >
            {changesPanelVisible ? (
              <PanelRightClose size={12} />
            ) : (
              <PanelRightOpen size={12} />
            )}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={onDelete}
            className="text-muted-foreground hover:text-destructive ml-1 h-6 w-6 p-0"
            title="Delete task"
            aria-label="Delete task"
          >
            <Trash2 size={12} />
          </Button>
        </div>
      </div>

      <section className="flex-1 overflow-hidden">
        {primaryWorktree ? (
          <Group
            key={changesPanelVisible ? "split" : "solo"}
            orientation="horizontal"
            className="flex h-full w-full"
          >
            <Panel defaultSize={changesPanelVisible ? terminalSize : 100} minSize={20}>
              <TerminalTabStrip
                taskId={task.id}
                shellSpawn={{
                  command: import.meta.env.VITE_SHELL ?? "/bin/zsh",
                  args: ["-l"],
                  cwd: primaryWorktree.worktree_path,
                  env: [
                    ["WEFT_TASK_ID", task.id],
                    ["WEFT_TASK_SLUG", task.slug],
                    ["WEFT_TASK_BRANCH", primaryWorktree.task_branch],
                    ["WEFT_HOOKS_URL", "http://127.0.0.1:17293/v1/events"],
                  ],
                }}
              />
            </Panel>
            {changesPanelVisible && (
              <>
                {/* Visible drag handle — 4px wide with a subtle vertical
                    line that brightens on hover. Discoverable; matches
                    Zed/Warp. */}
                <Separator className="group bg-transparent hover:bg-accent relative w-1.5 cursor-col-resize transition-colors">
                  <span className="bg-border group-hover:bg-foreground/60 absolute inset-y-0 left-1/2 w-[1px] -translate-x-1/2 transition-colors" />
                </Separator>
                <Panel defaultSize={changesSize} minSize={20}>
                  <ChangesPanel taskId={task.id} />
                </Panel>
              </>
            )}
          </Group>
        ) : (
          <div className="flex flex-col items-center justify-center gap-3 p-6 text-sm">
            <p className="text-muted-foreground/80">
              No ready worktree to attach a terminal to. Add a repo to
              start, or delete this task.
            </p>
            <div className="flex gap-2">
              <Button
                size="sm"
                variant="outline"
                onClick={() => setAddRepoOpen(true)}
              >
                <Plus size={12} />
                Add repo
              </Button>
              <Button size="sm" variant="ghost" onClick={onDelete}>
                Delete task
              </Button>
            </div>
          </div>
        )}
      </section>

      <AddRepoToTaskDialog
        taskId={task.id}
        open={addRepoOpen}
        onOpenChange={setAddRepoOpen}
      />
      <ContextDialog
        taskId={task.id}
        open={contextOpen}
        onOpenChange={setContextOpen}
      />
    </div>
  );
}
