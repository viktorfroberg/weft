import { useMemo, useState } from "react";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { useProjects } from "@/stores/projects";
import { useTaskWorktrees } from "@/stores/tasks";
import { taskAddRepo, terminalWrite } from "@/lib/commands";
import { useTerminalTabs } from "@/stores/terminal_tabs";
import { usePtyExits } from "@/stores/pty_exits";

const EMPTY_WORKTREES: never[] = [];

interface Props {
  taskId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function AddRepoToTaskDialog({ taskId, open, onOpenChange }: Props) {
  const { data: projects = [] } = useProjects();
  const { data: attachedList = EMPTY_WORKTREES } = useTaskWorktrees(taskId);
  const attachedIds = useMemo(
    () => new Set(attachedList.map((w) => w.project_id)),
    [attachedList],
  );

  const candidates = useMemo(
    () => projects.filter((p) => !attachedIds.has(p.id)),
    [projects, attachedIds],
  );

  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const onAdd = async (projectId: string) => {
    setBusy(projectId);
    setError(null);
    try {
      const wt = await taskAddRepo({ task_id: taskId, project_id: projectId });
      // If an agent is already running for this task, send `/add-dir` so
      // it picks up the new worktree without losing the conversation.
      // Falls back to a plain "added" toast when no agent is alive.
      const tabs = useTerminalTabs.getState().byTaskId[taskId] ?? [];
      const exits = usePtyExits.getState().bySessionId;
      const liveAgents = tabs.filter(
        (t) => t.kind === "agent" && t.sessionId && !exits[t.sessionId],
      );
      if (liveAgents.length > 0) {
        // `\r` because Claude Code runs the PTY in raw mode where the
        // Enter key arrives as CR; LF alone lands in the input box
        // without submitting.
        const cmd = `/add-dir ${wt.worktree_path}\r`;
        const bytes = new TextEncoder().encode(cmd);
        await Promise.all(
          liveAgents.map((t) =>
            terminalWrite(t.sessionId!, bytes).catch(() => {}),
          ),
        );
        toast.success(`Added ${wt.project_name}`, {
          description: `Sent /add-dir to ${liveAgents.length === 1 ? "the running agent" : `${liveAgents.length} agents`}.`,
        });
      } else {
        toast.success(`Added ${wt.project_name}`, {
          description: wt.worktree_path,
        });
      }
      onOpenChange(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Add repo to task</DialogTitle>
          <DialogDescription>
            Creates a worktree on this task's branch in the selected repo.
            If an agent is already running, weft sends{" "}
            <code className="font-mono">/add-dir</code> to it so the
            conversation continues with the new path attached.
          </DialogDescription>
        </DialogHeader>

        {candidates.length === 0 ? (
          <p className="text-muted-foreground text-sm">
            Every registered repo is already attached to this task.
          </p>
        ) : (
          <ul className="border-border max-h-64 space-y-1 overflow-y-auto rounded border p-1">
            {candidates.map((p) => (
              <li key={p.id}>
                <button
                  type="button"
                  disabled={busy !== null}
                  onClick={() => onAdd(p.id)}
                  className="hover:bg-accent flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm disabled:opacity-50"
                >
                  <span
                    className="inline-block h-2 w-2 shrink-0 rounded-full"
                    style={{
                      background: p.color ?? "var(--muted-foreground)",
                    }}
                  />
                  <span className="truncate">{p.name}</span>
                  <span className="text-muted-foreground ml-auto font-mono text-xs">
                    {p.default_branch}
                  </span>
                  {busy === p.id && (
                    <span className="text-muted-foreground text-xs">
                      adding…
                    </span>
                  )}
                </button>
              </li>
            ))}
          </ul>
        )}

        {error && (
          <p className="text-destructive text-sm" role="alert">
            {error}
          </p>
        )}

        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Close
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
