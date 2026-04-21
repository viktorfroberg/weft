import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
import { useUi } from "@/stores/ui";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Textarea } from "@/components/ui/textarea";
// Lazy — pulls in Monaco (~300 KB) only when the user actually opens a
// file diff. Keeps the initial ChangesPanel render cheap and the main
// bundle light.
const DiffViewer = lazy(() =>
  import("./DiffViewer").then((m) => ({ default: m.DiffViewer })),
);
import { useChanges, useRefetchChanges } from "@/stores/changes";
import { useProjects } from "@/stores/projects";
import { useLifecycleTrace, useRenderCount } from "@/lib/dev-trace";
import {
  taskCommitAll,
  worktreeCommit,
  worktreeDiscard,
  type CommitResult,
  type FileChange,
  type RepoChanges,
} from "@/lib/commands";
import { useConfirm } from "./ConfirmDialog";

interface Props {
  taskId: string;
}

type SelectedFile = {
  repo: RepoChanges;
  change: FileChange;
};

const KIND_LETTER: Record<FileChange["kind"], string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  copied: "C",
  untracked: "?",
  conflicted: "!",
  type_changed: "T",
  other: "•",
};

const KIND_COLOR: Record<FileChange["kind"], string> = {
  added: "text-emerald-400",
  modified: "text-amber-400",
  deleted: "text-red-400",
  renamed: "text-sky-400",
  copied: "text-sky-400",
  untracked: "text-muted-foreground",
  conflicted: "text-red-400",
  type_changed: "text-amber-400",
  other: "text-muted-foreground",
};

