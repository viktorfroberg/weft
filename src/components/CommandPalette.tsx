import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowRight, Folder, GitBranch, Keyboard, Plus, Settings } from "lucide-react";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { useWorkspaces } from "@/stores/workspaces";
import { useAllTasks } from "@/stores/tasks";
import { useNavigateRoute } from "@/lib/active-route";
import { useUi } from "@/stores/ui";
import { rank } from "@/lib/command-palette/ranker";
import type { Task } from "@/lib/commands";

type Item =
  | { kind: "workspace"; id: string; name: string; label: string; hint: string }
  | {
      kind: "task";
      id: string;
      name: string;
      label: string;
      hint: string;
      workspace: string;
    }
  | { kind: "action"; id: string; label: string; hint: string; run: () => void };

/**
 * ⌘K command palette. Fuzzy-ish (substring + prefix-boost) search over
 * workspaces, tasks, and global actions. Keyboard-first: arrows move
 * selection, Enter activates, Esc closes.
 *
 * Ranking: exact prefix of query > word-start match > substring match.
 * No external fuzzy-match lib — the dataset is bounded and the naive
 * ranker is legible.
 */
export function CommandPalette() {
  const open = useUi((s) => s.commandPaletteOpen);
  const setOpen = useUi((s) => s.setCommandPaletteOpen);
  const navigate = useNavigateRoute();
  const { data: workspaces = [] } = useWorkspaces();
  const allTasksByWs = useAllTasks();

  const setCreateWorkspaceOpen = useUi((s) => s.setCreateWorkspaceOpen);
  const setAddProjectOpen = useUi((s) => s.setAddProjectOpen);
  const setShortcutsOpen = useUi((s) => s.setShortcutsOpen);
  const setCreateTaskOpen = useUi((s) => s.setCreateTaskOpen);

  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // Reset on open so the palette always starts fresh.
  useEffect(() => {
    if (open) {
      setQuery("");
      setCursor(0);
      // Focus runs after the Dialog's mount animation. 0ms setTimeout
      // is enough; Radix re-focuses on open too but our input sits
      // alongside its initial focus target.
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open]);

  const tasks = useMemo(() => {
    const rows: Array<{ task: Task; workspaceName: string }> = [];
    for (const ws of workspaces) {
      const list = allTasksByWs[ws.id] ?? [];
      for (const t of list) rows.push({ task: t, workspaceName: ws.name });
    }
    return rows;
  }, [workspaces, allTasksByWs]);

  const items = useMemo<Item[]>(() => {
    const workspaceItems: Item[] = workspaces.map((w) => ({
      kind: "workspace",
      id: w.id,
      name: w.name,
      label: w.name,
      hint: "workspace",
    }));
    const taskItems: Item[] = tasks.map(({ task, workspaceName }) => ({
      kind: "task",
      id: task.id,
      name: task.name,
      label: task.name,
      hint: `${workspaceName} · ${task.branch_name}`,
      workspace: workspaceName,
    }));

    // Curated global actions. Listed even with an empty query so the
    // palette doubles as an index of app-level verbs.
    const actionItems: Item[] = [
      {
        kind: "action",
        id: "new-workspace",
        label: "New workspace",
        hint: "⌘N",
        run: () => setCreateWorkspaceOpen(true),
      },
      {
        kind: "action",
        id: "new-task",
        label: "New task in current workspace",
        hint: "⌘⇧N",
        run: () => setCreateTaskOpen(true),
      },
      {
        kind: "action",
        id: "add-project",
        label: "Add a repo",
        hint: "⌘P",
        run: () => setAddProjectOpen(true),
      },
      {
        kind: "action",
        id: "settings",
        label: "Open settings",
        hint: "",
        run: () => navigate({ kind: "settings" }),
      },
      {
        kind: "action",
        id: "shortcuts",
        label: "Show keyboard shortcuts",
        hint: "⌘/",
        run: () => setShortcutsOpen(true),
      },
    ];

    const all = [...workspaceItems, ...taskItems, ...actionItems];
    return rank(all, query);
  }, [
    workspaces,
    tasks,
    query,
    setCreateWorkspaceOpen,
    setAddProjectOpen,
    setShortcutsOpen,
    setCreateTaskOpen,
    navigate,
  ]);

  // Clamp cursor when the result set shrinks.
  useEffect(() => {
    if (cursor >= items.length) setCursor(Math.max(0, items.length - 1));
  }, [items.length, cursor]);

  const activate = (item: Item) => {
    setOpen(false);
    if (item.kind === "workspace") {
      navigate({ kind: "workspace", id: item.id });
    } else if (item.kind === "task") {
      navigate({ kind: "task", id: item.id });
    } else {
      item.run();
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent
        className="top-[20%] max-w-xl translate-y-0 gap-0 overflow-hidden p-0"
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setCursor((c) => Math.min(items.length - 1, c + 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setCursor((c) => Math.max(0, c - 1));
          } else if (e.key === "Enter") {
            if (items[cursor]) {
              e.preventDefault();
              activate(items[cursor]);
            }
          }
        }}
      >
        <div className="border-border flex items-center gap-2 border-b px-3 py-2">
          <span className="text-muted-foreground font-mono text-[10px]">⌘K</span>
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setCursor(0);
            }}
            placeholder="Jump to workspace, task, or action…"
            className="placeholder:text-muted-foreground/60 flex-1 bg-transparent text-sm outline-none"
          />
        </div>
        <ul className="max-h-[400px] overflow-y-auto py-1">
          {items.length === 0 ? (
            <li className="text-muted-foreground/70 px-4 py-3 text-xs">
              No matches.
            </li>
          ) : (
            items.map((item, idx) => (
              <li key={`${item.kind}:${item.id}`}>
                <button
                  type="button"
                  onClick={() => activate(item)}
                  onMouseEnter={() => setCursor(idx)}
                  className={`flex w-full items-center gap-3 px-3 py-1.5 text-left text-sm ${
                    idx === cursor ? "bg-accent text-accent-foreground" : ""
                  }`}
                >
                  <ItemIcon item={item} />
                  <span className="truncate">{item.label}</span>
                  <span className="text-muted-foreground ml-auto truncate font-mono text-[11px]">
                    {item.hint}
                  </span>
                  {idx === cursor && (
                    <ArrowRight size={12} className="text-muted-foreground" />
                  )}
                </button>
              </li>
            ))
          )}
        </ul>
      </DialogContent>
    </Dialog>
  );
}

function ItemIcon({ item }: { item: Item }) {
  const className = "text-muted-foreground shrink-0";
  if (item.kind === "workspace") return <Folder size={12} className={className} />;
  if (item.kind === "task") return <GitBranch size={12} className={className} />;
  // Action id → glyph. Gives each verb a distinct visual so the palette
  // skim-reads faster than a column of identical `+` marks.
  switch (item.id) {
    case "settings":
      return <Settings size={12} className={className} />;
    case "shortcuts":
      return <Keyboard size={12} className={className} />;
    default:
      return <Plus size={12} className={className} />;
  }
}

// Ranker lives in `src/lib/command-palette/ranker.ts` — kept here
// was too much logic for a presentational component.