export function ChangesPanel({ taskId }: Props) {
  useLifecycleTrace(`ChangesPanel(${taskId})`);
  useRenderCount(`ChangesPanel(${taskId})`, 20);
  const { data: rows, isFetching: loading } = useChanges(taskId);
  const refetchFn = useRefetchChanges();
  const refetch = (id: string) => refetchFn(id);
  const { data: projects = [] } = useProjects();

  const [selected, setSelected] = useState<SelectedFile | null>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [message, setMessage] = useState("");
  const [lastResults, setLastResults] = useState<
    Record<string, CommitResult>
  >({});
  const [committing, setCommitting] = useState(false);
  const confirm = useConfirm();
  const focusRepoId = useUi((s) => s.focusRepoId);
  const setFocusRepoId = useUi((s) => s.setFocusRepoId);
  const sectionRefs = useRef<Record<string, HTMLElement | null>>({});

  // ⌘1–⌘9 triggers a focus request via `ui.focusRepoId`. Scroll that
  // section into view + briefly flash it, then clear the flag so
  // subsequent renders don't re-scroll.
  useEffect(() => {
    if (!focusRepoId) return;
    const el = sectionRefs.current[focusRepoId];
    if (el) {
      el.scrollIntoView({ behavior: "smooth", block: "start" });
      el.classList.add("ring-2", "ring-accent-foreground/30");
      setTimeout(() => {
        el.classList.remove("ring-2", "ring-accent-foreground/30");
      }, 800);
    }
    setFocusRepoId(null);
  }, [focusRepoId, setFocusRepoId]);

  const totalFiles = useMemo(
    () => (rows ?? []).reduce((n, r) => n + r.changes.length, 0),
    [rows],
  );

  const reposWithChanges = useMemo(
    () => (rows ?? []).filter((r) => r.changes.length > 0),
    [rows],
  );

  const toggle = (projectId: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(projectId)) next.delete(projectId);
      else next.add(projectId);
      return next;
    });
  };

  const handleResults = (results: CommitResult[]) => {
    setLastResults((prev) => {
      const next = { ...prev };
      for (const r of results) next[r.project_id] = r;
      return next;
    });
  };

  const onCommitAll = async () => {
    if (!message.trim() || reposWithChanges.length === 0) return;
    setCommitting(true);
    try {
      const results = await taskCommitAll(taskId, message.trim());
      handleResults(results);
      // Only clear the message when EVERY repo succeeded. If any failed,
      // the user needs to retry per-repo with the same message still in
      // the box — previously we cleared it and they had to retype.
      const allSucceeded = results.every((r) => r.ok);
      if (allSucceeded) setMessage("");
      await refetch(taskId);
    } catch (err) {
      await confirm({
        title: "Commit all failed",
        description: String(err),
        confirmText: "OK",
        cancelText: "",
      });
    } finally {
      setCommitting(false);
    }
  };

  const onCommitRepo = async (repo: RepoChanges) => {
    if (!message.trim()) return;
    setCommitting(true);
    try {
      const result = await worktreeCommit(
        repo.project_id,
        repo.worktree_path,
        message.trim(),
      );
      handleResults([result]);
      if (result.ok) setMessage("");
      await refetch(taskId);
    } finally {
      setCommitting(false);
    }
  };

  const onDiscardRepo = async (repo: RepoChanges) => {
    const project = projects.find((p) => p.id === repo.project_id);
    const name = project?.name ?? "this repo";
    const ok = await confirm({
      title: `Discard changes in ${name}?`,
      description:
        "Tracked files revert to HEAD, untracked files are deleted. This cannot be undone.",
      confirmText: "Discard",
      destructive: true,
    });
    if (!ok) return;
    try {
      await worktreeDiscard(repo.worktree_path);
      setLastResults((prev) => {
        const next = { ...prev };
        delete next[repo.project_id];
        return next;
      });
      await refetch(taskId);
    } catch (err) {
      await confirm({
        title: `Failed to discard changes in ${name}`,
        description: String(err),
        confirmText: "OK",
        cancelText: "",
      });
    }
  };

  if (selected) {
    return (
      <Suspense
        fallback={
          <div className="text-muted-foreground flex h-full items-center justify-center text-sm">
            Loading diff viewer…
          </div>
        }
      >
        <DiffViewer
          worktreePath={selected.repo.worktree_path}
          baseBranch={selected.repo.base_branch}
          change={selected.change}
          onClose={() => setSelected(null)}
        />
      </Suspense>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <header className="border-border flex items-center justify-between border-b px-3 py-2">
        <div className="flex items-center gap-2">
          <h3 className="text-xs font-medium uppercase tracking-wider">
            Changes
          </h3>
          <span className="text-muted-foreground text-xs">
            {totalFiles} file{totalFiles === 1 ? "" : "s"}
            {reposWithChanges.length > 0 &&
              ` · ${reposWithChanges.length} repo${reposWithChanges.length === 1 ? "" : "s"}`}
          </span>
        </div>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => refetch(taskId)}
          disabled={loading}
          className="text-xs"
        >
          {loading ? "Refreshing…" : "Refresh"}
        </Button>
      </header>

      {reposWithChanges.length > 0 && (
        <div className="border-border space-y-2 border-b px-3 py-2">
          <Textarea
            placeholder="Commit message…&#10;&#10;First line = subject, blank line, body."
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                onCommitAll();
              }
            }}
            className="min-h-[60px] resize-y font-mono text-xs"
            rows={3}
          />
          <div className="flex items-center justify-between gap-2">
            <Button
              size="sm"
              onClick={onCommitAll}
              disabled={committing || !message.trim()}
              className="flex-1 text-xs"
            >
              {committing
                ? "Committing…"
                : `Commit all (${reposWithChanges.length})`}
            </Button>
            <span className="text-muted-foreground text-[10px]">
              ⌘↵
            </span>
          </div>
        </div>
      )}

      <div className="flex-1 overflow-y-auto">
        {!rows && loading && (
          <div className="text-muted-foreground p-4 text-sm">Loading…</div>
        )}
        {rows && rows.length === 0 && (
          <div className="text-muted-foreground/80 p-4 text-sm">
            No worktrees to inspect.
          </div>
        )}
        {rows && rows.length > 0 && totalFiles === 0 && (
          <div className="text-muted-foreground/80 px-3 py-2 text-xs">
            No changes yet.
          </div>
        )}
        {rows?.map((repo) => {
          const project = projects.find((p) => p.id === repo.project_id);
          // Repos with no changes auto-collapse unless the user has
          // explicitly expanded them (`collapsed` set holds repos the user
          // toggled — we XOR against the default so "clean" rows stay
          // quiet).
          const hasChanges = repo.changes.length > 0;
          const userToggled = collapsed.has(repo.project_id);
          const isCollapsed = hasChanges ? userToggled : !userToggled;
          const result = lastResults[repo.project_id];
          return (
            <section
              key={repo.project_id}
              ref={(el) => {
                sectionRefs.current[repo.project_id] = el;
              }}
              className="border-border/70 border-b transition-shadow last:border-b-0"
            >
              <div className="flex items-center gap-1 px-3 py-2">
                <button
                  type="button"
                  onClick={() => toggle(repo.project_id)}
                  className="hover:bg-accent flex min-w-0 flex-1 items-center gap-2 rounded py-0.5 text-left text-sm"
                >
                  <span className="text-muted-foreground w-2 text-xs">
                    {isCollapsed ? "▸" : "▾"}
                  </span>
                  <span
                    className="inline-block h-2 w-2 shrink-0 rounded-full"
                    style={{
                      background:
                        project?.color ?? "var(--muted-foreground)",
                    }}
                  />
                  <span className="truncate font-medium">
                    {project?.name ?? repo.project_id}
                  </span>
                  <Badge
                    variant="secondary"
                    className="font-mono text-[10px]"
                  >
                    {repo.changes.length}
                  </Badge>
                  {repo.error && (
                    <Badge variant="destructive" className="text-[10px]">
                      error
                    </Badge>
                  )}
                  {result?.ok && result.sha && (
                    <Badge
                      variant="secondary"
                      className="bg-emerald-900/40 font-mono text-[10px] text-emerald-300"
                      title={result.sha}
                    >
                      ✓ {result.sha.slice(0, 7)}
                    </Badge>
                  )}
                  {result && !result.ok && result.error && (
                    <Badge
                      variant="destructive"
                      className="text-[10px]"
                      title={result.error}
                    >
                      failed
                    </Badge>
                  )}
                </button>
                {repo.changes.length > 0 && (
                  <>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => onCommitRepo(repo)}
                      disabled={committing || !message.trim()}
                      className="h-7 px-2 text-xs"
                      title={
                        !message.trim()
                          ? "Enter a commit message above"
                          : "Commit just this repo"
                      }
                    >
                      Commit
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => onDiscardRepo(repo)}
                      className="text-muted-foreground hover:text-destructive h-7 px-2 text-xs"
                    >
                      Discard
                    </Button>
                  </>
                )}
              </div>
              {result && !result.ok && result.error && !isCollapsed && (
                <div className="text-destructive px-8 pb-2 font-mono text-xs">
                  {result.error}
                </div>
              )}
              {!isCollapsed && (
                <>
                  {repo.error && (
                    <div className="text-destructive px-3 pb-2 text-xs">
                      {repo.error}
                    </div>
                  )}
                  {repo.changes.length === 0 && !repo.error && (
                    <div className="text-muted-foreground/70 px-8 pb-2 text-xs">
                      clean
                    </div>
                  )}
                  <ul>
                    {repo.changes.map((change) => (
                      <li key={change.path}>
                        <button
                          type="button"
                          onClick={() =>
                            setSelected({ repo, change })
                          }
                          className="hover:bg-accent flex w-full items-center gap-2 px-8 py-1 text-left font-mono text-xs"
                        >
                          <span
                            className={`w-3 shrink-0 text-center ${KIND_COLOR[change.kind]}`}
                          >
                            {KIND_LETTER[change.kind]}
                          </span>
                          <span
                            className="truncate"
                            title={change.path}
                          >
                            {change.path}
                          </span>
                        </button>
                      </li>
                    ))}
                  </ul>
                </>
              )}
            </section>
          );
        })}
      </div>
    </div>
  );
}
